#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

//! NVIDIA `TensorRT` acceleration backend.
//!
//! # Wiring
//!
//! - **Detection**: [`TensorRtBackend::is_available`] loads the CUDA driver and `libnvinfer`
//!   dynamically through `realtime-noise-runtime-tensorrt`; hosts without the NVIDIA stack just
//!   see `false`. [`crate::HardwareScanner`] uses the same discovery for its audit.
//! - **Loading**: [`TensorRtBackend::try_new_hardware`] opens the CUDA context, stream and VRAM
//!   buffers on one GPU. [`TensorRtBackend::new`] does the same but degrades to a mock context
//!   (see [`TensorRtBackend::fallback_reason`]) when the stack is missing, mirroring the other
//!   runtimes.
//! - **Precision**: the Clearcore policy is applied per GPU by
//!   `GpuDevice::preferred_precision`: FP8 on `sm_89` and later, else FP16, else INT8. FP32 is
//!   never chosen for a GPU, so a GPU supporting none of those yields an error and the caller
//!   falls back to another backend.
//! - **Strictly TensorRT**: raw CUDA execution is excluded; there is no `CudaBackend`.
//!
//! # Known gap: no engine execution yet
//!
//! `libnvinfer` exposes only a C++ vtable API and the FFI crate binds just the version queries.
//! Until engine deserialization and `enqueueV3` are bound, the hardware path performs a real CUDA
//! host-to-device-to-host round trip on the selected GPU and returns the input unchanged. Check
//! [`TensorRtBackend::executes_engine`] before treating this backend as neural inference.

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};
use realtime_noise_runtime_tensorrt::{
    CudaDevice, PrecisionTarget, TensorRtError, TensorRtInferenceContext, TensorRtRuntime,
};

const DEFAULT_ASSET_ID: &str = "df-compatible-release-asset-v1";
const MOCK_ASSET_SHA256: &str = "mock-asset-sha256";

/// Static label for the GPU architecture, as carried by [`BackendDescriptor::cpu_profile`].
fn profile_for_sm(sm: &str) -> &'static str {
    match sm {
        "sm_61" => "sm_61",
        "sm_70" => "sm_70",
        "sm_75" => "sm_75",
        "sm_80" => "sm_80",
        "sm_86" => "sm_86",
        "sm_87" => "sm_87",
        "sm_89" => "sm_89",
        "sm_90" => "sm_90",
        "sm_100" => "sm_100",
        "sm_103" => "sm_103",
        "sm_120" => "sm_120",
        "sm_121" => "sm_121",
        _ => "nvidia-gpu",
    }
}

fn execution_error(error: &TensorRtError) -> InferenceError {
    InferenceError::InferenceExecution(error.to_string())
}

/// NVIDIA `TensorRT` acceleration backend targeting NVIDIA GPUs.
/// Strictly uses `TensorRT`; raw CUDA execution is excluded.
#[derive(Debug)]
pub struct TensorRtBackend {
    descriptor: BackendDescriptor,
    device_id: u32,
    context: TensorRtInferenceContext,
    fallback_reason: Option<String>,
    simulated_failure: bool,
}

impl TensorRtBackend {
    fn descriptor_for(
        context: &TensorRtInferenceContext,
        asset_id: String,
        asset_sha256: String,
    ) -> BackendDescriptor {
        BackendDescriptor {
            backend: "tensorrt",
            // The descriptor is `'static`, so it cannot carry the version of the `libnvinfer`
            // that was actually loaded: that is `native_runtime_version()`. It claims only what
            // is true of this build: the integration's own version, and an unknown runtime.
            backend_version: env!("CARGO_PKG_VERSION"),
            runtime: "tensorrt",
            runtime_version: "unknown",
            asset_id,
            asset_sha256,
            cpu_profile: profile_for_sm(&context.device().sm_string()),
        }
    }

    fn from_context(
        context: TensorRtInferenceContext,
        device_id: u32,
        asset_id: String,
        asset_sha256: String,
        fallback_reason: Option<String>,
    ) -> Self {
        Self {
            descriptor: Self::descriptor_for(&context, asset_id, asset_sha256),
            device_id,
            context,
            fallback_reason,
            simulated_failure: false,
        }
    }

    /// Opens a real CUDA/TensorRT context on `device_id`, choosing the precision with the
    /// Clearcore policy. Fails when the NVIDIA stack is missing or the GPU supports none of
    /// FP8/FP16/INT8, so callers can fall back to another backend.
    pub fn try_new_hardware(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_id: u32,
    ) -> Result<Self, InferenceError> {
        let context =
            TensorRtInferenceContext::new_auto(device_id).map_err(|e| execution_error(&e))?;
        Ok(Self::from_context(
            context,
            device_id,
            asset_id.into(),
            asset_sha256.into(),
            None,
        ))
    }

