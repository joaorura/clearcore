//! Streaming DeepFilterNet3 inference on OpenVINO using the explicit-state ONNX graphs.
//!
//! The three graphs produced by `tools/accelerators/make_stateful_onnx.py`
//! (`models/stateful/{enc,erb_dec,df_dec}.onnx`) expose every recurrent state as a graph port:
//!
//! | graph     | extra inputs                                  | extra outputs                                  |
//! |-----------|-----------------------------------------------|------------------------------------------------|
//! | `enc`     | `h_in` `[1,1,256]`, `feat_erb_buf` `[1,1,2,32]`, `feat_spec_buf` `[1,2,2,96]` | `h_out`, `feat_erb_buf_out`, `feat_spec_buf_out` |
//! | `erb_dec` | `h_in` `[2,1,256]`                            | `h_out`                                        |
//! | `df_dec`  | `h_in` `[2,1,256]`, `c0_buf` `[1,64,4,96]`    | `h_out`, `c0_buf_out`                          |
//!
//! [`Dfn3Session`] pins the time axis to `S = 1` (one 10 ms hop), compiles the graphs for one
//! OpenVINO device with the Clearcore precision policy ([`crate::precision`]), and feeds every
//! `*_out` state back into the matching `*_in` input after each hop.
//!
//! This type is the neural half only. STFT, ERB feature extraction, mask/deep-filter
//! application and iSTFT stay in the DSP pipeline of `realtime-noise-model`.

use std::path::Path;

use crate::core::OpenVinoCore;
use crate::error::OpenVinoError;
use crate::infer::OpenVinoInferRequest;
use crate::model::{OpenVinoCompiledModel, OpenVinoModel};
use crate::precision::{InferencePrecision, precision_for_device};
use crate::tensor::OpenVinoTensor;

/// ERB bands of the encoder input and of the ERB mask.
pub const NB_ERB: usize = 32;
/// Complex bins processed by deep filtering.
pub const NB_DF: usize = 96;
/// Deep filtering FIR order.
pub const DF_ORDER: usize = 5;
/// Values per hop in [`Dfn3Output::df_coefs`]: `NB_DF` bins x `DF_ORDER` taps x (re, im).
pub const DF_COEFS_LEN: usize = NB_DF * DF_ORDER * 2;

const EMB_DIM: i64 = 512;
const GRU_HIDDEN: i64 = 256;

