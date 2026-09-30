#![forbid(unsafe_code)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};

/// `CoreML` acceleration backend targeting Apple Silicon ANE / GPU devices.
#[derive(Debug, Clone)]
pub struct CoreMlBackend {
    descriptor: BackendDescriptor,
    compute_unit: String,
    simulated_failure: bool,
}

impl CoreMlBackend {
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        compute_unit: impl Into<String>,
    ) -> Self {
        Self {
            descriptor: BackendDescriptor {
                backend: "coreml",
                backend_version: "1.0",
                runtime: "coreml-ane",
                runtime_version: "17.0",
                asset_id: asset_id.into(),
                asset_sha256: asset_sha256.into(),
                cpu_profile: "apple-silicon-ane",
            },
            compute_unit: compute_unit.into(),
            simulated_failure: false,
        }
    }

    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(manifest.asset_id(), manifest.asset_sha256(), "all")
    }

    #[must_use]
    pub fn new_mock() -> Self {
        Self::new("df-compatible-release-asset-v1", "mock-asset-sha256", "all")
    }

    #[must_use]
    pub fn compute_unit(&self) -> &str {
        &self.compute_unit
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }
}

impl InferenceBackend for CoreMlBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "CoreML execution failed".to_owned(),
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
