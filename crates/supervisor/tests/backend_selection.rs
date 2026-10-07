#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_engine::DenoiseMode;
use realtime_noise_supervisor::{EngineSupervisor, find_repo_root, find_stateful_model_dir};
use std::path::Path;

#[test]
fn supervisor_auto_selects_accelerator_when_stateful_model_present() {
    let stateful_dir = find_stateful_model_dir();
    let root = find_repo_root();

    let mut supervisor = EngineSupervisor::default();
    let info = supervisor.select_backend("auto", stateful_dir.as_deref(), root.as_deref());

    // Priority in Auto: NPU -> iGPU -> dGPU (TensorRT) -> CPU (Tract)
    if info.is_hardware_accelerated {
        assert!(supervisor.is_hardware_accelerated());
        assert!(matches!(
            info.name.as_str(),
            "openvino-npu" | "openvino-gpu" | "nvidia-tensorrt"
        ));
        assert_eq!(supervisor.active_backend_name(), info.name);
    } else {
        assert!(info.name.starts_with("tract"));
    }
}

#[test]
fn supervisor_selects_tensorrt_backend_explicitly() {
    let root = find_repo_root();
    let mut supervisor = EngineSupervisor::default();

    let info = supervisor.select_backend("tensorrt", None, root.as_deref());
    if realtime_noise_accelerators::TensorRtBackend::is_available() {
        assert!(info.is_hardware_accelerated);
        assert_eq!(info.name, "nvidia-tensorrt");
        assert_eq!(supervisor.active_backend_name(), "nvidia-tensorrt");
        assert!(supervisor.is_hardware_accelerated());
    } else {
        assert!(info.is_fallback);
        assert!(!info.is_hardware_accelerated);
        assert!(info.name.starts_with("tract"));
    }
}

#[test]
fn supervisor_switches_to_tract_cpu_explicitly() {
    let root = find_repo_root();
    let mut supervisor = EngineSupervisor::default();

    let info = supervisor.select_backend("tract", None, root.as_deref());
    assert!(!info.is_hardware_accelerated);
    assert!(info.name.starts_with("tract"));
    assert_eq!(supervisor.active_backend_name(), info.name);
    assert!(!supervisor.is_hardware_accelerated());
}

#[test]
fn supervisor_falls_back_gracefully_when_accelerator_model_missing() {
    let root = find_repo_root();
    let mut supervisor = EngineSupervisor::default();

    let missing_path = Path::new("/nonexistent/clearcore/models");
    let info = supervisor.select_backend("openvino-npu", Some(missing_path), root.as_deref());

    // Must not panic, must fall back safely to Tract
    assert!(info.is_fallback);
    assert!(!info.is_hardware_accelerated);
    assert!(info.name.starts_with("tract"));
    assert!(info.fallback_reason.is_some());
    assert_eq!(supervisor.active_backend_name(), info.name);
}

#[test]
fn supervisor_process_frame_honours_bypass_and_mute() {
    let root = find_repo_root();
    let mut supervisor = EngineSupervisor::default();
    let _ = supervisor.select_backend("tract", None, root.as_deref());

    let test_frame: AudioFrame = [0.42; HOP_SAMPLES];

    // Bypass mode: returns identical input
    supervisor.set_mode(DenoiseMode::Bypass);
    let bypass_out = supervisor.process_frame(&test_frame).expect("bypass");
    assert_eq!(bypass_out, test_frame);

    // Mute mode: returns digital silence
    supervisor.set_mode(DenoiseMode::Mute);
    let mute_out = supervisor.process_frame(&test_frame).expect("mute");
    assert!(mute_out.iter().all(|&s| s == 0.0));
}

