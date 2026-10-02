#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::cognitive_complexity
)]

use realtime_noise_accelerators::{
    CoreMlBackend, DirectMlBackend, OpenVINOBackend, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, RyzenAiBackend, TensorRtBackend,
    VulkanBackend, evaluate_calibration,
};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ};
use realtime_noise_model::{InferenceBackend, InferenceError};

#[test]
fn qualification_enforces_p99_latency_threshold() {
    assert_eq!(QUALIFICATION_MAX_P99_MS, 10.0);
    assert_eq!(QUALIFICATION_MAX_DEADLINE_MS, 10.0);

    // Latency within threshold passes (if all else passes)
    let at_threshold = evaluate_calibration(
        "cuda", 300.0, 30_000, 2.0, 5.0, 10.0, // p99 == 10.0 ms
        10.0, // max == 10.0 ms
        0,    // deadline misses
        0,    // discontinuities
    );
    assert_eq!(at_threshold.decision, PromotionDecision::Promoted);

    // Latency exceeding 10.0 ms fails
    let over_threshold = evaluate_calibration(
        "cuda", 300.0, 30_000, 2.0, 5.0, 10.001, // p99 > 10.0 ms
        10.001, 0, 0,
    );
    assert_eq!(over_threshold.decision, PromotionDecision::NotPromoted);
    assert!(
        over_threshold
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("p99 latency")
    );
}

#[test]
fn qualification_enforces_zero_discontinuities() {
    let with_discontinuity = evaluate_calibration(
        "openvino", 300.0, 30_000, 1.0, 2.0, 3.0, 4.0, 0, 1, // 1 discontinuity
    );
    assert_eq!(with_discontinuity.decision, PromotionDecision::NotPromoted);
    assert!(
        with_discontinuity
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("discontinuities")
    );
}

#[test]
fn qualification_enforces_zero_deadline_misses() {
    let with_deadline_miss = evaluate_calibration(
        "coreml", 300.0, 30_000, 1.0, 2.0, 3.0, 11.5, 1, // 1 deadline miss
        0,
    );
    assert_eq!(with_deadline_miss.decision, PromotionDecision::NotPromoted);
    assert!(
        with_deadline_miss
            .reason
            .as_deref()
            .unwrap_or("")
            .contains("deadline")
    );
}

#[test]
fn qualification_enforces_contracts_adherence() {
    // Contract assertions:
    // 1. AudioFrame size is exactly 480 samples (10 ms at 48 kHz)
    assert_eq!(HOP_SAMPLES, 480);
    assert_eq!(SAMPLE_RATE_HZ, 48_000);

    // 2. All backends must implement InferenceBackend and adhere to contracts
    let mut tensorrt = TensorRtBackend::new_mock();
    let mut openvino = OpenVINOBackend::new_mock();
    let mut coreml = CoreMlBackend::new_mock();
    let mut directml = DirectMlBackend::new_mock();
    let mut vulkan = VulkanBackend::new_mock();
    let mut ryzenai = RyzenAiBackend::new_mock();

    // Check descriptors
    assert_eq!(tensorrt.descriptor().backend, "tensorrt");
    assert_eq!(openvino.descriptor().backend, "openvino");
    assert_eq!(coreml.descriptor().backend, "coreml");
    assert_eq!(directml.descriptor().backend, "directml");
    assert_eq!(vulkan.descriptor().backend, "vulkan");
    assert_eq!(ryzenai.descriptor().backend, "ryzenai");

    // Check algorithmic latency contract (1440 samples = 30 ms)
    assert_eq!(tensorrt.algorithmic_latency_samples(), 1_440);
    assert_eq!(openvino.algorithmic_latency_samples(), 1_440);
    assert_eq!(coreml.algorithmic_latency_samples(), 1_440);
    assert_eq!(directml.algorithmic_latency_samples(), 1_440);
    assert_eq!(vulkan.algorithmic_latency_samples(), 1_440);
    assert_eq!(ryzenai.algorithmic_latency_samples(), 1_440);

    // Check finite input processing contract
    let input: AudioFrame = [0.0; HOP_SAMPLES];
    let tensorrt_out = tensorrt.process(&input).unwrap();
    assert_eq!(tensorrt_out.samples.len(), HOP_SAMPLES);
    assert!(tensorrt_out.samples.iter().all(|s| s.is_finite()));

    let directml_out = directml.process(&input).unwrap();
    assert_eq!(directml_out.samples.len(), HOP_SAMPLES);
    assert!(directml_out.samples.iter().all(|s| s.is_finite()));

    let vulkan_out = vulkan.process(&input).unwrap();
    assert_eq!(vulkan_out.samples.len(), HOP_SAMPLES);
    assert!(vulkan_out.samples.iter().all(|s| s.is_finite()));

    let ryzenai_out = ryzenai.process(&input).unwrap();
    assert_eq!(ryzenai_out.samples.len(), HOP_SAMPLES);
    assert!(ryzenai_out.samples.iter().all(|s| s.is_finite()));

    // Check non-finite input rejection (contracts enforcement)
    let mut nan_input = [0.0; HOP_SAMPLES];
    nan_input[0] = f32::NAN;
    assert!(matches!(
        tensorrt.process(&nan_input),
        Err(InferenceError::InputContract(_))
    ));
    assert!(matches!(
        openvino.process(&nan_input),
        Err(InferenceError::InputContract(_))
    ));
    assert!(matches!(
        coreml.process(&nan_input),
        Err(InferenceError::InputContract(_))
    ));
    assert!(matches!(
        directml.process(&nan_input),
        Err(InferenceError::InputContract(_))
    ));
    assert!(matches!(
        vulkan.process(&nan_input),
        Err(InferenceError::InputContract(_))
    ));
    assert!(matches!(
        ryzenai.process(&nan_input),
        Err(InferenceError::InputContract(_))
    ));
}
