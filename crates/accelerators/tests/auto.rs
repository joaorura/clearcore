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
fn auto_selects_inference_accelerator_when_quality_gate_passes() {
    let passing_trt_report = CalibrationReport {
        backend_name: "openvino-npu".to_owned(),
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
    assert_eq!(selection, BackendSelection::OpenVinoNpu);
    assert_eq!(selection.name(), "openvino-npu");
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

    // 1. TensorRT does not run inference yet (CUDA round trip only), so a promoted report for it
    //    is never chosen, neither by AUTO nor by an explicit request: both fall back to tract.
    let selection = policy.resolve_request(BackendRequest::Auto, Some(&passing_tensorrt_report));
    assert_eq!(selection, BackendSelection::TractCpu);

    // 2. User explicitly selects TensorRT: same honest fallback
    let explicit_trt =
        policy.resolve_request(BackendRequest::TensorRt, Some(&passing_tensorrt_report));
    assert_eq!(explicit_trt, BackendSelection::TractCpu);

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

    // 4. User explicitly selects DirectML (Windows): passthrough today, so it falls back to tract
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
    assert_eq!(explicit_dml, BackendSelection::TractCpu);
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

    // 1. All candidates present: the passthrough runtimes (TensorRT, Vulkan, Ryzen AI) are
    //    skipped even though promoted, so the best *inference* backend (OpenVINO NPU) wins.
    let all_candidates = [
        trt_report.clone(),
        vulkan_report.clone(),
        ryzenai_npu_report.clone(),
        openvino_npu_report.clone(),
        openvino_gpu_report.clone(),
        tract_cpu_report.clone(),
    ];
    let sel = select_best(&all_candidates);
    assert_eq!(sel, BackendSelection::OpenVinoNpu);
    assert_eq!(sel.tier(), DeviceTier::Npu);
    assert!(sel.is_npu());

    // 2. Without the NPU, Tier 3 (Integrated GPU: OpenVINO GPU) wins over CPU
    let igpu_and_cpu = [
        vulkan_report.clone(),
        openvino_gpu_report.clone(),
        tract_cpu_report.clone(),
    ];
    let sel = select_best(&igpu_and_cpu);
    assert_eq!(sel, BackendSelection::OpenVinoGpu);
    assert_eq!(sel.tier(), DeviceTier::IntegratedGpu);
    assert!(sel.is_integrated_gpu());

    // 3. Only passthrough candidates: nothing may be chosen, AUTO stays on tract
    let passthrough_only = [trt_report, vulkan_report, ryzenai_npu_report];
    assert_eq!(select_best(&passthrough_only), BackendSelection::TractCpu);

    // 4. CPU only: Tier 4 (Tract) is selected safely
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

    // User forces Dedicated GPU: the only candidate is TensorRT, which does not run inference
    // yet, so the request falls back to tract.
    assert_eq!(
        policy.resolve_candidates(BackendRequest::DedicatedGpu, &candidates),
        BackendSelection::TractCpu
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
fn ryzen_ai_and_vulkan_requests_fall_back_while_passthrough() {
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

    // The Ryzen AI path does not run inference yet: a promoted report is not enough to select it
    let explicit_ryzen =
        policy.resolve_request(BackendRequest::RyzenAi, Some(&passing_ryzenai_report));
    assert_eq!(explicit_ryzen, BackendSelection::TractCpu);
    assert_eq!(BackendSelection::RyzenAiNpu.tier(), DeviceTier::Npu);
    assert!(BackendSelection::RyzenAiNpu.is_amd());

    // Vulkan is a passthrough as well, so an explicit request falls back to tract
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
    assert_eq!(explicit_vulkan, BackendSelection::TractCpu);
    assert!(BackendSelection::Vulkan.is_dedicated_gpu());

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
fn ryzen_ai_npu_and_igpu_are_not_selectable_while_passthrough() {
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

    // Ryzen AI NPU / iGPU keep their tier metadata, but neither runs inference yet, so promoted
    // reports for them are never selected, by name or by category.
    assert_eq!(BackendSelection::RyzenAiNpu.name(), "ryzenai-npu");
    assert_eq!(BackendSelection::RyzenAiNpu.tier(), DeviceTier::Npu);
    assert_eq!(BackendSelection::RyzenAiGpu.name(), "ryzenai-gpu");
    assert_eq!(
        BackendSelection::RyzenAiGpu.tier(),
        DeviceTier::IntegratedGpu
    );

    assert_eq!(
        policy.resolve_request(BackendRequest::RyzenAiNpu, Some(&passing_npu)),
        BackendSelection::TractCpu
    );
    assert_eq!(
        policy.resolve_request(BackendRequest::RyzenAiGpu, Some(&passing_igpu)),
        BackendSelection::TractCpu
    );
    let candidates = [passing_igpu, passing_npu];
    assert_eq!(
        policy.resolve_candidates(BackendRequest::RyzenAi, &candidates),
        BackendSelection::TractCpu
    );
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
fn passthrough_gpu_runtimes_are_skipped_even_when_promoted() {
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

    // 1. TensorRT, DirectML and the Ryzen AI NPU rank highest by score, but none of them runs
    //    inference yet, so AUTO falls back to tract.
    let passthrough_only = [trt_report, dml_dgpu.clone(), amd_npu.clone()];
    let sel = policy.resolve_candidates(BackendRequest::Auto, &passthrough_only);
    assert_eq!(sel, BackendSelection::TractCpu);

    // 2. With an inference-capable iGPU in the mix, it is chosen over the higher-scored passthroughs
    let mixed = [dml_dgpu, amd_npu, intel_igpu, tract_cpu];
    let sel = policy.resolve_candidates(BackendRequest::Auto, &mixed);
    assert_eq!(sel, BackendSelection::OpenVinoGpu);
    assert_eq!(sel.tier(), DeviceTier::IntegratedGpu);

    // 3. User forces Dedicated GPU: DirectML is the only dGPU candidate and is a passthrough
    let dgpu_sel = policy.resolve_candidates(BackendRequest::DedicatedGpu, &mixed);
    assert_eq!(dgpu_sel, BackendSelection::TractCpu);
}

fn promoted_report(name: &str) -> CalibrationReport {
    CalibrationReport {
        backend_name: name.to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.5,
        p95_latency_ms: 0.8,
        p99_latency_ms: 1.0,
        max_latency_ms: 1.5,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: PromotionDecision::Promoted,
        reason: None,
    }
}

#[test]
fn deserialized_promoted_passthrough_report_is_never_selected() {
    // A report loaded from disk can name any backend as promoted, e.g. "vulkan", which only copies
    // samples today. select_auto, select_best and resolve_candidates must all refuse it.
    let policy = AutoPolicy::new();
    for name in [
        "vulkan",
        "directml",
        "tensorrt",
        "cuda",
        "ryzenai-npu",
        "ryzenai-gpu",
        "coreml",
    ] {
        let json = serde_json::to_string(&promoted_report(name)).unwrap();
        let report: CalibrationReport = serde_json::from_str(&json).unwrap();
        assert!(
            report.is_promoted(),
            "{name}: the report itself is promoted"
        );

        assert_eq!(
            select_auto(report.clone()),
            BackendSelection::TractCpu,
            "{name}"
        );
        assert_eq!(
            select_best(std::slice::from_ref(&report)),
            BackendSelection::TractCpu,
            "{name}"
        );
        for request in [
            BackendRequest::Auto,
            BackendRequest::DedicatedGpu,
            BackendRequest::IntegratedGpu,
            BackendRequest::Npu,
            BackendRequest::RyzenAi,
            BackendRequest::Vulkan,
            BackendRequest::DirectMl,
            BackendRequest::TensorRt,
            BackendRequest::CoreMl,
        ] {
            assert_eq!(
                policy.resolve_candidates(request, std::slice::from_ref(&report)),
                BackendSelection::TractCpu,
                "{name} via {request:?}"
            );
        }
    }
}