/// Result of one streaming hop.
#[derive(Debug, Clone, PartialEq)]
pub struct Dfn3Output {
    /// Local SNR estimate in dB.
    pub lsnr: f32,
    /// ERB mask gains, one per band.
    pub erb_mask: [f32; NB_ERB],
    /// Deep filter coefficients, `[bin][tap][re, im]` flattened (interleaved real/imaginary).
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

struct Stage {
    // The request must drop before the compiled model that created it.
    request: OpenVinoInferRequest,
    _compiled: OpenVinoCompiledModel,
}

/// Static input shapes of the three graphs for `S = 1`.
const ENC_PINS: [(&str, &[i64]); 2] = [
    ("feat_erb", &[1, 1, 1, NB_ERB as i64]),
    ("feat_spec", &[1, 2, 1, NB_DF as i64]),
];
const ERB_PINS: [(&str, &[i64]); 5] = [
    ("emb", &[1, 1, EMB_DIM]),
    ("e3", &[1, 64, 1, 8]),
    ("e2", &[1, 64, 1, 8]),
    ("e1", &[1, 64, 1, 16]),
    ("e0", &[1, 64, 1, 32]),
];
const DF_PINS: [(&str, &[i64]); 2] = [("emb", &[1, 1, EMB_DIM]), ("c0", &[1, 64, 1, NB_DF as i64])];

/// A loaded, device-bound streaming DeepFilterNet3 network.
pub struct Dfn3Session {
    device: String,
    precision: InferencePrecision,
    enc: Stage,
    erb: Stage,
    df: Stage,
    // encoder inputs
    feat_erb: OpenVinoTensor,
    feat_spec: OpenVinoTensor,
    enc_h: OpenVinoTensor,
    enc_erb_buf: OpenVinoTensor,
    enc_spec_buf: OpenVinoTensor,
    // erb decoder inputs
    erb_emb: OpenVinoTensor,
    erb_e3: OpenVinoTensor,
    erb_e2: OpenVinoTensor,
    erb_e1: OpenVinoTensor,
    erb_e0: OpenVinoTensor,
    erb_h: OpenVinoTensor,
    // df decoder inputs
    df_emb: OpenVinoTensor,
    df_c0: OpenVinoTensor,
    df_h: OpenVinoTensor,
    df_c0_buf: OpenVinoTensor,
    frames: u64,
}

/// File names of the three stateful graphs inside the model directory.
pub const ENC_FILE: &str = "enc.onnx";
pub const ERB_DEC_FILE: &str = "erb_dec.onnx";
pub const DF_DEC_FILE: &str = "df_dec.onnx";

/// Upper bound accepted for one graph file. The real graphs are 2-4 MB; anything far beyond that
/// is not one of ours and is refused before it is read into memory.
pub const MAX_GRAPH_BYTES: u64 = 64 * 1024 * 1024;

/// Serialized ONNX bytes of the three stateful graphs.
///
/// Compiling from these bytes (instead of from a path) lets a caller verify exactly the bytes
/// that will be compiled, with no window for the file to change between check and use.
#[derive(Clone)]
pub struct Dfn3Graphs {
    pub enc: Vec<u8>,
    pub erb_dec: Vec<u8>,
    pub df_dec: Vec<u8>,
}

impl Dfn3Graphs {
    /// Reads one graph file, refusing anything larger than [`MAX_GRAPH_BYTES`].
    pub fn read_file(dir: &Path, file: &str) -> Result<Vec<u8>, OpenVinoError> {
        use std::io::Read;
        let path = dir.join(file);
        let read_error = |error: std::io::Error| {
            OpenVinoError::ModelReadFailed(format!("{}: {error}", path.display()))
        };
        let mut bytes = Vec::new();
        std::fs::File::open(&path)
            .map_err(read_error)?
            .take(MAX_GRAPH_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(read_error)?;
        if bytes.len() as u64 > MAX_GRAPH_BYTES {
            return Err(OpenVinoError::ModelReadFailed(format!(
                "{} is larger than the {MAX_GRAPH_BYTES}-byte limit",
                path.display()
            )));
        }
        Ok(bytes)
    }

    /// Reads `enc.onnx`, `erb_dec.onnx` and `df_dec.onnx` from `dir` without verifying them.
    pub fn read_dir(dir: &Path) -> Result<Self, OpenVinoError> {
        Ok(Self {
            enc: Self::read_file(dir, ENC_FILE)?,
            erb_dec: Self::read_file(dir, ERB_DEC_FILE)?,
            df_dec: Self::read_file(dir, DF_DEC_FILE)?,
        })
    }
}

fn compile_stage(
    core: &OpenVinoCore,
    graph: &[u8],
    pins: &[(&str, &[i64])],
    device: &str,
    precision: InferencePrecision,
) -> Result<(OpenVinoCompiledModel, OpenVinoInferRequest), OpenVinoError> {
    let mut model: OpenVinoModel = core.read_model_from_memory(graph, None)?;
    for (name, shape) in pins {
        model.reshape_input(name, shape)?;
    }
    let compiled = core.compile_model_with_properties(
        &model,
        device,
        &[
            ("PERFORMANCE_HINT", "LATENCY"),
            ("INFERENCE_PRECISION_HINT", precision.hint()),
        ],
    )?;
    let request = compiled.create_infer_request()?;
    Ok((compiled, request))
}

fn zeros(core: &OpenVinoCore, shape: &[i64]) -> Result<OpenVinoTensor, OpenVinoError> {
    let len: i64 = shape.iter().product();
    let len = usize::try_from(len)
        .map_err(|_| OpenVinoError::ShapeError(format!("invalid tensor shape {shape:?}")))?;
    OpenVinoTensor::from_slice_f32(shape, &vec![0.0; len], core.library().clone())
}

fn bind(
    request: &mut OpenVinoInferRequest,
    name: &str,
    tensor: &OpenVinoTensor,
) -> Result<(), OpenVinoError> {
    request.set_tensor(name, tensor)
}

fn copy_slice(dst: &mut OpenVinoTensor, src: &[f32]) -> Result<(), OpenVinoError> {
    let dst = dst.as_mut_slice_f32()?;
    if dst.len() != src.len() {
        return Err(OpenVinoError::ShapeError(format!(
            "tensor holds {} values, expected {}",
            dst.len(),
            src.len()
        )));
    }
    dst.copy_from_slice(src);
    Ok(())
}

fn copy_tensor(dst: &mut OpenVinoTensor, src: &OpenVinoTensor) -> Result<(), OpenVinoError> {
    copy_slice(dst, src.as_slice_f32()?)
}

fn zero_fill(tensor: &mut OpenVinoTensor) -> Result<(), OpenVinoError> {
    tensor.as_mut_slice_f32()?.fill(0.0);
    Ok(())
}

impl Dfn3Session {
    /// Loads the three stateful graphs from `model_dir` and compiles them for `device`
    /// (`"NPU"`, `"GPU"`, `"GPU.1"`, `"CPU"`, ...), using [`precision_for_device`].
    ///
    /// The files are not verified here; callers that need provenance should read them with
    /// [`Dfn3Graphs::read_file`], check the bytes and call [`Self::load_graphs`].
    pub fn load(model_dir: &Path, device: &str) -> Result<Self, OpenVinoError> {
        Self::load_graphs(&Dfn3Graphs::read_dir(model_dir)?, device)
    }

