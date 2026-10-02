#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::unreadable_literal,
    clippy::redundant_clone
)]

use realtime_noise_accelerators::{
    AutoPolicy, BackendRequest, BackendSelection, DeviceTier, DirectMlBackend, PromotionDecision,
    RyzenAiBackend, VulkanBackend, evaluate_calibration,
};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::{InferenceBackend, InferenceError};
use realtime_noise_runtime_gpu_compute::directml::DirectMlDeviceType;
use realtime_noise_runtime_gpu_compute::ryzenai::RyzenAiDeviceType;
use realtime_noise_runtime_gpu_compute::vulkan::{SPIRV_COMPUTE_WORDS, VulkanDeviceType};

#[test]
fn test_directml_backend_integration() {
    let dml_dgpu = DirectMlBackend::new_mock_dgpu();
    let dml_igpu = DirectMlBackend::new_mock_igpu();

    // Verify descriptors
    assert_eq!(dml_dgpu.descriptor().backend, "directml");
    assert_eq!(dml_dgpu.descriptor().runtime, "directml");
    assert_eq!(dml_dgpu.descriptor().cpu_profile, "dx12-dgpu");
    assert_eq!(dml_dgpu.device_type(), DirectMlDeviceType::DedicatedGpu);

    assert_eq!(dml_igpu.descriptor().backend, "directml");
    assert_eq!(dml_igpu.descriptor().runtime, "directml-igpu");
    assert_eq!(dml_igpu.descriptor().cpu_profile, "dx12-igpu");
    assert_eq!(dml_igpu.device_type(), DirectMlDeviceType::IntegratedGpu);

    // Verify InferenceBackend polymorphism
    let mut boxed_backend: Box<dyn InferenceBackend> = Box::new(dml_dgpu);
    let input: AudioFrame = [0.123f32; HOP_SAMPLES];
    let output = boxed_backend.process(&input).unwrap();
    assert_eq!(output.samples, input);
    assert_eq!(output.algorithmic_latency_samples, 1_440);

    // Verify non-finite rejection
    let mut invalid_input: AudioFrame = [0.0f32; HOP_SAMPLES];
    invalid_input[42] = f32::NAN;
    assert!(matches!(
        boxed_backend.process(&invalid_input),
        Err(InferenceError::InputContract(_))
    ));
}

#[test]
fn test_vulkan_backend_integration() {
    let vulkan_dgpu = VulkanBackend::new_mock_dgpu();
    let vulkan_igpu = VulkanBackend::new_mock_igpu();

    // Verify descriptors
    assert_eq!(vulkan_dgpu.descriptor().backend, "vulkan");
    assert_eq!(vulkan_dgpu.descriptor().runtime, "vulkan-spirv");
    assert_eq!(vulkan_dgpu.descriptor().cpu_profile, "vulkan-dgpu");
    assert_eq!(vulkan_dgpu.device_type(), VulkanDeviceType::DedicatedGpu);

    assert_eq!(vulkan_igpu.descriptor().backend, "vulkan");
    assert_eq!(vulkan_igpu.descriptor().runtime, "vulkan-spirv-igpu");
    assert_eq!(vulkan_igpu.descriptor().cpu_profile, "vulkan-igpu");
    assert_eq!(vulkan_igpu.device_type(), VulkanDeviceType::IntegratedGpu);

    // Verify SPIR-V compute binary words
    assert_eq!(SPIRV_COMPUTE_WORDS[0], 0x07230203);
    assert!(!SPIRV_COMPUTE_WORDS.is_empty());

    // Verify InferenceBackend polymorphism
    let mut boxed_backend: Box<dyn InferenceBackend> = Box::new(vulkan_dgpu);
    let input: AudioFrame = [0.456f32; HOP_SAMPLES];
    let output = boxed_backend.process(&input).unwrap();
    assert_eq!(output.samples, input);
    assert_eq!(output.algorithmic_latency_samples, 1_440);

    // Verify non-finite rejection
    let mut invalid_input: AudioFrame = [0.0f32; HOP_SAMPLES];
    invalid_input[100] = f32::INFINITY;
    assert!(matches!(
        boxed_backend.process(&invalid_input),
        Err(InferenceError::InputContract(_))
    ));
}

#[test]
fn test_ryzenai_backend_integration() {
    let ryzen_npu = RyzenAiBackend::new_mock_npu();
    let ryzen_gpu = RyzenAiBackend::new_mock_gpu();

    // Verify descriptors and types
    assert_eq!(ryzen_npu.descriptor().backend, "ryzenai");
    assert_eq!(ryzen_npu.descriptor().runtime, "ryzenai-npu");
    assert_eq!(ryzen_npu.descriptor().cpu_profile, "amd-xdna-npu");
    assert!(ryzen_npu.is_npu());
    assert!(!ryzen_npu.is_gpu());
    assert_eq!(ryzen_npu.device_type(), RyzenAiDeviceType::XdnaNpu);

    assert_eq!(ryzen_gpu.descriptor().backend, "ryzenai");
    assert_eq!(ryzen_gpu.descriptor().runtime, "ryzenai-gpu");
    assert_eq!(ryzen_gpu.descriptor().cpu_profile, "amd-rdna-igpu");
    assert!(ryzen_gpu.is_gpu());
    assert!(!ryzen_gpu.is_npu());
    assert_eq!(ryzen_gpu.device_type(), RyzenAiDeviceType::RdnaGpu);

    // Verify InferenceBackend polymorphism
    let mut boxed_backend: Box<dyn InferenceBackend> = Box::new(ryzen_npu);
    let input: AudioFrame = [0.789f32; HOP_SAMPLES];
    let output = boxed_backend.process(&input).unwrap();
    assert_eq!(output.samples, input);
    assert_eq!(output.algorithmic_latency_samples, 1_440);

    // Verify non-finite rejection
    let mut invalid_input: AudioFrame = [0.0f32; HOP_SAMPLES];
    invalid_input[0] = f32::NEG_INFINITY;
    assert!(matches!(
        boxed_backend.process(&invalid_input),
        Err(InferenceError::InputContract(_))
    ));
}

