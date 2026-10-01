#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::redundant_clone
)]

use realtime_noise_accelerators::{
    AutoPolicy, BackendRequest, BackendSelection, CalibrationReport, DeviceTier, PromotionDecision,
    select_auto, select_best,
};

#[test]
fn auto_uses_warmed_tract_when_plugin_fails_quality_gate() {
    // 1. When an accelerator fails calibration (NotPromoted), AUTO selects tract CPU.
    let failed_report = CalibrationReport {
        backend_name: "cuda".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 2.0,
        p95_latency_ms: 5.0,
        p99_latency_ms: 12.5, // exceeds 10.0 ms threshold
        max_latency_ms: 18.0,
        deadline_miss_count: 5,
        discontinuities: 2,
        decision: PromotionDecision::NotPromoted,
        reason: Some("Failed quality gate: p99 latency exceeded 10.0 ms".to_owned()),
    };

    let selection = select_auto(failed_report.clone());
    assert_eq!(selection, BackendSelection::TractCpu);
    assert_eq!(selection.name(), "tract");
    assert!(selection.is_tract_cpu());

    // 2. Even if report claims "Promoted", but has quality gate violations, AUTO rejects and selects TractCpu.
    let deceitful_report = CalibrationReport {
        backend_name: "openvino".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.0,
        p95_latency_ms: 2.0,
        p99_latency_ms: 11.0, // Quality gate violation: > 10.0 ms
        max_latency_ms: 12.0,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let selection = select_auto(deceitful_report);
    assert_eq!(selection, BackendSelection::TractCpu);
    assert!(selection.is_tract_cpu());

    // 3. Discontinuity violation forces fallback to tract CPU.
    let discontinuity_report = CalibrationReport {
        backend_name: "coreml".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.0,
        p95_latency_ms: 2.0,
        p99_latency_ms: 4.0,
        max_latency_ms: 5.0,
        deadline_miss_count: 0,
        discontinuities: 1, // Non-zero discontinuity
        decision: PromotionDecision::Promoted,
        reason: None,
    };
    assert_eq!(
        select_auto(discontinuity_report),
        BackendSelection::TractCpu
    );

    // 4. Test AutoPolicy struct selection behavior
    let policy = AutoPolicy::new();
    let selection = policy.select(&failed_report);
    assert_eq!(selection, BackendSelection::TractCpu);
}

#[test]
fn auto_selects_accelerator_when_quality_gate_passes() {
    let passing_trt_report = CalibrationReport {
        backend_name: "tensorrt".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.50,
        p95_latency_ms: 0.85,
        p99_latency_ms: 1.14,
        max_latency_ms: 1.82,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let selection = select_auto(passing_trt_report);
    assert_eq!(selection, BackendSelection::TensorRt);
    assert_eq!(selection.name(), "tensorrt");
    assert!(!selection.is_tract_cpu());
}

#[test]
fn auto_policy_backend_request_auto_respects_calibration() {
    let policy = AutoPolicy::new();
    assert_eq!(
        policy.resolve_request(BackendRequest::Auto, None),
        BackendSelection::TractCpu
    );

    let passing_report = CalibrationReport {
        backend_name: "openvino".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.0,
        p95_latency_ms: 1.5,
        p99_latency_ms: 2.0,
        max_latency_ms: 3.0,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    assert_eq!(
        policy.resolve_request(BackendRequest::Auto, Some(&passing_report)),
        BackendSelection::OpenVino
    );
}

#[test]
fn user_can_select_tensorrt_openvino_or_directml_explicitly() {
    let policy = AutoPolicy::new();

    let passing_tensorrt_report = CalibrationReport {
        backend_name: "tensorrt".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.35,
        p95_latency_ms: 0.45,
        p99_latency_ms: 0.50,
        max_latency_ms: 0.80,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    // 1. AUTO prioritizes TensorRT when NVIDIA TensorRT passes calibration
    let selection = policy.resolve_request(BackendRequest::Auto, Some(&passing_tensorrt_report));
    assert_eq!(selection, BackendSelection::TensorRt);
    assert_eq!(selection.name(), "tensorrt");
    assert!(selection.is_nvidia());

    // 2. User explicitly selects TensorRT
    let explicit_trt =
        policy.resolve_request(BackendRequest::TensorRt, Some(&passing_tensorrt_report));
    assert_eq!(explicit_trt, BackendSelection::TensorRt);

    // 3. User explicitly selects OpenVINO (supported on both Linux and Windows)
    let passing_openvino_report = CalibrationReport {
        backend_name: "openvino".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.2,
        p95_latency_ms: 1.8,
        p99_latency_ms: 2.2,
        max_latency_ms: 3.1,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };
    let explicit_ov =
        policy.resolve_request(BackendRequest::OpenVino, Some(&passing_openvino_report));
    assert_eq!(explicit_ov, BackendSelection::OpenVino);
    assert_eq!(explicit_ov.name(), "openvino");

    // 4. User explicitly selects DirectML (Windows)
    let passing_directml_report = CalibrationReport {
        backend_name: "directml".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.5,
        p95_latency_ms: 2.0,
        p99_latency_ms: 2.5,
        max_latency_ms: 3.5,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };
    let explicit_dml =
        policy.resolve_request(BackendRequest::DirectMl, Some(&passing_directml_report));
    assert_eq!(explicit_dml, BackendSelection::DirectMl);
    assert_eq!(explicit_dml.name(), "directml");
}

#[test]
fn auto_four_tier_hierarchy_priority() {
    let trt_report = CalibrationReport {
        backend_name: "tensorrt".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.35,
        p95_latency_ms: 0.45,
        p99_latency_ms: 0.50,
        max_latency_ms: 0.80,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let vulkan_report = CalibrationReport {
        backend_name: "vulkan".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.80,
        p95_latency_ms: 1.20,
        p99_latency_ms: 1.50,
        max_latency_ms: 2.00,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let ryzenai_npu_report = CalibrationReport {
        backend_name: "ryzen-ai".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.90,
        p95_latency_ms: 1.30,
        p99_latency_ms: 1.60,
        max_latency_ms: 2.20,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let openvino_npu_report = CalibrationReport {
        backend_name: "openvino-npu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.00,
        p95_latency_ms: 1.40,
        p99_latency_ms: 1.70,
        max_latency_ms: 2.30,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let openvino_gpu_report = CalibrationReport {
        backend_name: "openvino-gpu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.50,
        p95_latency_ms: 2.10,
        p99_latency_ms: 2.80,
        max_latency_ms: 3.50,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let tract_cpu_report = CalibrationReport {
        backend_name: "tract".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 4.80,
        p95_latency_ms: 5.60,
        p99_latency_ms: 6.20,
        max_latency_ms: 6.90,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    // 1. All candidates present: Tier 1 Specific (TensorRT) wins
    let all_candidates = [
        trt_report.clone(),
        vulkan_report.clone(),
        ryzenai_npu_report.clone(),
        openvino_npu_report.clone(),
        openvino_gpu_report.clone(),
        tract_cpu_report.clone(),
    ];
    let sel = select_best(&all_candidates);
    assert_eq!(sel, BackendSelection::TensorRt);
    assert_eq!(sel.tier(), DeviceTier::DedicatedGpu);

    // 2. If TensorRT is absent/fails, Tier 1 General (Vulkan) wins over NPU, iGPU, and CPU
    let no_trt = [
        vulkan_report.clone(),
        ryzenai_npu_report.clone(),
        openvino_gpu_report.clone(),
        tract_cpu_report.clone(),
    ];
    let sel = select_best(&no_trt);
    assert_eq!(sel, BackendSelection::Vulkan);
    assert_eq!(sel.tier(), DeviceTier::DedicatedGpu);

    // 3. If all Dedicated GPUs fail/absent, Tier 2 (NPU: Ryzen AI or OpenVINO NPU) wins over iGPU and CPU
    let npu_and_igpu = [
        ryzenai_npu_report.clone(),
        openvino_gpu_report.clone(),
        tract_cpu_report.clone(),
    ];
    let sel = select_best(&npu_and_igpu);
    assert_eq!(sel, BackendSelection::RyzenAiNpu);
    assert_eq!(sel.tier(), DeviceTier::Npu);
    assert!(sel.is_npu());
    assert!(sel.is_amd());

    // 4. If NPU is also absent/fails, Tier 3 (Integrated GPU: OpenVINO GPU) wins over CPU
    let igpu_and_cpu = [openvino_gpu_report.clone(), tract_cpu_report.clone()];
    let sel = select_best(&igpu_and_cpu);
    assert_eq!(sel, BackendSelection::OpenVinoGpu);
    assert_eq!(sel.tier(), DeviceTier::IntegratedGpu);
    assert!(sel.is_integrated_gpu());

    // 5. If iGPU also fails/absent, Tier 4 (CPU: Tract) is selected safely
    let cpu_only = [tract_cpu_report];
    let sel = select_best(&cpu_only);
    assert_eq!(sel, BackendSelection::TractCpu);
    assert_eq!(sel.tier(), DeviceTier::Cpu);
    assert!(sel.is_tract_cpu());
}

#[test]
fn user_can_select_by_device_category() {
    let policy = AutoPolicy::new();

    let trt_report = CalibrationReport {
        backend_name: "tensorrt".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.35,
        p95_latency_ms: 0.45,
        p99_latency_ms: 0.50,
        max_latency_ms: 0.80,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let openvino_npu = CalibrationReport {
        backend_name: "openvino-npu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.00,
        p95_latency_ms: 1.40,
        p99_latency_ms: 1.70,
        max_latency_ms: 2.30,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let openvino_gpu = CalibrationReport {
        backend_name: "openvino-gpu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.50,
        p95_latency_ms: 2.10,
        p99_latency_ms: 2.80,
        max_latency_ms: 3.50,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let candidates = [trt_report, openvino_npu, openvino_gpu];

    // User forces Dedicated GPU
    assert_eq!(
        policy.resolve_candidates(BackendRequest::DedicatedGpu, &candidates),
        BackendSelection::TensorRt
    );

    // User forces NPU (skipping Dedicated GPU)
    let npu_sel = policy.resolve_candidates(BackendRequest::Npu, &candidates);
    assert_eq!(npu_sel, BackendSelection::OpenVinoNpu);
    assert!(npu_sel.is_npu());

    // User forces Integrated GPU (skipping dGPU and NPU)
    let igpu_sel = policy.resolve_candidates(BackendRequest::IntegratedGpu, &candidates);
    assert_eq!(igpu_sel, BackendSelection::OpenVinoGpu);
    assert!(igpu_sel.is_integrated_gpu());

    // User forces CPU
    let cpu_sel = policy.resolve_candidates(BackendRequest::Cpu, &candidates);
    assert_eq!(cpu_sel, BackendSelection::TractCpu);
    assert!(cpu_sel.is_tract_cpu());
}

#[test]
fn user_can_select_amd_ryzen_ai_on_linux_and_windows() {
    let policy = AutoPolicy::new();

    let passing_ryzenai_report = CalibrationReport {
        backend_name: "ryzen-ai".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.95,
        p95_latency_ms: 1.40,
        p99_latency_ms: 1.75,
        max_latency_ms: 2.50,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    // User explicitly selects Ryzen AI
    let explicit_ryzen =
        policy.resolve_request(BackendRequest::RyzenAi, Some(&passing_ryzenai_report));
    assert_eq!(explicit_ryzen, BackendSelection::RyzenAiNpu);
    assert_eq!(explicit_ryzen.name(), "ryzenai-npu");
    assert!(explicit_ryzen.is_amd());
    assert!(explicit_ryzen.is_npu());
    assert_eq!(explicit_ryzen.tier(), DeviceTier::Npu);

    // User explicitly selects Vulkan for AMD iGPU or dGPU
    let passing_vulkan_report = CalibrationReport {
        backend_name: "vulkan".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.10,
        p95_latency_ms: 1.60,
        p99_latency_ms: 2.00,
        max_latency_ms: 2.80,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };
    let explicit_vulkan =
        policy.resolve_request(BackendRequest::Vulkan, Some(&passing_vulkan_report));
    assert_eq!(explicit_vulkan, BackendSelection::Vulkan);
    assert_eq!(explicit_vulkan.name(), "vulkan");
    assert!(explicit_vulkan.is_dedicated_gpu());

    // If Ryzen AI report fails quality gate, falls back to TractCpu safely
    let failed_ryzenai_report = CalibrationReport {
        backend_name: "ryzen-ai".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 2.0,
        p95_latency_ms: 5.0,
        p99_latency_ms: 12.0, // > 10.0 ms
        max_latency_ms: 15.0,
        deadline_miss_count: 2,
        discontinuities: 0,
        decision: PromotionDecision::NotPromoted,
        reason: Some("Exceeded latency budget".to_owned()),
    };
    let fallback = policy.resolve_request(BackendRequest::RyzenAi, Some(&failed_ryzenai_report));
    assert_eq!(fallback, BackendSelection::TractCpu);
    assert!(fallback.is_tract_cpu());
}

#[test]
fn ryzen_ai_supports_both_npu_and_igpu() {
    let policy = AutoPolicy::new();

    let passing_npu = CalibrationReport {
        backend_name: "ryzenai-npu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.90,
        p95_latency_ms: 1.30,
        p99_latency_ms: 1.60,
        max_latency_ms: 2.20,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let passing_igpu = CalibrationReport {
        backend_name: "ryzenai-gpu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.40,
        p95_latency_ms: 2.00,
        p99_latency_ms: 2.60,
        max_latency_ms: 3.40,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    // 1. AMD NPU reports as Tier 2 (NPU)
    let sel_npu = policy.resolve_request(BackendRequest::RyzenAiNpu, Some(&passing_npu));
    assert_eq!(sel_npu, BackendSelection::RyzenAiNpu);
    assert_eq!(sel_npu.name(), "ryzenai-npu");
    assert_eq!(sel_npu.tier(), DeviceTier::Npu);
    assert!(sel_npu.is_npu());
    assert!(sel_npu.is_amd());

    // 2. AMD iGPU reports as Tier 3 (Integrated GPU)
    let sel_igpu = policy.resolve_request(BackendRequest::RyzenAiGpu, Some(&passing_igpu));
    assert_eq!(sel_igpu, BackendSelection::RyzenAiGpu);
    assert_eq!(sel_igpu.name(), "ryzenai-gpu");
    assert_eq!(sel_igpu.tier(), DeviceTier::IntegratedGpu);
    assert!(sel_igpu.is_integrated_gpu());
    assert!(sel_igpu.is_amd());

    // 3. User requests RyzenAi category: prefers NPU over iGPU
    let candidates = [passing_igpu, passing_npu];
    let sel_auto_ryzen = policy.resolve_candidates(BackendRequest::RyzenAi, &candidates);
    assert_eq!(sel_auto_ryzen, BackendSelection::RyzenAiNpu);
}

#[test]
fn intel_cpu_prefers_openvino_over_onnx_tract() {
    let policy = AutoPolicy::new();

    let openvino_cpu_report = CalibrationReport {
        backend_name: "openvino-cpu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 2.20,
        p95_latency_ms: 3.10,
        p99_latency_ms: 4.00,
        max_latency_ms: 5.20,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let tract_cpu_report = CalibrationReport {
        backend_name: "tract".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 4.80,
        p95_latency_ms: 5.60,
        p99_latency_ms: 6.20,
        max_latency_ms: 6.90,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let candidates = [tract_cpu_report, openvino_cpu_report];

    // On Intel CPU, BackendRequest::Cpu prefers OpenVINO CPU (score 60) over ONNX Tract (score 50)
    let cpu_sel = policy.resolve_candidates(BackendRequest::Cpu, &candidates);
    assert_eq!(cpu_sel, BackendSelection::OpenVinoCpu);
    assert_eq!(cpu_sel.name(), "openvino-cpu");
    assert_eq!(cpu_sel.tier(), DeviceTier::Cpu);
    assert!(cpu_sel.is_intel());

    // AUTO policy selecting among CPU candidates also prefers OpenVINO CPU
    let auto_sel = policy.resolve_candidates(BackendRequest::Auto, &candidates);
    assert_eq!(auto_sel, BackendSelection::OpenVinoCpu);

    // Explicit TractCpu request always returns TractCpu
    let tract_sel = policy.resolve_candidates(BackendRequest::TractCpu, &candidates);
    assert_eq!(tract_sel, BackendSelection::TractCpu);
    assert!(tract_sel.is_tract_cpu());
}

#[test]
fn directml_is_below_proprietary_gpu_runtimes_but_above_npu() {
    let policy = AutoPolicy::new();

    let trt_report = CalibrationReport {
        backend_name: "tensorrt".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.50,
        p95_latency_ms: 0.85,
        p99_latency_ms: 1.10,
        max_latency_ms: 1.70,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let dml_dgpu = CalibrationReport {
        backend_name: "directml".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.65,
        p95_latency_ms: 0.95,
        p99_latency_ms: 1.25,
        max_latency_ms: 1.90,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let amd_npu = CalibrationReport {
        backend_name: "ryzenai-npu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.90,
        p95_latency_ms: 1.30,
        p99_latency_ms: 1.60,
        max_latency_ms: 2.20,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let intel_igpu = CalibrationReport {
        backend_name: "openvino-gpu".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 1.50,
        p95_latency_ms: 2.10,
        p99_latency_ms: 2.80,
        max_latency_ms: 3.50,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    let tract_cpu = CalibrationReport {
        backend_name: "tract".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 4.80,
        p95_latency_ms: 5.60,
        p99_latency_ms: 6.20,
        max_latency_ms: 6.90,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    };

    // 1. Proprietary GPU runtime (TensorRT score 100) is preferred over DirectML (score 90)
    let candidates_with_trt = [trt_report, dml_dgpu.clone(), amd_npu.clone()];
    let sel_trt = policy.resolve_candidates(BackendRequest::Auto, &candidates_with_trt);
    assert_eq!(sel_trt, BackendSelection::TensorRt);

    // 2. When proprietary GPU runtimes are absent, DirectML (score 90) takes top priority over NPU (score 80), iGPU (score 70), and CPU (score 50)
    let candidates_without_cuda = [dml_dgpu, amd_npu, intel_igpu, tract_cpu];
    let sel_dml = policy.resolve_candidates(BackendRequest::Auto, &candidates_without_cuda);
    assert_eq!(sel_dml, BackendSelection::DirectMl);
    assert_eq!(sel_dml.name(), "directml");
    assert_eq!(sel_dml.tier(), DeviceTier::DedicatedGpu);
    assert!(sel_dml.is_dedicated_gpu());

    // 3. User forces Dedicated GPU: selects DirectML when it's the available dGPU
    let dgpu_sel =
        policy.resolve_candidates(BackendRequest::DedicatedGpu, &candidates_without_cuda);
    assert_eq!(dgpu_sel, BackendSelection::DirectMl);
}
