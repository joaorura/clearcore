#![forbid(unsafe_code)]
#![allow(clippy::doc_markdown)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, BackendDescriptor, InferenceBackend,
    InferenceError, ProcessedFrame,
};
use realtime_noise_runtime_gpu_compute::vulkan::{
    VulkanContext, VulkanDeviceType, is_vulkan_available,
};

/// Vulkan backend slot for GPUs on Linux and Windows.
///
/// Known gap: no Vulkan device is opened and no shader runs; frames are copied through on the
/// host. [`Self::executes_inference`] is `false` and AUTO never selects this backend.
#[derive(Debug)]
pub struct VulkanBackend {
    descriptor: BackendDescriptor,
    context: VulkanContext,
    device_type: VulkanDeviceType,
    simulated_failure: bool,
}

impl VulkanBackend {
    /// Creates a Vulkan compute backend for the specified asset and target device.
    pub fn new(
        asset_id: impl Into<String>,
        asset_sha256: impl Into<String>,
        device_type: VulkanDeviceType,
    ) -> Self {
        let (runtime, cpu_profile) = match device_type {
            VulkanDeviceType::DedicatedGpu => ("vulkan-spirv", "vulkan-dgpu"),
            VulkanDeviceType::IntegratedGpu => ("vulkan-spirv-igpu", "vulkan-igpu"),
        };

        let context = VulkanContext::new(device_type)
            .unwrap_or_else(|_| VulkanContext::new_mock(device_type));

        Self {
            descriptor: BackendDescriptor {
                backend: "vulkan",
                backend_version: "1.3.0",
                runtime,
                runtime_version: "1.3.0",
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
            VulkanDeviceType::DedicatedGpu,
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
        let device_type = VulkanDeviceType::DedicatedGpu;
        Self {
            descriptor: BackendDescriptor {
                backend: "vulkan",
                backend_version: "1.3.0",
                runtime: "vulkan-spirv",
                runtime_version: "1.3.0",
                asset_id: "df-compatible-release-asset-v1".to_owned(),
                asset_sha256: "mock-asset-sha256".to_owned(),
                cpu_profile: "vulkan-dgpu",
            },
            context: VulkanContext::new_mock(device_type),
            device_type,
            simulated_failure: false,
        }
    }

    /// Creates an emulated mock backend targeting Integrated GPU.
    #[must_use]
    pub fn new_mock_igpu() -> Self {
        let device_type = VulkanDeviceType::IntegratedGpu;
        Self {
            descriptor: BackendDescriptor {
                backend: "vulkan",
                backend_version: "1.3.0",
                runtime: "vulkan-spirv-igpu",
                runtime_version: "1.3.0",
                asset_id: "df-compatible-release-asset-v1".to_owned(),
                asset_sha256: "mock-asset-sha256".to_owned(),
                cpu_profile: "vulkan-igpu",
            },
            context: VulkanContext::new_mock(device_type),
            device_type,
            simulated_failure: false,
        }
    }

    /// Returns true if Vulkan loader and hardware compute are available on the host system.
    #[must_use]
    pub fn is_available() -> bool {
        is_vulkan_available()
    }

    /// Whether frames go through a neural network on the device. Always `false` today: this
    /// backend only copies samples (see the underlying context), so it must not be selected as
    /// an inference accelerator.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        self.context.executes_inference()
    }

    #[must_use]
    pub const fn device_type(&self) -> VulkanDeviceType {
        self.device_type
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
        self.context.set_simulated_failure(fail);
    }
}

impl InferenceBackend for VulkanBackend {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if self.simulated_failure {
            return Err(InferenceError::InferenceExecution(
                "Vulkan SPIR-V compute execution failed".to_owned(),
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
    fn test_vulkan_backend_mock_process() {
        let mut backend = VulkanBackend::new_mock_dgpu();
        assert_eq!(backend.descriptor().backend, "vulkan");
        assert_eq!(backend.descriptor().runtime, "vulkan-spirv");
        assert_eq!(backend.device_type(), VulkanDeviceType::DedicatedGpu);
        assert_eq!(backend.algorithmic_latency_samples(), 1_440);

        let input: AudioFrame = [0.2f32; realtime_noise_contracts::HOP_SAMPLES];
        let processed = backend.process(&input);
        assert!(processed.is_ok());
        let frame = processed.unwrap_or_else(|_| unreachable!());
        assert_eq!(frame.samples, input);
        assert_eq!(frame.algorithmic_latency_samples, 1_440);
    }

    #[test]
    fn test_vulkan_backend_rejects_non_finite_sample() {
        let mut backend = VulkanBackend::new_mock();
        let mut input: AudioFrame = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        input[5] = f32::INFINITY;

        let res = backend.process(&input);
        assert!(matches!(res, Err(InferenceError::InputContract(_))));
    }

    #[test]
    fn test_vulkan_backend_simulated_failure() {
        let mut backend = VulkanBackend::new_mock();
        backend.set_simulated_failure(true);

        let input: AudioFrame = [0.0f32; realtime_noise_contracts::HOP_SAMPLES];
        let res = backend.process(&input);
        assert!(matches!(res, Err(InferenceError::InferenceExecution(_))));
    }

    #[test]
    fn test_vulkan_igpu_variant() {
        let backend = VulkanBackend::new_mock_igpu();
        assert_eq!(backend.device_type(), VulkanDeviceType::IntegratedGpu);
        assert_eq!(backend.descriptor().runtime, "vulkan-spirv-igpu");
    }
}
