use std::ffi::{CString, c_void};
use std::path::Path;
use std::sync::Arc;

use crate::cuda::bindings::CudaDriver;
use crate::cuda::buffer::GpuBuffer;
use crate::cuda::context::CudaContext;
use crate::cuda::device::GpuDevice;
use crate::cuda::stream::CudaStream;
use crate::error::TensorRtError;
use crate::precision::PrecisionTarget;
use crate::tensorrt::bindings::TensorRtLibrary;
use crate::tensorrt::shim;

/// ERB bands of the encoder input and of the ERB mask.
pub const NB_ERB: usize = 32;
/// Complex bins processed by deep filtering.
pub const NB_DF: usize = 96;
/// Deep filtering FIR order.
pub const DF_ORDER: usize = 5;
/// Values per hop in [`Dfn3Output::df_coefs`]: `NB_DF` bins x `DF_ORDER` taps x (re, im).
pub const DF_COEFS_LEN: usize = NB_DF * DF_ORDER * 2;

/// File names of the three stateful engine plans.
pub const ENC_ENGINE_FILE: &str = "enc.engine";
pub const ERB_DEC_ENGINE_FILE: &str = "erb_dec.engine";
pub const DF_DEC_ENGINE_FILE: &str = "df_dec.engine";

/// Result of one streaming hop.
#[derive(Debug, Clone, PartialEq)]
pub struct Dfn3Output {
    /// Local SNR estimate in dB.
    pub lsnr: f32,
    /// ERB mask gains, one per band.
    pub erb_mask: [f32; NB_ERB],
    /// Deep filter coefficients, `[bin][tap][re, im]` flattened.
    pub df_coefs: [f32; DF_COEFS_LEN],
}

impl Dfn3Output {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lsnr: 0.0,
            erb_mask: [0.0; NB_ERB],
            df_coefs: [0.0; DF_COEFS_LEN],
        }
    }
}

impl Default for Dfn3Output {
    fn default() -> Self {
        Self::new()
    }
}

/// Serialized TensorRT engine bytes for DeepFilterNet3.
#[derive(Clone)]
pub struct Dfn3Engines {
    pub enc: Vec<u8>,
    pub erb_dec: Vec<u8>,
    pub df_dec: Vec<u8>,
}

impl Dfn3Engines {
    /// Reads `enc.engine`, `erb_dec.engine` and `df_dec.engine` from `dir` or `dir/tensorrt`.
    pub fn read_dir(dir: &Path) -> Result<Self, TensorRtError> {
        let search_dirs = [dir.join("tensorrt"), dir.to_path_buf()];

        let read_engine = |name: &str| -> Result<Vec<u8>, TensorRtError> {
            for d in &search_dirs {
                let candidate = d.join(name);
                if candidate.exists() {
                    return std::fs::read(&candidate).map_err(|e| {
                        TensorRtError::ModelLoadFailed(format!(
                            "failed to read {}: {e}",
                            candidate.display()
                        ))
                    });
                }
            }
            Err(TensorRtError::ModelLoadFailed(format!(
                "engine file {name} not found in {}",
                dir.display()
            )))
        };

        Ok(Self {
            enc: read_engine(ENC_ENGINE_FILE)?,
            erb_dec: read_engine(ERB_DEC_ENGINE_FILE)?,
            df_dec: read_engine(DF_DEC_ENGINE_FILE)?,
        })
    }
}