    /// Like [`Self::try_new_hardware`], but degrades to a mock Blackwell context when the NVIDIA
    /// stack is unavailable. [`Self::is_hardware_backed`] and [`Self::fallback_reason`] tell
    /// which one you got.
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_id: u32,
    ) -> Self {
        let asset_id = asset_id.into();
        let asset_sha256 = asset_sha256.into();
        match TensorRtInferenceContext::new_auto(device_id) {
            Ok(context) => Self::from_context(context, device_id, asset_id, asset_sha256, None),
            Err(error) => Self::from_context(
                TensorRtInferenceContext::new_mock_sm120_fp8(),
                device_id,
                asset_id,
                asset_sha256,
                Some(error.to_string()),
            ),
        }
    }

    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), 0)
    }

    /// Mock backend on an emulated Blackwell `sm_120` (FP8); never touches the host GPU.
    #[must_use]
    pub fn new_mock() -> Self {
        Self::from_context(
            TensorRtInferenceContext::new_mock_sm120_fp8(),
            0,
            DEFAULT_ASSET_ID.to_owned(),
            MOCK_ASSET_SHA256.to_owned(),
            None,
        )
    }

    /// Mock backend on an emulated GPU, with the precision the policy picks for it.
    pub fn new_mock_for_device(device: CudaDevice) -> Result<Self, InferenceError> {
        let name = device.name().to_owned();
        let context = TensorRtInferenceContext::new_mock_preferred(device).ok_or_else(|| {
            InferenceError::InferenceExecution(format!(
                "{name} supports none of FP8/FP16/INT8; another backend must be used"
            ))
        })?;
        Ok(Self::from_context(
            context,
            0,
            DEFAULT_ASSET_ID.to_owned(),
            MOCK_ASSET_SHA256.to_owned(),
            None,
        ))
    }

    /// Whether the CUDA driver and `libnvinfer` can both be loaded on this host.
    #[must_use]
    pub fn is_available() -> bool {
        TensorRtRuntime::is_available()
    }

    #[must_use]
    pub const fn device_id(&self) -> u32 {
        self.device_id
    }

    /// Precision selected for the GPU by the Clearcore policy.
    #[must_use]
    pub const fn precision(&self) -> PrecisionTarget {
        self.context.precision()
    }

    /// Name of the GPU (or of the emulated GPU for mock backends).
    #[must_use]
    pub fn device_name(&self) -> &str {
        self.context.device().name()
    }

    /// Compute capability string such as `sm_120`.
    #[must_use]
    pub fn sm_version(&self) -> String {
        self.context.device().sm_string()
    }

    /// True when a real CUDA context, stream and VRAM buffers back this backend.
    #[must_use]
    pub const fn is_hardware_backed(&self) -> bool {
        self.context.is_hardware_backed()
    }

    /// Whether frames run through a deserialized TensorRT engine. Always false today; see the
    /// module docs for the gap.
    #[must_use]
    pub const fn executes_engine(&self) -> bool {
        self.context.executes_engine()
    }

    /// Whether frames go through a neural network. Same as [`Self::executes_engine`]: the CUDA
    /// round trip is a copy, not inference.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        self.executes_engine()
    }

    /// Why [`Self::new`] had to use a mock context, if it did.
    #[must_use]
    pub fn fallback_reason(&self) -> Option<&str> {
        self.fallback_reason.as_deref()
    }

    /// Version of the loaded `libnvinfer`, or `None` for mock backends.
    #[must_use]
    pub fn native_runtime_version(&self) -> Option<&str> {
        self.is_hardware_backed()
            .then(|| self.context.runtime_version())
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        self.context.set_simulated_failure(fail);
    }
}

impl InferenceBackend for TensorRtBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "TensorRT engine execution failed".to_owned(),
            ));
        }

        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }

        let mut output = [0.0f32; HOP_SAMPLES];
        self.context
            .process_frame(input, &mut output)
            .map_err(|error| execution_error(&error))?;
        ProcessedFrame::checked(output, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }

    fn set_voice_profile(
        &mut self,
        profile: Option<&realtime_noise_model::VoiceProfile>,
    ) -> Result<(), realtime_noise_model::InferenceError> {
        realtime_noise_model::reject_unsupported_voice_profile(profile)
    }

    fn supports_voice_profile(&self) -> bool {
        false
    }
}