    /// Compiles already-read graphs for `device` (see [`Self::load`]).
    pub fn load_graphs(graphs: &Dfn3Graphs, device: &str) -> Result<Self, OpenVinoError> {
        let core = OpenVinoCore::new()?;
        if !core.is_device_available(device) {
            return Err(OpenVinoError::DeviceNotFound(device.to_owned()));
        }
        let precision = precision_for_device(device);

        let (enc_c, mut enc_r) = compile_stage(&core, &graphs.enc, &ENC_PINS, device, precision)?;
        let (erb_c, mut erb_r) =
            compile_stage(&core, &graphs.erb_dec, &ERB_PINS, device, precision)?;
        let (df_c, mut df_r) = compile_stage(&core, &graphs.df_dec, &DF_PINS, device, precision)?;

        let feat_erb = zeros(&core, &[1, 1, 1, NB_ERB as i64])?;
        let feat_spec = zeros(&core, &[1, 2, 1, NB_DF as i64])?;
        let enc_h = zeros(&core, &[1, 1, GRU_HIDDEN])?;
        let enc_erb_buf = zeros(&core, &[1, 1, 2, NB_ERB as i64])?;
        let enc_spec_buf = zeros(&core, &[1, 2, 2, NB_DF as i64])?;
        bind(&mut enc_r, "feat_erb", &feat_erb)?;
        bind(&mut enc_r, "feat_spec", &feat_spec)?;
        bind(&mut enc_r, "h_in", &enc_h)?;
        bind(&mut enc_r, "feat_erb_buf", &enc_erb_buf)?;
        bind(&mut enc_r, "feat_spec_buf", &enc_spec_buf)?;

        let erb_emb = zeros(&core, &[1, 1, EMB_DIM])?;
        let erb_e3 = zeros(&core, &[1, 64, 1, 8])?;
        let erb_e2 = zeros(&core, &[1, 64, 1, 8])?;
        let erb_e1 = zeros(&core, &[1, 64, 1, 16])?;
        let erb_e0 = zeros(&core, &[1, 64, 1, 32])?;
        let erb_h = zeros(&core, &[2, 1, GRU_HIDDEN])?;
        bind(&mut erb_r, "emb", &erb_emb)?;
        bind(&mut erb_r, "e3", &erb_e3)?;
        bind(&mut erb_r, "e2", &erb_e2)?;
        bind(&mut erb_r, "e1", &erb_e1)?;
        bind(&mut erb_r, "e0", &erb_e0)?;
        bind(&mut erb_r, "h_in", &erb_h)?;

        let df_emb = zeros(&core, &[1, 1, EMB_DIM])?;
        let df_c0 = zeros(&core, &[1, 64, 1, NB_DF as i64])?;
        let df_h = zeros(&core, &[2, 1, GRU_HIDDEN])?;
        let df_c0_buf = zeros(&core, &[1, 64, 4, NB_DF as i64])?;
        bind(&mut df_r, "emb", &df_emb)?;
        bind(&mut df_r, "c0", &df_c0)?;
        bind(&mut df_r, "h_in", &df_h)?;
        bind(&mut df_r, "c0_buf", &df_c0_buf)?;

        Ok(Self {
            device: device.to_owned(),
            precision,
            enc: Stage {
                request: enc_r,
                _compiled: enc_c,
            },
            erb: Stage {
                request: erb_r,
                _compiled: erb_c,
            },
            df: Stage {
                request: df_r,
                _compiled: df_c,
            },
            feat_erb,
            feat_spec,
            enc_h,
            enc_erb_buf,
            enc_spec_buf,
            erb_emb,
            erb_e3,
            erb_e2,
            erb_e1,
            erb_e0,
            erb_h,
            df_emb,
            df_c0,
            df_h,
            df_c0_buf,
            frames: 0,
        })
    }

