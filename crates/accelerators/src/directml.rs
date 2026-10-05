#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};
use realtime_noise_runtime_gpu_compute::directml::{
    DirectMlContext, DirectMlDeviceType, is_directml_available,
};

/// Microsoft DirectML backend slot for DirectX 12 devices (Dedicated and Integrated GPUs) on
/// Windows hosts.
///
/// Known gap: no DirectX 12 device is opened and no operator runs; frames are copied through on
/// the host. [`Self::executes_inference`] is `false` and AUTO never selects this backend.
#[derive(Debug)]
pub struct DirectMlBackend {
    descriptor: BackendDescriptor,
    context: DirectMlContext,
    device_type: DirectMlDeviceType,
    simulated_failure: bool,
}

impl DirectMlBackend {
    /// Creates a DirectML backend for the specified asset and target device.
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_type: DirectMlDeviceType,
    ) -> Self {
        let (runtime, cpu_profile) = match device_type {
            DirectMlDeviceType::DedicatedGpu => ("directml", "dx12-dgpu"),
            DirectMlDeviceType::IntegratedGpu => ("directml-igpu", "dx12-igpu"),
        };

        let context = DirectMlContext::new(device_type)
            .unwrap_or_else(|_| DirectMlContext::new_mock(device_type));

        Self {
            descriptor: BackendDescriptor {
                backend: "directml",
                backend_version: "1.15.0",
                runtime,
                runtime_version: "1.15.0",
                asset_id: asset_id.into(),
                asset_sha256: asset_sha256.into(),
                cpu_profile,
            },
            context,
            device_type,
            simulated_failure: false,
        }
    }

    /// Creates a backend instance from an approved model asset manifest.
    #[must_use]
    pub fn from_manifest(manifest: &ApprovedAssetManifest) -> Self {
        Self::new(
            manifest.asset_id(),
            manifest.asset_sha256(),
            DirectMlDeviceType::DedicatedGpu,
        )
    }

    /// Creates an emulated mock backend targeting Dedicated GPU for testing.
    #[must_use]
    pub fn new_mock() -> Self {
        Self::new_mock_dgpu()
    }

    /// Creates an emulated mock backend targeting Dedicated GPU.
    #[must_use]
    pub fn new_mock_dgpu() -> Self {
        let device_type = DirectMlDeviceType::DedicatedGpu;
        Self {
            descriptor: BackendDescriptor {
                backend: "directml",
                backend_version: "1.15.0",
                runtime: "directml",
                runtime_version: "1.15.0",
                asset_id: "df-compatible-release-asset-v1".to_owned(),
                asset_sha256: "mock-asset-sha256".to_owned(),
                cpu_profile: "dx12-dgpu",
            },
            context: DirectMlContext::new_mock(device_type),
            device_type,
            simulated_failure: false,
        }
    }

    /// Creates an emulated mock backend targeting Integrated GPU.
    #[must_use]
    pub fn new_mock_igpu() -> Self {
        let device_type = DirectMlDeviceType::IntegratedGpu;
        Self {
            descriptor: BackendDescriptor {
                backend: "directml",
                backend_version: "1.15.0",
                runtime: "directml-igpu",
                runtime_version: "1.15.0",
                asset_id: "df-compatible-release-asset-v1".to_owned(),
                asset_sha256: "mock-asset-sha256".to_owned(),
                cpu_profile: "dx12-igpu",
            },
            context: DirectMlContext::new_mock(device_type),
            device_type,
            simulated_failure: false,
        }
    }

    /// Returns true if DirectML is natively available on the host system.
    #[must_use]
    pub fn is_available() -> bool {
        is_directml_available()
    }

    /// Whether frames go through a neural network on the device. Always `false` today: this
    /// backend only copies samples (see the underlying context), so it must not be selected as
    /// an inference accelerator.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        self.context.executes_inference()
    }

    #[must_use]
    pub const fn device_type(&self) -> DirectMlDeviceType {
        self.device_type
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        self.context.set_simulated_failure(fail);
    }
}

impl InferenceBackend for DirectMlBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "DirectML compute execution failed".to_owned(),
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

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn test_directml_backend_mock_process() {
        let mut backend = DirectMlBackend::new_mock_dgpu();
        assert_eq!(backend.descriptor().backend, "directml");
        assert_eq!(backend.descriptor().runtime, "directml");
        assert_eq!(backend.device_type(), DirectMlDeviceType::DedicatedGpu);
        assert_eq!(backend.algorithmic_latency_samples(), 1_440);

        let input: AudioFrame = [0.1f32; realtime_noise_contracts::HOP_SAMPLES];
        let processed = backend.process(&input);
        assert!(processed.is_ok());
        let frame = processed.unwrap_or_else(|_| unreachable!());
        assert_eq!(frame.samples, input);
        assert_eq!(frame.algorithmic_latency_samples, 1_440);
    }

    #[test]
    fn test_directml_backend_rejects_non_finite_sample() {
        let mut backend = DirectMlBackend::new_mock();
        let mut input: AudioFrame = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        input[10] = f32::NAN;

        let res = backend.process(&input);
        assert!(matches!(res, Err(InferenceError::InputContract(_))));
    }

    #[test]
    fn test_directml_backend_simulated_failure() {
        let mut backend = DirectMlBackend::new_mock();
        backend.set_simulated_failure(true);

        let input: AudioFrame = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        let res = backend.process(&input);
        assert!(matches!(res, Err(InferenceError::InferenceExecution(_))));
    }

    #[test]
    fn test_directml_igpu_variant() {
        let backend = DirectMlBackend::new_mock_igpu();
        assert_eq!(backend.device_type(), DirectMlDeviceType::IntegratedGpu);
        assert_eq!(backend.descriptor().runtime, "directml-igpu");
    }
}