/// Device-bound streaming DeepFilterNet3 session executing on an NVIDIA GPU via TensorRT.
pub struct TensorRtDfn3Session {
    device_ordinal: u32,
    device: GpuDevice,
    precision: PrecisionTarget,
    _trt_lib: Arc<TensorRtLibrary>,
    _cuda_driver: Arc<CudaDriver>,
    cuda_ctx: CudaContext,
    stream: CudaStream,
    runtime: *mut c_void,
    enc_engine: *mut c_void,
    enc_ctx: *mut c_void,
    erb_engine: *mut c_void,
    erb_ctx: *mut c_void,
    df_engine: *mut c_void,
    df_ctx: *mut c_void,
    // GPU buffers for encoder inputs & states
    feat_erb: GpuBuffer<f32>,
    feat_spec: GpuBuffer<f32>,
    enc_h: GpuBuffer<f32>,
    enc_erb_buf: GpuBuffer<f32>,
    enc_spec_buf: GpuBuffer<f32>,
    // GPU buffers for encoder outputs / intermediate embeddings
    #[allow(dead_code)]
    enc_e0: GpuBuffer<f32>,
    #[allow(dead_code)]
    enc_e1: GpuBuffer<f32>,
    #[allow(dead_code)]
    enc_e2: GpuBuffer<f32>,
    #[allow(dead_code)]
    enc_e3: GpuBuffer<f32>,
    #[allow(dead_code)]
    enc_emb: GpuBuffer<f32>,
    #[allow(dead_code)]
    enc_c0: GpuBuffer<f32>,
    enc_lsnr: GpuBuffer<f32>,
    enc_h_out: GpuBuffer<f32>,
    enc_erb_buf_out: GpuBuffer<f32>,
    enc_spec_buf_out: GpuBuffer<f32>,
    // GPU buffers for ERB decoder
    erb_h: GpuBuffer<f32>,
    erb_m: GpuBuffer<f32>,
    erb_h_out: GpuBuffer<f32>,
    // GPU buffers for DF decoder
    df_h: GpuBuffer<f32>,
    df_c0_buf: GpuBuffer<f32>,
    df_coefs: GpuBuffer<f32>,
    #[allow(dead_code)]
    df_aux: GpuBuffer<f32>,
    df_h_out: GpuBuffer<f32>,
    df_c0_buf_out: GpuBuffer<f32>,
    frames_processed: u64,
}

// SAFETY: TensorRT execution contexts and GPU buffers are bound to their thread-safe CUDA context.
unsafe impl Send for TensorRtDfn3Session {}
unsafe impl Sync for TensorRtDfn3Session {}

impl TensorRtDfn3Session {
    /// Loads the three TensorRT engine plans from `model_dir` and initializes inference on `device_ordinal`.
    pub fn load(model_dir: &Path, device_ordinal: u32) -> Result<Self, TensorRtError> {
        let engines = Dfn3Engines::read_dir(model_dir)?;
        Self::load_engines(&engines, device_ordinal)
    }

