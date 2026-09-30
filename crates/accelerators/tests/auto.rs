#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use realtime_noise_accelerators::{
    AutoPolicy, BackendRequest, BackendSelection, CalibrationReport, PromotionDecision,
    select_auto,
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
    assert_eq!(select_auto(discontinuity_report), BackendSelection::TractCpu);

    // 4. Test AutoPolicy struct selection behavior
    let policy = AutoPolicy::new();
    let selection = policy.select(&failed_report);
    assert_eq!(selection, BackendSelection::TractCpu);
}

#[test]
fn auto_selects_accelerator_when_quality_gate_passes() {
    let passing_cuda_report = CalibrationReport {
        backend_name: "cuda".to_owned(),
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

    let selection = select_auto(passing_cuda_report);
    assert_eq!(selection, BackendSelection::Cuda);
    assert_eq!(selection.name(), "cuda");
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
    let explicit_trt = policy.resolve_request(BackendRequest::TensorRt, Some(&passing_tensorrt_report));
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
    let explicit_ov = policy.resolve_request(BackendRequest::OpenVino, Some(&passing_openvino_report));
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
    let explicit_dml = policy.resolve_request(BackendRequest::DirectMl, Some(&passing_directml_report));
    assert_eq!(explicit_dml, BackendSelection::DirectMl);
    assert_eq!(explicit_dml.name(), "directml");
}
