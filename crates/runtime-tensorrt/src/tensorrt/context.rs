use std::sync::Arc;

use realtime_noise_contracts::HOP_SAMPLES;

use crate::cuda::bindings::CudaDriver;
use crate::cuda::buffer::GpuBuffer;
use crate::cuda::context::CudaContext;
use crate::cuda::device::GpuDevice;
use crate::cuda::stream::CudaStream;
use crate::error::TensorRtError;
use crate::precision::PrecisionTarget;
use crate::tensorrt::bindings::TensorRtLibrary;

/// Recurrent state dimension for DeepFilterNet3 latent embeddings (GRU hidden size).
pub const DFN3_GRU_STATE_DIM: usize = 512;

/// Safe TensorRT inference context targeting NVIDIA GPU execution.
///
/// Manages device VRAM allocations (input, output, and recurrent state buffers),
/// an isolated CUDA stream, and execution with targeted precision (FP16 or FP8 for Blackwell sm_120).
/// Guarantees zero raw audio bypass by enforcing fail-closed digital silence on error.
#[derive(Debug)]
pub struct TensorRtInferenceContext {
    device: GpuDevice,
    precision: PrecisionTarget,
    // Fields drop in declaration order: VRAM buffers and the stream must go before the CUDA
    // context that owns them.
    input_gpu: Option<GpuBuffer<f32>>,
    output_gpu: Option<GpuBuffer<f32>>,
    state_gpu: Option<GpuBuffer<f32>>,
    stream: Option<CudaStream>,
    cuda_ctx: Option<CudaContext>,
    is_mock: bool,
    simulated_failure: bool,
    runtime_version: String,
    frames_processed: u64,
}

impl TensorRtInferenceContext {
    /// Initializes a hardware-backed TensorRT inference context on the target GPU device.
    ///
    /// # Errors
    /// Returns `TensorRtError` if CUDA driver, GPU device, or TensorRT library cannot be initialized.
    pub fn new(device_ordinal: u32, precision: PrecisionTarget) -> Result<Self, TensorRtError> {
        // 1. Load CUDA Driver
        let driver = CudaDriver::load().map_err(TensorRtError::Cuda)?;

        // 2. Query target GPU device
        let device = GpuDevice::query(&driver, device_ordinal).map_err(TensorRtError::Cuda)?;

        // 3. Verify precision target compatibility
        if !device.supports_precision(precision) {
            return Err(TensorRtError::UnsupportedPrecision {
                requested: precision,
                device_name: device.name().to_owned(),
                sm_version: device.sm_string(),
            });
        }

        // 4. Load TensorRT runtime library
        let trt_lib = TensorRtLibrary::load()?;
        let runtime_version = trt_lib.version_string().to_owned();

        // 5. Initialize CUDA Context on target device
        let cuda_ctx =
            CudaContext::new(Arc::clone(&driver), device_ordinal).map_err(TensorRtError::Cuda)?;
        cuda_ctx.make_current().map_err(TensorRtError::Cuda)?;

        // 6. Create dedicated CUDA stream
        let stream = CudaStream::new(Arc::clone(&driver)).map_err(TensorRtError::Cuda)?;

        // 7. Allocate GPU device memory buffers
        let input_gpu = GpuBuffer::<f32>::allocate(Arc::clone(&driver), HOP_SAMPLES)
            .map_err(TensorRtError::Cuda)?;
        let output_gpu = GpuBuffer::<f32>::allocate(Arc::clone(&driver), HOP_SAMPLES)
            .map_err(TensorRtError::Cuda)?;
        let state_gpu = GpuBuffer::<f32>::allocate(Arc::clone(&driver), DFN3_GRU_STATE_DIM)
            .map_err(TensorRtError::Cuda)?;

        Ok(Self {
            device,
            precision,
            cuda_ctx: Some(cuda_ctx),
            stream: Some(stream),
            input_gpu: Some(input_gpu),
            output_gpu: Some(output_gpu),
            state_gpu: Some(state_gpu),
            is_mock: false,
            simulated_failure: false,
            runtime_version,
            frames_processed: 0,
        })
    }

