#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};
use realtime_noise_runtime_gpu_compute::ryzenai::{
    RyzenAiContext, RyzenAiDeviceType, RyzenAiDriverKind, detect_driver, is_ryzenai_available,
};

/// AMD Ryzen AI backend slot for the XDNA NPU (and RDNA iGPU/dGPU) on Linux (`amdnpu` /
/// `amdxdna` kernel driver + XRT) and Windows (`VitisAI`).
///
/// Known gap: nothing is loaded into XRT or `VitisAI`; frames are copied through on the host.
/// [`Self::executes_inference`] is `false` and AUTO never selects this backend.
#[derive(Debug)]
pub struct RyzenAiBackend {
    descriptor: BackendDescriptor,
    context: RyzenAiContext,
    device: String,
    device_type: RyzenAiDeviceType,
    simulated_failure: bool,
}

impl RyzenAiBackend {
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device: impl Into<String>,
    ) -> Self {
        let dev = device.into();
        let (device_type, runtime, cpu_profile) = match dev.to_ascii_uppercase().as_str() {
            "GPU" => (RyzenAiDeviceType::RdnaGpu, "ryzenai-gpu", "amd-rdna-igpu"),
            _ => (RyzenAiDeviceType::XdnaNpu, "ryzenai-npu", "amd-xdna-npu"),
        };

        let context = RyzenAiContext::new(device_type)
            .unwrap_or_else(|_| RyzenAiContext::new_mock(device_type));

        Self {
            descriptor: BackendDescriptor {
                backend: "ryzenai",
                backend_version: "1.2.0",
                runtime,
                runtime_version: "1.2.0",
                asset_id: asset_id.into(),
                asset_sha256: asset_sha256.into(),
                cpu_profile,
            },
            context,
            device: dev,
            device_type,
            simulated_failure: false,
        }
    }

    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), "NPU")
    }

    #[must_use]
    pub fn new_mock() -> Self {
        Self::new_mock_npu()
    }

    #[must_use]
    pub fn new_mock_npu() -> Self {
        let device_type = RyzenAiDeviceType::XdnaNpu;
        Self {
            descriptor: BackendDescriptor {
                backend: "ryzenai",
                backend_version: "1.2.0",
                runtime: "ryzenai-npu",
                runtime_version: "1.2.0",
                asset_id: "df-compatible-release-asset-v1".to_owned(),
                asset_sha256: "mock-asset-sha256".to_owned(),
                cpu_profile: "amd-xdna-npu",
            },
            context: RyzenAiContext::new_mock(device_type),
            device: "NPU".to_owned(),
            device_type,
            simulated_failure: false,
        }
    }

    #[must_use]
    pub fn new_mock_gpu() -> Self {
        let device_type = RyzenAiDeviceType::RdnaGpu;
        Self {
            descriptor: BackendDescriptor {
                backend: "ryzenai",
                backend_version: "1.2.0",
                runtime: "ryzenai-gpu",
                runtime_version: "1.2.0",
                asset_id: "df-compatible-release-asset-v1".to_owned(),
                asset_sha256: "mock-asset-sha256".to_owned(),
                cpu_profile: "amd-rdna-igpu",
            },
            context: RyzenAiContext::new_mock(device_type),
            device: "GPU".to_owned(),
            device_type,
            simulated_failure: false,
        }
    }

    #[must_use]
    pub fn new_mock_igpu() -> Self {
        Self::new_mock_gpu()
    }

    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Whether frames go through a neural network on the device. Always `false` today: this
    /// backend only copies samples (see the underlying context), so it must not be selected as
    /// an inference accelerator.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        self.context.executes_inference()
    }

    #[must_use]
    pub const fn device_type(&self) -> RyzenAiDeviceType {
        self.device_type
    }

    #[must_use]
    pub const fn is_npu(&self) -> bool {
        self.device_type.is_npu()
    }

    #[must_use]
    pub const fn is_gpu(&self) -> bool {
        self.device_type.is_gpu()
    }

    #[must_use]
    pub const fn driver_kind(&self) -> RyzenAiDriverKind {
        self.context.driver_kind()
    }

    #[must_use]
    pub fn is_available() -> bool {
        is_ryzenai_available()
    }

    #[must_use]
    pub fn detected_driver() -> RyzenAiDriverKind {
        detect_driver()
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        self.context.set_simulated_failure(fail);
    }
}

impl InferenceBackend for RyzenAiBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "Ryzen AI hardware execution failed".to_owned(),
            ));
        }

        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }

        let mut output = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        self.context
            .process_frame(input, &mut output)
            .map_err(|e| InferenceError::InferenceExecution(e.to_string()))?;

        ProcessedFrame::checked(output, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_ryzenai_npu_mock_process() {
        let mut backend = RyzenAiBackend::new_mock_npu();
        assert_eq!(backend.descriptor().backend, "ryzenai");
        assert_eq!(backend.descriptor().runtime, "ryzenai-npu");
        assert!(backend.is_npu());
        assert!(!backend.is_gpu());
        assert_eq!(backend.algorithmic_latency_samples(), 1_440);

        let input: AudioFrame = [0.3f32; realtime_noise_contracts::HOP_SAMPLES];
        let processed = backend.process(&input);
        assert!(processed.is_ok());
        let frame = processed.unwrap_or_else(|_| unreachable!());
        assert_eq!(frame.samples, input);
        assert_eq!(frame.algorithmic_latency_samples, 1_440);
    }

    #[test]
    fn test_ryzenai_gpu_mock_process() {
        let mut backend = RyzenAiBackend::new_mock_gpu();
        assert_eq!(backend.descriptor().backend, "ryzenai");
        assert_eq!(backend.descriptor().runtime, "ryzenai-gpu");
        assert!(backend.is_gpu());
        assert!(!backend.is_npu());

        let input: AudioFrame = [0.1f32; realtime_noise_contracts::HOP_SAMPLES];
        let processed = backend.process(&input);
        assert!(processed.is_ok());
    }

    #[test]
    fn test_ryzenai_rejects_non_finite_sample() {
        let mut backend = RyzenAiBackend::new_mock();
        let mut input: AudioFrame = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        input[0] = f32::NAN;

        let res = backend.process(&input);
        assert!(matches!(res, Err(InferenceError::InputContract(_))));
    }

    #[test]
    fn test_ryzenai_simulated_failure() {
        let mut backend = RyzenAiBackend::new_mock();
        backend.set_simulated_failure(true);

        let input: AudioFrame = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        let res = backend.process(&input);
        assert!(matches!(res, Err(InferenceError::InferenceExecution(_))));
    }
}