    /// Tries each device in order and returns the first session that loads; the error lists
    /// every device that failed and why.
    pub fn load_with_fallback(model_dir: &Path, devices: &[String]) -> Result<Self, OpenVinoError> {
        Self::load_graphs_with_fallback(&Dfn3Graphs::read_dir(model_dir)?, devices)
    }

    /// [`Self::load_with_fallback`] over graphs that were already read (and verified).
    pub fn load_graphs_with_fallback(
        graphs: &Dfn3Graphs,
        devices: &[String],
    ) -> Result<Self, OpenVinoError> {
        let mut failures = Vec::new();
        for device in devices {
            match Self::load_graphs(graphs, device) {
                Ok(session) => return Ok(session),
                Err(err) => failures.push(format!("{device}: {err}")),
            }
        }
        Err(OpenVinoError::DeviceNotFound(if failures.is_empty() {
            "no OpenVINO device to try".to_owned()
        } else {
            failures.join("; ")
        }))
    }

    /// OpenVINO device the graphs are compiled for.
    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Precision requested from the plugin (see [`crate::precision`]).
    #[must_use]
    pub const fn precision(&self) -> InferencePrecision {
        self.precision
    }

    /// Number of hops processed since creation or the last [`Self::reset`].
    #[must_use]
    pub const fn frames_processed(&self) -> u64 {
        self.frames
    }

    /// Zeroes every recurrent state (GRU hidden states and causal delay rings).
    pub fn reset(&mut self) -> Result<(), OpenVinoError> {
        for tensor in [
            &mut self.enc_h,
            &mut self.enc_erb_buf,
            &mut self.enc_spec_buf,
            &mut self.erb_h,
            &mut self.df_h,
            &mut self.df_c0_buf,
        ] {
            zero_fill(tensor)?;
        }
        self.frames = 0;
        Ok(())
    }

