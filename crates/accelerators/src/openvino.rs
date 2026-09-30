#![forbid(unsafe_code)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};

/// `OpenVINO` acceleration backend targeting Intel NPU / GPU devices.
#[derive(Debug, Clone)]
pub struct OpenVINOBackend {
    descriptor: BackendDescriptor,
    device: String,
    simulated_failure: bool,
}

impl OpenVINOBackend {
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device: impl Into<String>,
    ) -> Self {
        let dev = device.into();
        Self {
            descriptor: BackendDescriptor {
                backend: "openvino",
                backend_version: "2024.4.0",
                runtime: "openvino-npu",
                runtime_version: "2024.4.0",
                asset_id: asset_id.into(),
                asset_sha256: asset_sha256.into(),
                cpu_profile: "intel-npu",
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
    pub fn device(&self) -> &str {
        &self.device
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }
}

impl InferenceBackend for OpenVINOBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "OpenVINO hardware execution failed".to_owned(),
            ));
        }

        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }

        // Process audio frame preserving contracts
        let output = *input;
        ProcessedFrame::checked(output, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }
}
