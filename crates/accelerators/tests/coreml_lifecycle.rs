#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names
)]

use std::path::Path;

use realtime_noise_accelerators::{
    BackendRequest, BackendSelection, CoreMlBackend, DeviceTier, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, evaluate_calibration,
};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ};
use realtime_noise_model::{ALGORITHM_LATENCY_SAMPLES, InferenceBackend, InferenceError};
use realtime_noise_runtime_coreml::ComputeUnit;

#[test]
fn coreml_backend_creation_and_compute_units() {
    let backend_all = CoreMlBackend::new_mock();
    assert_eq!(backend_all.descriptor().backend, "coreml");
    assert_eq!(backend_all.descriptor().runtime, "coreml-ane");
    assert_eq!(backend_all.descriptor().cpu_profile, "apple-silicon-ane");
    assert_eq!(backend_all.compute_unit(), "all");
    assert_eq!(backend_all.parsed_compute_unit(), ComputeUnit::All);
    assert_eq!(
        backend_all.algorithmic_latency_samples(),
        ALGORITHM_LATENCY_SAMPLES
    );

    let backend_ane = CoreMlBackend::new_mock_with_unit("ane");
    assert_eq!(backend_ane.compute_unit(), "ane");
    assert_eq!(
        backend_ane.parsed_compute_unit(),
        ComputeUnit::CpuAndNeuralEngine
    );

    let backend_gpu = CoreMlBackend::new_mock_with_unit("gpu");
    assert_eq!(backend_gpu.compute_unit(), "gpu");
    assert_eq!(backend_gpu.parsed_compute_unit(), ComputeUnit::CpuAndGpu);

    let backend_cpu = CoreMlBackend::new_mock_with_unit("cpu");
    assert_eq!(backend_cpu.compute_unit(), "cpu");
    assert_eq!(backend_cpu.parsed_compute_unit(), ComputeUnit::CpuOnly);

    // Verify runner access
    assert_eq!(backend_all.runner().compute_unit(), ComputeUnit::All);
    assert_eq!(backend_all.runner().input_tensor().len(), HOP_SAMPLES);
    assert_eq!(backend_all.runner().output_tensor().len(), HOP_SAMPLES);
}

#[test]
fn coreml_backend_process_and_contracts_adherence() {
    let mut backend = CoreMlBackend::new_mock();

    // 1. Audio contract verification (480 samples = 10ms at 48kHz mono)
    assert_eq!(HOP_SAMPLES, 480);
    assert_eq!(SAMPLE_RATE_HZ, 48_000);

    let input: AudioFrame = [0.035; HOP_SAMPLES];
    let processed = backend.process(&input).expect("process succeeds");
    assert_eq!(processed.samples.len(), HOP_SAMPLES);
    assert!(processed.samples.iter().all(|s| s.is_finite()));
    assert_eq!(
        processed.algorithmic_latency_samples,
        ALGORITHM_LATENCY_SAMPLES
    );
    assert_eq!(processed.provenance.backend, "coreml");

    // 2. Reject non-finite input (NaN)
    let mut nan_input = [0.0; HOP_SAMPLES];
    nan_input[12] = f32::NAN;
    let nan_res = backend.process(&nan_input);
    assert!(matches!(nan_res, Err(InferenceError::InputContract(_))));

    // 3. Reject non-finite input (Infinity)
    let mut inf_input = [0.0; HOP_SAMPLES];
    inf_input[50] = f32::INFINITY;
    let inf_res = backend.process(&inf_input);
    assert!(matches!(inf_res, Err(InferenceError::InputContract(_))));

    // 4. Simulated failure propagation
    backend.set_simulated_failure(true);
    let fail_res = backend.process(&input);
    assert!(matches!(
        fail_res,
        Err(InferenceError::InferenceExecution(_))
    ));
    backend.set_simulated_failure(false);
    assert!(backend.process(&input).is_ok());
}

#[test]
fn coreml_backend_load_compiled_validation() {
    let non_existent = Path::new("/var/invalid/bundle_that_does_not_exist.mlmodelc");
    let result = CoreMlBackend::load_compiled(non_existent, "ane");
    assert!(matches!(result, Err(InferenceError::ModelCorruption(_))));
}

#[test]
fn coreml_qualification_and_calibration_gate() {
    assert_eq!(QUALIFICATION_MAX_P99_MS, 10.0);
    assert_eq!(QUALIFICATION_MAX_DEADLINE_MS, 10.0);

    // CoreML ANE pass: p99 latency 2.5 ms, 0 deadline misses, 0 discontinuities
    let report_pass = evaluate_calibration("coreml", 300.0, 30_000, 1.2, 2.0, 2.5, 3.8, 0, 0);
    assert_eq!(report_pass.decision, PromotionDecision::Promoted);
    assert!(report_pass.reason.is_none());

    // CoreML fail: p99 exceeds 10.0 ms threshold
    let report_latency_fail =
        evaluate_calibration("coreml", 300.0, 30_000, 5.0, 8.0, 10.5, 12.0, 0, 0);
    assert_eq!(report_latency_fail.decision, PromotionDecision::NotPromoted);
    assert!(
        report_latency_fail
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("p99 latency")
    );

    // CoreML fail: deadline miss
    let report_deadline_fail =
        evaluate_calibration("coreml", 300.0, 30_000, 1.0, 2.0, 3.0, 10.1, 1, 0);
    assert_eq!(
        report_deadline_fail.decision,
        PromotionDecision::NotPromoted
    );

    // CoreML fail: discontinuity
    let report_disc_fail = evaluate_calibration("coreml", 300.0, 30_000, 1.0, 2.0, 3.0, 4.0, 0, 1);
    assert_eq!(report_disc_fail.decision, PromotionDecision::NotPromoted);
}

#[test]
fn coreml_tier2_npu_auto_hierarchy() {
    assert_eq!(BackendSelection::CoreMl.tier(), DeviceTier::Npu);
    assert!(BackendSelection::CoreMl.is_npu());
    assert!(!BackendSelection::CoreMl.is_dedicated_gpu());
    assert!(!BackendSelection::CoreMl.is_tract_cpu());
    assert_eq!(BackendSelection::CoreMl.name(), "coreml");

    // Request mapping
    assert_eq!(BackendRequest::CoreMl, BackendRequest::CoreMl);
}
