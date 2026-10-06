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

    if realtime_noise_accelerators::TensorRtBackend::is_available() {
        assert!(info.is_hardware_accelerated);
        assert_eq!(info.name, "nvidia-tensorrt");
        assert!(supervisor.is_hardware_accelerated());
        assert_eq!(supervisor.active_backend_name(), info.name);
        assert_eq!(
            supervisor.status().active_backend.as_deref(),
            Some(info.name.as_str())
        );
    } else if stateful_dir.is_some() && realtime_noise_accelerators::OpenVINOBackend::is_available() {
        assert!(info.is_hardware_accelerated);
        assert!(info.name.starts_with("openvino"));
        assert!(supervisor.is_hardware_accelerated());
        assert_eq!(supervisor.active_backend_name(), info.name);
        assert_eq!(
            supervisor.status().active_backend.as_deref(),
            Some(info.name.as_str())
        );
    } else {
        assert!(!info.is_hardware_accelerated);
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
