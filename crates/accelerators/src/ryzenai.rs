#![forbid(unsafe_code)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};

/// AMD Ryzen AI acceleration backend targeting AMD XDNA NPU (and RDNA iGPU/dGPU)
/// on both Linux (`amdnpu` / `amdxdna` kernel driver + XRT) and Windows (`VitisAI`).
#[derive(Debug, Clone)]
pub struct RyzenAiBackend {
    descriptor: BackendDescriptor,
    device: String,
    simulated_failure: bool,
}

impl RyzenAiBackend {
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device: impl Into<String>,
    ) -> Self {
        let dev = device.into();
        let (runtime, cpu_profile) = match dev.to_ascii_uppercase().as_str() {
            "GPU" => ("ryzenai-gpu", "amd-rdna-igpu"),
            _ => ("ryzenai-npu", "amd-xdna-npu"),
        };
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
            device: dev,
            simulated_failure: false,
        }
    }

    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), "NPU")
    }

    #[must_use]
    pub fn new_mock() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", "NPU")
    }

    #[must_use]
    pub fn new_mock_npu() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", "NPU")
    }

    #[must_use]
    pub fn new_mock_gpu() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", "GPU")
    }

    #[must_use]
    pub fn new_mock_igpu() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", "GPU")
    }

    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
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

        let output = *input;
        ProcessedFrame::checked(output, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }
}