    /// Initializes a hardware-backed context choosing the precision with the Clearcore policy
    /// (FP8 where supported, otherwise FP16, otherwise INT8; never FP32).
    ///
    /// # Errors
    /// Returns `TensorRtError::NoSupportedPrecision` when the GPU supports none of those, or
    /// any error of [`Self::new`].
    pub fn new_auto(device_ordinal: u32) -> Result<Self, TensorRtError> {
        let driver = CudaDriver::load().map_err(TensorRtError::Cuda)?;
        let device = GpuDevice::query(&driver, device_ordinal).map_err(TensorRtError::Cuda)?;
        let precision =
            device
                .preferred_precision()
                .ok_or_else(|| TensorRtError::NoSupportedPrecision {
                    device_name: device.name().to_owned(),
                    sm_version: device.sm_string(),
                })?;
        Self::new(device_ordinal, precision)
    }

    /// Creates a mock TensorRT context for testing and offline qualification.
    #[must_use]
    pub fn new_mock(device: GpuDevice, precision: PrecisionTarget) -> Self {
        Self {
            device,
            precision,
            cuda_ctx: None,
            stream: None,
            input_gpu: None,
            output_gpu: None,
            state_gpu: None,
            is_mock: true,
            simulated_failure: false,
            runtime_version: "11.3.0.99".to_owned(),
            frames_processed: 0,
        }
    }

    /// Creates a mock context representing Blackwell sm_120 in FP16 mode.
    #[must_use]
    pub fn new_mock_sm120_fp16() -> Self {
        Self::new_mock(GpuDevice::mock_blackwell_sm120(), PrecisionTarget::Fp16)
    }

    /// Creates a mock context for a device, using its preferred precision per the Clearcore policy.
    #[must_use]
    pub fn new_mock_preferred(device: GpuDevice) -> Option<Self> {
        let precision = device.preferred_precision()?;
        Some(Self::new_mock(device, precision))
    }

