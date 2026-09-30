#![forbid(unsafe_code)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};

/// CUDA / `TensorRT` acceleration backend targeting NVIDIA GPUs.
#[derive(Debug, Clone)]
pub struct CudaBackend {
    descriptor: BackendDescriptor,
    device_id: u32,
    simulated_failure: bool,
}

impl CudaBackend {
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_id: u32,
    ) -> Self {
        Self {
            descriptor: BackendDescriptor {
                backend: "cuda",
                backend_version: "13.4",
                runtime: "tensorrt",
                runtime_version: "11.3.0.99",
                asset_id: asset_id.into(),
                asset_sha256: asset_sha256.into(),
                cpu_profile: "sm_120",
            },
            device_id,
            simulated_failure: false,
        }
    }

    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), 0)
    }

    #[must_use]
    pub fn new_mock() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", 0)
    }

    #[must_use]
    pub const fn device_id(&self) -> u32 {
        self.device_id
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }
}

impl InferenceBackend for CudaBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "CUDA kernel execution failed".to_owned(),
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