    /// Initializes a streaming session on `device_ordinal` using the provided engine bytes.
    pub fn load_engines(engines: &Dfn3Engines, device_ordinal: u32) -> Result<Self, TensorRtError> {
        if !shim::is_trt_shim_compiled() {
            return Err(TensorRtError::InitializationFailed(
                "TensorRT C++ shim was not compiled; check build logs".to_owned(),
            ));
        }

        let cuda_driver = CudaDriver::load().map_err(TensorRtError::Cuda)?;
        let device = GpuDevice::query(&cuda_driver, device_ordinal).map_err(TensorRtError::Cuda)?;
        let precision = device.preferred_precision().unwrap_or(PrecisionTarget::Fp16);

        let trt_lib = TensorRtLibrary::load()?;

        let cuda_ctx = CudaContext::new(Arc::clone(&cuda_driver), device_ordinal)
            .map_err(TensorRtError::Cuda)?;
        cuda_ctx.make_current().map_err(TensorRtError::Cuda)?;

        let stream = CudaStream::new(Arc::clone(&cuda_driver)).map_err(TensorRtError::Cuda)?;

        // Create TensorRT runtime via dynamic entrypoint
        let create_fn = trt_lib.functions().create_infer_runtime_internal;
        let runtime = unsafe { shim::trt_runtime_create(create_fn, 0) };
        if runtime.is_null() {
            return Err(TensorRtError::InitializationFailed(
                "failed to create TensorRT runtime".to_owned(),
            ));
        }

        // Deserialize engines
        let enc_engine = unsafe {
            shim::trt_engine_deserialize(runtime, engines.enc.as_ptr().cast(), engines.enc.len())
        };
        if enc_engine.is_null() {
            unsafe { shim::trt_runtime_destroy(runtime) };
            return Err(TensorRtError::ModelLoadFailed(
                "failed to deserialize enc.engine".to_owned(),
            ));
        }

        let erb_engine = unsafe {
            shim::trt_engine_deserialize(
                runtime,
                engines.erb_dec.as_ptr().cast(),
                engines.erb_dec.len(),
            )
        };
        if erb_engine.is_null() {
            unsafe {
                shim::trt_engine_destroy(enc_engine);
                shim::trt_runtime_destroy(runtime);
            };
            return Err(TensorRtError::ModelLoadFailed(
                "failed to deserialize erb_dec.engine".to_owned(),
            ));
        }

        let df_engine = unsafe {
            shim::trt_engine_deserialize(
                runtime,
                engines.df_dec.as_ptr().cast(),
                engines.df_dec.len(),
            )
        };
        if df_engine.is_null() {
            unsafe {
                shim::trt_engine_destroy(erb_engine);
                shim::trt_engine_destroy(enc_engine);
                shim::trt_runtime_destroy(runtime);
            };
            return Err(TensorRtError::ModelLoadFailed(
                "failed to deserialize df_dec.engine".to_owned(),
            ));
        }

        // Create execution contexts
        let enc_ctx = unsafe { shim::trt_context_create(enc_engine) };
        let erb_ctx = unsafe { shim::trt_context_create(erb_engine) };
        let df_ctx = unsafe { shim::trt_context_create(df_engine) };

        if enc_ctx.is_null() || erb_ctx.is_null() || df_ctx.is_null() {
            unsafe {
                if !df_ctx.is_null() {
                    shim::trt_context_destroy(df_ctx);
                }
                if !erb_ctx.is_null() {
                    shim::trt_context_destroy(erb_ctx);
                }
                if !enc_ctx.is_null() {
                    shim::trt_context_destroy(enc_ctx);
                }
                shim::trt_engine_destroy(df_engine);
                shim::trt_engine_destroy(erb_engine);
                shim::trt_engine_destroy(enc_engine);
                shim::trt_runtime_destroy(runtime);
            };
            return Err(TensorRtError::InitializationFailed(
                "failed to create execution contexts for TensorRT engines".to_owned(),
            ));
        }

        // Allocate GPU device memory buffers
        let alloc = |len: usize| {
            GpuBuffer::<f32>::allocate(Arc::clone(&cuda_driver), len).map_err(TensorRtError::Cuda)
        };

        // Encoder buffers
        let feat_erb = alloc(NB_ERB)?;
        let feat_spec = alloc(2 * NB_DF)?;
        let mut enc_h = alloc(256)?;
        let mut enc_erb_buf = alloc(2 * NB_ERB)?;
        let mut enc_spec_buf = alloc(4 * NB_DF)?;

        let enc_e0 = alloc(64 * NB_ERB)?;
        let enc_e1 = alloc(64 * 16)?;
        let enc_e2 = alloc(64 * 8)?;
        let enc_e3 = alloc(64 * 8)?;
        let enc_emb = alloc(512)?;
        let enc_c0 = alloc(64 * NB_DF)?;
        let enc_lsnr = alloc(1)?;
        let enc_h_out = alloc(256)?;
        let enc_erb_buf_out = alloc(2 * NB_ERB)?;
        let enc_spec_buf_out = alloc(4 * NB_DF)?;

        // ERB decoder buffers
        let mut erb_h = alloc(512)?;
        let erb_m = alloc(NB_ERB)?;
        let erb_h_out = alloc(512)?;

        // DF decoder buffers
        let mut df_h = alloc(512)?;
        let mut df_c0_buf = alloc(64 * 4 * NB_DF)?;
        let df_coefs = alloc(DF_COEFS_LEN)?;
        let df_aux = alloc(1)?;
        let df_h_out = alloc(512)?;
        let df_c0_buf_out = alloc(64 * 4 * NB_DF)?;

        // Zero-fill all state buffers on stream
        enc_h
            .async_zero_fill(&stream)
            .map_err(TensorRtError::Cuda)?;
        enc_erb_buf
            .async_zero_fill(&stream)
            .map_err(TensorRtError::Cuda)?;
        enc_spec_buf
            .async_zero_fill(&stream)
            .map_err(TensorRtError::Cuda)?;
        erb_h
            .async_zero_fill(&stream)
            .map_err(TensorRtError::Cuda)?;
        df_h.async_zero_fill(&stream).map_err(TensorRtError::Cuda)?;
        df_c0_buf
            .async_zero_fill(&stream)
            .map_err(TensorRtError::Cuda)?;

        // Bind tensor addresses
        let bind = |ctx: *mut c_void, name: &str, buf: &GpuBuffer<f32>| -> bool {
            let c_name = match CString::new(name) {
                Ok(s) => s,
                Err(_) => return false,
            };
            unsafe {
                shim::trt_context_set_tensor_address(
                    ctx,
                    c_name.as_ptr(),
                    buf.device_ptr() as *mut c_void,
                )
            }
        };

        // Bind encoder
        bind(enc_ctx, "feat_erb", &feat_erb);
        bind(enc_ctx, "feat_spec", &feat_spec);
        bind(enc_ctx, "h_in", &enc_h);
        bind(enc_ctx, "feat_erb_buf", &enc_erb_buf);
        bind(enc_ctx, "feat_spec_buf", &enc_spec_buf);
        bind(enc_ctx, "e0", &enc_e0);
        bind(enc_ctx, "e1", &enc_e1);
        bind(enc_ctx, "e2", &enc_e2);
        bind(enc_ctx, "e3", &enc_e3);
        bind(enc_ctx, "emb", &enc_emb);
        bind(enc_ctx, "c0", &enc_c0);
        bind(enc_ctx, "lsnr", &enc_lsnr);
        bind(enc_ctx, "h_out", &enc_h_out);
        bind(enc_ctx, "feat_erb_buf_out", &enc_erb_buf_out);
        bind(enc_ctx, "feat_spec_buf_out", &enc_spec_buf_out);

        // Bind ERB decoder (emb, e0, e1, e2, e3 share encoder output buffers directly)
        bind(erb_ctx, "emb", &enc_emb);
        bind(erb_ctx, "e0", &enc_e0);
        bind(erb_ctx, "e1", &enc_e1);
        bind(erb_ctx, "e2", &enc_e2);
        bind(erb_ctx, "e3", &enc_e3);
        bind(erb_ctx, "h_in", &erb_h);
        bind(erb_ctx, "m", &erb_m);
        bind(erb_ctx, "h_out", &erb_h_out);

        // Bind DF decoder (emb and c0 share encoder output buffers directly)
        bind(df_ctx, "emb", &enc_emb);
        bind(df_ctx, "c0", &enc_c0);
        bind(df_ctx, "h_in", &df_h);
        bind(df_ctx, "c0_buf", &df_c0_buf);
        bind(df_ctx, "coefs", &df_coefs);
        bind(df_ctx, "235", &df_aux);
        bind(df_ctx, "h_out", &df_h_out);
        bind(df_ctx, "c0_buf_out", &df_c0_buf_out);

        stream.synchronize().map_err(TensorRtError::Cuda)?;

        Ok(Self {
            device_ordinal,
            device,
            precision,
            _trt_lib: trt_lib,
            _cuda_driver: cuda_driver,
            cuda_ctx,
            stream,
            runtime,
            enc_engine,
            enc_ctx,
            erb_engine,
            erb_ctx,
            df_engine,
            df_ctx,
            feat_erb,
            feat_spec,
            enc_h,
            enc_erb_buf,
            enc_spec_buf,
            enc_e0,
            enc_e1,
            enc_e2,
            enc_e3,
            enc_emb,
            enc_c0,
            enc_lsnr,
            enc_h_out,
            enc_erb_buf_out,
            enc_spec_buf_out,
            erb_h,
            erb_m,
            erb_h_out,
            df_h,
            df_c0_buf,
            df_coefs,
            df_aux,
            df_h_out,
            df_c0_buf_out,
            frames_processed: 0,
        })
    }