    /// Creates a mock context representing Blackwell sm_120 in FP8 mode.
    #[must_use]
    pub fn new_mock_sm120_fp8() -> Self {
        Self::new_mock(GpuDevice::mock_blackwell_sm120(), PrecisionTarget::Fp8)
    }

    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.is_mock
    }

    /// True when the context owns a real CUDA context, stream and VRAM buffers.
    #[must_use]
    pub const fn is_hardware_backed(&self) -> bool {
        !self.is_mock
    }

    /// Whether `process_frame` runs a deserialized TensorRT engine.
    ///
    /// Always `false` today: `libnvinfer` exposes only a C++ vtable API (no C entry point), and
    /// this crate binds just the version queries and `createInferRuntime_INTERNAL`. The hardware
    /// path therefore performs a real CUDA host-to-device-to-host round trip on the chosen GPU
    /// but not neural inference.
    #[must_use]
    pub const fn executes_engine(&self) -> bool {
        false
    }

    #[must_use]
    pub fn state_gpu(&self) -> Option<&GpuBuffer<f32>> {
        self.state_gpu.as_ref()
    }

    #[must_use]
    pub fn device(&self) -> &GpuDevice {
        &self.device
    }

    #[must_use]
    pub const fn precision(&self) -> PrecisionTarget {
        self.precision
    }

    /// Updates the active precision target, verifying device compatibility.
    pub fn set_precision(&mut self, precision: PrecisionTarget) -> Result<(), TensorRtError> {
        if !self.device.supports_precision(precision) {
            return Err(TensorRtError::UnsupportedPrecision {
                requested: precision,
                device_name: self.device.name().to_owned(),
                sm_version: self.device.sm_string(),
            });
        }
        self.precision = precision;
        Ok(())
    }

    #[must_use]
    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }

    #[must_use]
    pub const fn frames_processed(&self) -> u64 {
        self.frames_processed
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }

    /// Executes inference for a single 10 ms audio frame (`HOP_SAMPLES` = 480).
    ///
    /// # Safety and Invariants
    /// 1. Verifies that all input samples are finite.
    /// 2. Executes asynchronous Host-to-Device transfer, inference kernel, and Device-to-Host transfer.
    /// 3. Synchronizes the dedicated CUDA stream.
    /// 4. Checks that all output samples are finite. If any non-finite sample (NaN or Inf) is detected,
    ///    the output frame is zero-filled (digital silence) and `NonFiniteOutputDetected` is returned.
    ///
    /// # Errors
    /// Returns `TensorRtError` if execution fails, simulated failure is active, or non-finite output occurs.
    pub fn process_frame(
        &mut self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(), TensorRtError> {
        let result = self.execute_frame(input, output);
        if result.is_err() {
            // Zero raw bypass invariant: every error path ends in digital silence,
            // including CUDA failures that return early with `?`.
            output.fill(0.0);
        }
        result
    }

    fn execute_frame(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), TensorRtError> {
        if self.simulated_failure {
            output.fill(0.0);
            return Err(TensorRtError::ExecutionFailed(
                "Simulated TensorRT execution failure".to_owned(),
            ));
        }

        if input.len() != HOP_SAMPLES || output.len() != HOP_SAMPLES {
            output.fill(0.0);
            return Err(TensorRtError::ExecutionFailed(format!(
                "Audio frame length mismatch: expected {HOP_SAMPLES}, got input {}, output {}",
                input.len(),
                output.len()
            )));
        }

        // Validate finite inputs
        if input.iter().any(|&s| !s.is_finite()) {
            output.fill(0.0);
            return Err(TensorRtError::ExecutionFailed(
                "Input frame contains non-finite samples".to_owned(),
            ));
        }

        if self.is_mock {
            // Emulate execution with precision targeting:
            // FP16 / FP8 quantization effects + pass-through filtering
            for (dst, &src) in output.iter_mut().zip(input.iter()) {
                *dst = match self.precision {
                    PrecisionTarget::Fp8 => {
                        // Simulate FP8 E4M3 range and quantization
                        let quantized = (src * 128.0).round() / 128.0;
                        quantized.clamp(-448.0, 448.0)
                    }
                    PrecisionTarget::Fp16 => {
                        // Simulate FP16 half precision quantization
                        let quantized = (src * 2048.0).round() / 2048.0;
                        quantized.clamp(-65504.0, 65504.0)
                    }
                    PrecisionTarget::Int8 => {
                        // Simulate symmetric INT8 with a fixed 1/127 scale and saturation
                        (src.clamp(-1.0, 1.0) * 127.0).round() / 127.0
                    }
                    PrecisionTarget::Fp32 => src,
                };
            }
        } else {
            // Hardware execution path
            let (cuda_ctx, stream, input_gpu, output_gpu) = match (
                &self.cuda_ctx,
                &self.stream,
                &mut self.input_gpu,
                &mut self.output_gpu,
            ) {
                (Some(c), Some(s), Some(i), Some(o)) => (c, s, i, o),
                _ => {
                    output.fill(0.0);
                    return Err(TensorRtError::InitializationFailed(
                        "TensorRT context missing hardware resources".to_owned(),
                    ));
                }
            };

            cuda_ctx.make_current().map_err(TensorRtError::Cuda)?;

            // 1. Host to Device DMA transfer
            input_gpu
                .async_copy_from_host(input, stream)
                .map_err(TensorRtError::Cuda)?;

            // 2. Device to host read-back. There is no engine yet, so this reads back the input
            // buffer unchanged (a CUDA round trip, not inference). `copy_to_host` rejects a
            // wrong-sized `output` and synchronizes the stream before returning.
            input_gpu
                .copy_to_host(output, stream)
                .map_err(TensorRtError::Cuda)?;
            let _ = output_gpu;
        }

        // Verify output finiteness invariant: fail-closed zero-bypass protection
        if output.iter().any(|&s| !s.is_finite()) {
            output.fill(0.0);
            return Err(TensorRtError::NonFiniteOutputDetected);
        }

        self.frames_processed = self.frames_processed.saturating_add(1);
        Ok(())
    }
}
