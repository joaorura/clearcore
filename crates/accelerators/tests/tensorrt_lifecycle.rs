#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use realtime_noise_accelerators::{
    BackendSelection, DetectedHardware, HardwareScanner, OpenVINOBackend, TensorRtBackend,
};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::{ALGORITHM_LATENCY_SAMPLES, InferenceBackend, InferenceError};
use realtime_noise_runtime_tensorrt::{CudaDevice, PrecisionTarget};

fn models_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/stateful")
}

#[test]
fn mock_backend_describes_blackwell_fp8_and_never_claims_hardware() {
    let mut backend = TensorRtBackend::new_mock();
    let descriptor = backend.descriptor();
    assert_eq!(descriptor.backend, "tensorrt");
    assert_eq!(descriptor.runtime, "tensorrt");
    assert_eq!(descriptor.cpu_profile, "sm_120");
    assert_eq!(backend.precision(), PrecisionTarget::Fp8);
    assert_eq!(backend.sm_version(), "sm_120");
    assert!(!backend.is_hardware_backed());
    assert!(backend.native_runtime_version().is_none());
    assert!(backend.fallback_reason().is_none());
    // Known gap: no TensorRT engine is executed by any path yet.
    assert!(!backend.executes_engine());
    assert_eq!(
        backend.algorithmic_latency_samples(),
        ALGORITHM_LATENCY_SAMPLES
    );

    let input: AudioFrame = [0.25; HOP_SAMPLES];
    let frame = backend.process(&input).unwrap();
    assert_eq!(frame.provenance.backend, "tensorrt");
    assert!(frame.samples.iter().all(|s| (s - 0.25).abs() < 0.05));
}

#[test]
fn precision_follows_the_policy_for_every_gpu_generation() {
    let cases = [
        ((12, 0), "sm_120", Some(PrecisionTarget::Fp8)),
        ((9, 0), "sm_90", Some(PrecisionTarget::Fp8)),
        ((8, 9), "sm_89", Some(PrecisionTarget::Fp8)),
        ((8, 6), "sm_86", Some(PrecisionTarget::Fp16)),
        ((7, 5), "sm_75", Some(PrecisionTarget::Fp16)),
        ((6, 1), "sm_61", Some(PrecisionTarget::Int8)),
        ((5, 2), "nvidia-gpu", None),
    ];
    for (cc, profile, expected) in cases {
        let result = TensorRtBackend::new_mock_for_device(CudaDevice::mock("gpu", cc));
        match (expected, result) {
            (Some(precision), Ok(backend)) => {
                assert_eq!(backend.precision(), precision, "{cc:?}");
                assert_eq!(backend.descriptor().cpu_profile, profile, "{cc:?}");
                assert_ne!(
                    backend.precision(),
                    PrecisionTarget::Fp32,
                    "FP32 never on GPU"
                );
            }
            (None, Err(InferenceError::InferenceExecution(message))) => {
                assert!(message.contains("FP8/FP16/INT8"), "{message}");
            }
            (expected, other) => {
                unreachable!("{cc:?}: expected {expected:?}, got {other:?}");
            }
        }
    }
}

#[test]
fn mock_backend_enforces_the_frame_contracts() {
    let mut backend = TensorRtBackend::new_mock();

    let mut nan: AudioFrame = [0.0; HOP_SAMPLES];
    nan[3] = f32::NAN;
    assert!(matches!(
        backend.process(&nan),
        Err(InferenceError::InputContract(_))
    ));

    let mut inf: AudioFrame = [0.0; HOP_SAMPLES];
    inf[99] = f32::INFINITY;
    assert!(matches!(
        backend.process(&inf),
        Err(InferenceError::InputContract(_))
    ));

    let silence: AudioFrame = [0.0; HOP_SAMPLES];
    backend.set_simulated_failure(true);
    assert!(matches!(
        backend.process(&silence),
        Err(InferenceError::InferenceExecution(_))
    ));
    backend.set_simulated_failure(false);
    assert!(backend.process(&silence).is_ok());
}