    /// Runs one 10 ms hop through the encoder and both decoders, then advances every state.
    ///
    /// `feat_spec` is `[real; 96]` followed by `[imag; 96]`. On error no state is advanced
    /// past the point of failure, but the caller should [`Self::reset`] before reusing the
    /// session, because the decoders may have run with a half-updated hop.
    pub fn run_frame(
        &mut self,
        feat_erb: &[f32; NB_ERB],
        feat_spec: &[[f32; NB_DF]; 2],
        out: &mut Dfn3Output,
    ) -> Result<(), OpenVinoError> {
        if feat_erb
            .iter()
            .chain(feat_spec.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err(OpenVinoError::InvalidInput(
                "feature frame contains a non-finite value".to_owned(),
            ));
        }
        copy_slice(&mut self.feat_erb, feat_erb)?;
        let mut spec = [0.0f32; 2 * NB_DF];
        spec[..NB_DF].copy_from_slice(&feat_spec[0]);
        spec[NB_DF..].copy_from_slice(&feat_spec[1]);
        copy_slice(&mut self.feat_spec, &spec)?;

        // Encoder.
        self.enc.request.infer()?;
        let req = &self.enc.request;
        copy_tensor(&mut self.erb_e0, &req.get_tensor("e0")?)?;
        copy_tensor(&mut self.erb_e1, &req.get_tensor("e1")?)?;
        copy_tensor(&mut self.erb_e2, &req.get_tensor("e2")?)?;
        copy_tensor(&mut self.erb_e3, &req.get_tensor("e3")?)?;
        let emb = req.get_tensor("emb")?;
        copy_tensor(&mut self.erb_emb, &emb)?;
        copy_tensor(&mut self.df_emb, &emb)?;
        copy_tensor(&mut self.df_c0, &req.get_tensor("c0")?)?;
        let lsnr = *req
            .get_tensor("lsnr")?
            .as_slice_f32()?
            .first()
            .ok_or_else(|| OpenVinoError::TensorAccessFailed("empty lsnr tensor".to_owned()))?;
        let enc_h_out = req.get_tensor("h_out")?;
        let enc_erb_buf_out = req.get_tensor("feat_erb_buf_out")?;
        let enc_spec_buf_out = req.get_tensor("feat_spec_buf_out")?;

        // ERB decoder.
        self.erb.request.infer()?;
        let erb_mask = self.erb.request.get_tensor("m")?;
        let erb_h_out = self.erb.request.get_tensor("h_out")?;

        // Deep filtering decoder.
        self.df.request.infer()?;
        let coefs = self.df.request.get_tensor("coefs")?;
        let df_h_out = self.df.request.get_tensor("h_out")?;
        let df_c0_buf_out = self.df.request.get_tensor("c0_buf_out")?;

        let mask = erb_mask.as_slice_f32()?;
        let coefs = coefs.as_slice_f32()?;
        if mask.len() != NB_ERB || coefs.len() != DF_COEFS_LEN {
            return Err(OpenVinoError::ShapeError(format!(
                "unexpected decoder output sizes: mask {} (want {NB_ERB}), coefs {} (want {DF_COEFS_LEN})",
                mask.len(),
                coefs.len()
            )));
        }
        if !lsnr.is_finite() || mask.iter().chain(coefs).any(|v| !v.is_finite()) {
            return Err(OpenVinoError::InferenceExecutionFailed(
                "network produced a non-finite value".to_owned(),
            ));
        }
        out.lsnr = lsnr;
        out.erb_mask.copy_from_slice(mask);
        out.df_coefs.copy_from_slice(coefs);

        // Commit the recurrent state only after all three stages succeeded.
        copy_tensor(&mut self.enc_h, &enc_h_out)?;
        copy_tensor(&mut self.enc_erb_buf, &enc_erb_buf_out)?;
        copy_tensor(&mut self.enc_spec_buf, &enc_spec_buf_out)?;
        copy_tensor(&mut self.erb_h, &erb_h_out)?;
        copy_tensor(&mut self.df_h, &df_h_out)?;
        copy_tensor(&mut self.df_c0_buf, &df_c0_buf_out)?;
        self.frames = self.frames.saturating_add(1);
        Ok(())
    }
}

impl std::fmt::Debug for Dfn3Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Dfn3Session")
            .field("device", &self.device)
            .field("precision", &self.precision)
            .field("frames", &self.frames)
            .finish_non_exhaustive()
    }
}