#[test]
fn supervisor_selects_tensorrt_with_stateful_neural_inference_on_hardware() {
    let root = find_repo_root();
    let stateful_dir = find_stateful_model_dir();
    let mut supervisor = EngineSupervisor::default();

    let info = supervisor.select_backend("tensorrt", stateful_dir.as_deref(), root.as_deref());
    if realtime_noise_accelerators::TensorRtBackend::is_available() {
        assert_eq!(info.name, "nvidia-tensorrt");
        assert!(info.is_hardware_accelerated);
        assert!(!info.is_fallback);
        assert_eq!(supervisor.active_backend_name(), "nvidia-tensorrt");
        assert!(supervisor.is_hardware_accelerated());
        assert_eq!(supervisor.active_backend_device().as_deref(), Some("GPU"));

        // Verify inference processes real audio frame without errors
        let test_frame: AudioFrame = [0.1; HOP_SAMPLES];
        let processed = supervisor.process_frame(&test_frame).expect("inference");
        assert!(processed.iter().all(|&s| s.is_finite()));
    }
}

#[test]
fn supervisor_selects_openvino_npu_explicitly_when_available() {
    let root = find_repo_root();
    let stateful_dir = find_stateful_model_dir();
    let mut supervisor = EngineSupervisor::default();

    let info = supervisor.select_backend("openvino-npu", stateful_dir.as_deref(), root.as_deref());
    if realtime_noise_accelerators::OpenVINOBackend::is_available() && stateful_dir.is_some() {
        if !info.is_fallback {
            assert_eq!(info.name, "openvino-npu");
            assert!(info.is_hardware_accelerated);
            assert_eq!(supervisor.active_backend_name(), "openvino-npu");
            assert!(supervisor.is_hardware_accelerated());
            assert_eq!(supervisor.active_backend_device().as_deref(), Some("NPU"));

            let test_frame: AudioFrame = [0.1; HOP_SAMPLES];
            let processed = supervisor.process_frame(&test_frame).expect("inference");
            assert!(processed.iter().all(|&s| s.is_finite()));
        } else {
            // Graceful fallback when host driver does not support dynamic shapes on NPU
            assert_eq!(info.name, "tract");
            assert!(!info.is_hardware_accelerated);
        }
    }
}

struct SlowMockBackend {
    delay: std::time::Duration,
    descriptor: realtime_noise_model::BackendDescriptor,
}

impl SlowMockBackend {
    fn new(delay: std::time::Duration) -> Self {
        Self {
            delay,
            descriptor: realtime_noise_model::BackendDescriptor {
                backend: "mock",
                backend_version: "1.0.0",
                runtime: "mock",
                runtime_version: "1.0.0",
                asset_id: "test-asset".to_string(),
                asset_sha256: "test-sha".to_string(),
                cpu_profile: "generic",
            },
        }
    }
}

impl realtime_noise_model::InferenceBackend for SlowMockBackend {
    fn descriptor(&self) -> realtime_noise_model::BackendDescriptor {
        self.descriptor.clone()
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        1440
    }

    fn process(
        &mut self,
        input: &AudioFrame,
    ) -> Result<realtime_noise_model::ProcessedFrame, realtime_noise_model::InferenceError> {
        if self.delay > std::time::Duration::ZERO {
            std::thread::sleep(self.delay);
        }
        realtime_noise_model::ProcessedFrame::checked(*input, 1440, self.descriptor())
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

#[test]
fn runtime_qualification_rejects_backend_exceeding_deadline() {
    use realtime_noise_supervisor::qualify_backend_runtime;
    use std::time::Duration;

    // A backend that takes 15ms per frame exceeds the 8.0ms threshold and the 10.0ms deadline
    let mut slow_backend = SlowMockBackend::new(Duration::from_millis(15));
    let qualified = qualify_backend_runtime(&mut slow_backend, 8.0);
    assert!(!qualified, "Slow backend taking 15ms must be rejected by runtime qualification");
}

#[test]
fn runtime_qualification_accepts_fast_backend_within_deadline() {
    use realtime_noise_supervisor::qualify_backend_runtime;
    use std::time::Duration;

    // A fast backend taking 0ms per frame passes comfortably
    let mut fast_backend = SlowMockBackend::new(Duration::ZERO);
    let qualified = qualify_backend_runtime(&mut fast_backend, 8.0);
    assert!(qualified, "Fast backend must pass runtime qualification");
}