#[test]
fn hardware_path_runs_on_the_real_gpu_or_degrades_gracefully() {
    let available = TensorRtBackend::is_available();
    let strict = TensorRtBackend::try_new_hardware("dfn3", "unverified", 0);
    let lenient = TensorRtBackend::new("dfn3", "unverified", 0);

    match strict {
        Ok(mut backend) => {
            assert!(
                available,
                "hardware context created while probe said unavailable"
            );
            assert!(backend.is_hardware_backed());
            assert!(lenient.is_hardware_backed());
            assert!(lenient.fallback_reason().is_none());
            assert!(backend.native_runtime_version().is_some());
            assert_ne!(backend.precision(), PrecisionTarget::Fp32);
            assert!(!backend.executes_engine());
            eprintln!(
                "TensorRT on {} ({}), precision {}, libnvinfer {}",
                backend.device_name(),
                backend.sm_version(),
                backend.precision().as_str(),
                backend.native_runtime_version().unwrap_or("?")
            );

            // The CUDA round trip returns what it was given (identity, not inference).
            let input: AudioFrame = std::array::from_fn(|i| (i as f32 / 480.0) - 0.5);
            for _ in 0..3 {
                let frame = backend.process(&input).unwrap();
                assert_eq!(frame.samples, input);
            }
        }
        Err(error) => {
            eprintln!("TensorRT hardware unavailable (graceful): {error}");
            assert!(!lenient.is_hardware_backed());
            assert!(lenient.fallback_reason().is_some());
            // The degraded backend is still safe to drive.
            let mut lenient = lenient;
            let input: AudioFrame = [0.1; HOP_SAMPLES];
            assert!(lenient.process(&input).is_ok());
        }
    }
}

#[test]
fn hardware_instantiation_is_strict_about_what_it_wires() {
    // TensorRT has no engine execution yet (CUDA copy only), so it is refused even on a host
    // where the strict constructor succeeds: `Ok` would hand back a backend that never denoises.
    assert!(!BackendSelection::TensorRt.executes_inference());
    let via_selection =
        BackendSelection::TensorRt.instantiate_hardware_backend(&models_dir(), "a", "b");
    let error = via_selection.err().expect("TensorRT must not instantiate");
    assert!(
        error.to_string().contains("does not execute inference"),
        "{error}"
    );

    // OpenVINO: a missing model directory is an error, never a silent mock.
    let missing = BackendSelection::OpenVinoCpu.instantiate_hardware_backend(
        std::path::Path::new("/nonexistent/stateful"),
        "a",
        "b",
    );
    assert!(missing.is_err());

    if OpenVINOBackend::is_available() && models_dir().join("enc.onnx").exists() {
        let mut backend = BackendSelection::OpenVinoCpu
            .instantiate_hardware_backend(&models_dir(), "a", "b")
            .unwrap();
        let input: AudioFrame = [0.01; HOP_SAMPLES];
        assert!(backend.process(&input).is_ok());
        assert!(
            BackendSelection::OpenVino
                .instantiate_hardware_backend(&models_dir(), "a", "b")
                .is_ok()
        );
    }

    // Runtimes with their own constructors are not routed through this entry point.
    assert!(
        BackendSelection::DirectMl
            .instantiate_hardware_backend(&models_dir(), "a", "b")
            .is_err()
    );
}

#[test]
fn scanner_agrees_with_the_runtime_discovery() {
    let audits = HardwareScanner::scan();
    for audit in &audits {
        match audit.hardware {
            DetectedHardware::NvidiaGpu
                if realtime_noise_runtime_tensorrt::locate_tensorrt_library().is_some() =>
            {
                assert!(audit.is_runtime_installed(), "{audit:?}");
            }
            DetectedHardware::IntelNpu | DetectedHardware::IntelGpu
                if realtime_noise_runtime_openvino::locate_library().is_some() =>
            {
                assert!(audit.is_runtime_installed(), "{audit:?}");
            }
            _ => {}
        }
    }
}

#[test]
fn descriptor_does_not_assert_a_runtime_version_it_cannot_know() {
    let backend = TensorRtBackend::new_mock();
    let descriptor = backend.descriptor();
    // The static descriptor must not claim a specific libnvinfer release; the real one is
    // reported by `native_runtime_version()` (None for mocks).
    assert_eq!(descriptor.runtime_version, "unknown");
    assert!(!descriptor.backend_version.starts_with("10."));
    assert_eq!(backend.native_runtime_version(), None);
    assert!(!backend.executes_inference());
}