#[test]
fn test_backend_selection_factory_polymorphism() {
    let dml_backend = BackendSelection::DirectMl.instantiate_mock_backend();
    assert_eq!(dml_backend.descriptor().backend, "directml");
    assert_eq!(dml_backend.algorithmic_latency_samples(), 1_440);

    let vulkan_backend = BackendSelection::Vulkan.instantiate_mock_backend();
    assert_eq!(vulkan_backend.descriptor().backend, "vulkan");
    assert_eq!(vulkan_backend.algorithmic_latency_samples(), 1_440);

    let ryzen_npu_backend = BackendSelection::RyzenAiNpu.instantiate_mock_backend();
    assert_eq!(ryzen_npu_backend.descriptor().backend, "ryzenai");
    assert_eq!(ryzen_npu_backend.descriptor().runtime, "ryzenai-npu");
    assert_eq!(ryzen_npu_backend.algorithmic_latency_samples(), 1_440);

    let ryzen_gpu_backend = BackendSelection::RyzenAiGpu.instantiate_mock_backend();
    assert_eq!(ryzen_gpu_backend.descriptor().backend, "ryzenai");
    assert_eq!(ryzen_gpu_backend.descriptor().runtime, "ryzenai-gpu");
    assert_eq!(ryzen_gpu_backend.algorithmic_latency_samples(), 1_440);
}

#[test]
fn test_auto_policy_routing_for_universal_runtimes() {
    let policy = AutoPolicy::new();

    let dml_report = evaluate_calibration("directml", 300.0, 30_000, 2.0, 4.0, 6.0, 8.0, 0, 0);
    assert_eq!(dml_report.decision, PromotionDecision::Promoted);

    let vulkan_report = evaluate_calibration("vulkan", 300.0, 30_000, 2.5, 4.5, 6.5, 8.5, 0, 0);
    assert_eq!(vulkan_report.decision, PromotionDecision::Promoted);

    let ryzen_npu_report =
        evaluate_calibration("ryzenai-npu", 300.0, 30_000, 3.0, 5.0, 7.0, 9.0, 0, 0);
    assert_eq!(ryzen_npu_report.decision, PromotionDecision::Promoted);

    let ryzen_gpu_report =
        evaluate_calibration("ryzenai-gpu", 300.0, 30_000, 3.5, 5.5, 7.5, 9.5, 0, 0);
    assert_eq!(ryzen_gpu_report.decision, PromotionDecision::Promoted);

    // None of these backends runs inference yet (they only copy samples), so a promoted report
    // for them is never selected: every request falls back to the tract baseline. Their tier
    // metadata stays correct for when an inference path lands.
    let requests_and_reports = [
        (BackendRequest::DirectMl, &dml_report),
        (BackendRequest::Vulkan, &vulkan_report),
        (BackendRequest::RyzenAiNpu, &ryzen_npu_report),
        (BackendRequest::RyzenAiGpu, &ryzen_gpu_report),
    ];
    for (request, report) in requests_and_reports {
        assert_eq!(
            policy.resolve_request(request, Some(report)),
            BackendSelection::TractCpu,
            "{request:?}"
        );
    }

    assert!(BackendSelection::DirectMl.is_directml());
    assert_eq!(BackendSelection::DirectMl.tier(), DeviceTier::DedicatedGpu);
    assert!(BackendSelection::Vulkan.is_vulkan());
    assert_eq!(BackendSelection::Vulkan.tier(), DeviceTier::DedicatedGpu);
    assert!(BackendSelection::RyzenAiNpu.is_npu());
    assert!(BackendSelection::RyzenAiNpu.is_amd());
    assert_eq!(BackendSelection::RyzenAiNpu.tier(), DeviceTier::Npu);
    assert!(BackendSelection::RyzenAiGpu.is_integrated_gpu());
    assert!(BackendSelection::RyzenAiGpu.is_amd());
    assert_eq!(
        BackendSelection::RyzenAiGpu.tier(),
        DeviceTier::IntegratedGpu
    );

    // Multi-candidate AUTO: the ranking scores (DirectML 90 > Vulkan 85 > Ryzen AI NPU 80 > iGPU
    // 70) no longer matter, because none of the candidates is allowed to win.
    let candidates = [
        ryzen_gpu_report,
        ryzen_npu_report,
        vulkan_report,
        dml_report,
    ];
    let best = policy.resolve_candidates(BackendRequest::Auto, &candidates);
    assert_eq!(best, BackendSelection::TractCpu);
}