    #[must_use]
    pub const fn device_ordinal(&self) -> u32 {
        self.device_ordinal
    }

    #[must_use]
    pub fn device(&self) -> &GpuDevice {
        &self.device
    }

    #[must_use]
    pub const fn precision(&self) -> PrecisionTarget {
        self.precision
    }

    #[must_use]
    pub const fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    /// Resets all recurrent hidden states and causal delay buffers on the GPU.
    pub fn reset(&mut self) -> Result<(), TensorRtError> {
        self.cuda_ctx.make_current().map_err(TensorRtError::Cuda)?;
        self.enc_h
            .async_zero_fill(&self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.enc_erb_buf
            .async_zero_fill(&self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.enc_spec_buf
            .async_zero_fill(&self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.erb_h
            .async_zero_fill(&self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.df_h
            .async_zero_fill(&self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.df_c0_buf
            .async_zero_fill(&self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.stream.synchronize().map_err(TensorRtError::Cuda)?;
        self.frames_processed = 0;
        Ok(())
    }

    /// Runs one streaming 10 ms hop through the encoder and both decoders on the GPU.
    pub fn run_frame(
        &mut self,
        feat_erb: &[f32; NB_ERB],
        feat_spec: &[[f32; NB_DF]; 2],
        out: &mut Dfn3Output,
    ) -> Result<(), TensorRtError> {
        if feat_erb
            .iter()
            .chain(feat_spec.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err(TensorRtError::ExecutionFailed(
                "feature frame contains non-finite values".to_owned(),
            ));
        }

        self.cuda_ctx.make_current().map_err(TensorRtError::Cuda)?;

        // 1. Copy input features to GPU
        self.feat_erb
            .async_copy_from_host(feat_erb, &self.stream)
            .map_err(TensorRtError::Cuda)?;

        let mut spec = [0.0f32; 2 * NB_DF];
        spec[..NB_DF].copy_from_slice(&feat_spec[0]);
        spec[NB_DF..].copy_from_slice(&feat_spec[1]);
        self.feat_spec
            .async_copy_from_host(&spec, &self.stream)
            .map_err(TensorRtError::Cuda)?;

        // 2. Enqueue Encoder
        let raw_stream = self.stream.raw_stream();
        let ok_enc = unsafe { shim::trt_context_enqueue_v3(self.enc_ctx, raw_stream) };
        if !ok_enc {
            return Err(TensorRtError::ExecutionFailed(
                "TensorRT encoder enqueue failed".to_owned(),
            ));
        }

        // 3. Enqueue ERB Decoder
        let ok_erb = unsafe { shim::trt_context_enqueue_v3(self.erb_ctx, raw_stream) };
        if !ok_erb {
            return Err(TensorRtError::ExecutionFailed(
                "TensorRT erb_dec enqueue failed".to_owned(),
            ));
        }

        // 4. Enqueue DF Decoder
        let ok_df = unsafe { shim::trt_context_enqueue_v3(self.df_ctx, raw_stream) };
        if !ok_df {
            return Err(TensorRtError::ExecutionFailed(
                "TensorRT df_dec enqueue failed".to_owned(),
            ));
        }

        // 5. Asynchronously read back outputs to host
        let mut lsnr_val = [0.0f32; 1];
        // SAFETY: lsnr_val is valid host memory for 1 float and stays alive until stream sync.
        unsafe { self.enc_lsnr.async_copy_to_host(&mut lsnr_val, &self.stream) }
            .map_err(TensorRtError::Cuda)?;
        // SAFETY: out.erb_mask stays alive until stream sync.
        unsafe { self.erb_m.async_copy_to_host(&mut out.erb_mask, &self.stream) }
            .map_err(TensorRtError::Cuda)?;
        // SAFETY: out.df_coefs stays alive until stream sync.
        unsafe { self.df_coefs.async_copy_to_host(&mut out.df_coefs, &self.stream) }
            .map_err(TensorRtError::Cuda)?;

        // 6. Commit recurrent states on GPU asynchronously (Device to Device)
        self.enc_h
            .async_copy_from_device(&self.enc_h_out, &self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.enc_erb_buf
            .async_copy_from_device(&self.enc_erb_buf_out, &self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.enc_spec_buf
            .async_copy_from_device(&self.enc_spec_buf_out, &self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.erb_h
            .async_copy_from_device(&self.erb_h_out, &self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.df_h
            .async_copy_from_device(&self.df_h_out, &self.stream)
            .map_err(TensorRtError::Cuda)?;
        self.df_c0_buf
            .async_copy_from_device(&self.df_c0_buf_out, &self.stream)
            .map_err(TensorRtError::Cuda)?;

        // 7. Synchronize stream to finalize reads and verify finiteness
        self.stream.synchronize().map_err(TensorRtError::Cuda)?;

        let lsnr = lsnr_val[0];
        if !lsnr.is_finite()
            || out.erb_mask.iter().any(|v| !v.is_finite())
            || out.df_coefs.iter().any(|v| !v.is_finite())
        {
            out.erb_mask.fill(0.0);
            out.df_coefs.fill(0.0);
            return Err(TensorRtError::NonFiniteOutputDetected);
        }

        out.lsnr = lsnr;
        self.frames_processed = self.frames_processed.saturating_add(1);
        Ok(())
    }
}

impl Drop for TensorRtDfn3Session {
    fn drop(&mut self) {
        unsafe {
            if !self.enc_ctx.is_null() {
                shim::trt_context_destroy(self.enc_ctx);
            }
            if !self.erb_ctx.is_null() {
                shim::trt_context_destroy(self.erb_ctx);
            }
            if !self.df_ctx.is_null() {
                shim::trt_context_destroy(self.df_ctx);
            }
            if !self.enc_engine.is_null() {
                shim::trt_engine_destroy(self.enc_engine);
            }
            if !self.erb_engine.is_null() {
                shim::trt_engine_destroy(self.erb_engine);
            }
            if !self.df_engine.is_null() {
                shim::trt_engine_destroy(self.df_engine);
            }
            if !self.runtime.is_null() {
                shim::trt_runtime_destroy(self.runtime);
            }
        }
    }
}

impl std::fmt::Debug for TensorRtDfn3Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TensorRtDfn3Session")
            .field("device_ordinal", &self.device_ordinal)
            .field("device", &self.device.name())
            .field("precision", &self.precision)
            .field("frames_processed", &self.frames_processed)
            .finish_non_exhaustive()
    }
}
