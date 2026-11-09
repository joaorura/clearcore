#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_service::ServiceDaemon;
use serde_json::json;
use std::io::Cursor;

fn send(daemon: &mut ServiceDaemon, command: IpcCommand) -> IpcResponse {
    let req = IpcRequest::new(command, json!({}));
    let req_str = format!("{}\n", req.to_json().expect("serialize"));
    let mut reader = Cursor::new(req_str.into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(&mut reader, &mut writer)
        .expect("serve");
    let resp_str = String::from_utf8(writer.into_inner()).expect("utf8");
    IpcResponse::from_json(resp_str.trim()).expect("parse")
}

/// Production policy: the bundled pDFNet3 Pro (FiLM) is the only isolation model and runs on
/// tract, whatever accelerators the host has. `auto` is not a fallback.
#[test]
fn service_starts_with_the_bundled_pdfnet3_pro_on_auto() {
    let mut daemon = ServiceDaemon::new();
    let resp = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(resp.status, IpcStatus::Ok);
    assert_eq!(resp.payload["active_backend"], "tract");
    assert_eq!(resp.payload["isolation_model"], "pdfnet3-pro");
    assert_eq!(resp.payload["isolation_model_error"], json!(null));
    assert_eq!(resp.payload["voice_profile_supported"], true);
    assert_eq!(resp.payload["is_hardware_accelerated"], false);
    assert_eq!(resp.payload["backend_device"], "CPU");
}

/// An accelerator request cannot run the pDFNet3 Pro (no FiLM inputs on those backends): it
/// resolves to tract with the Pro model and says so, instead of loading the base `DFNet3`.
#[test]
fn service_switches_backend_via_set_backend_ipc() {
    let mut daemon = ServiceDaemon::new();

    let switch_to_tract = send(&mut daemon, IpcCommand::set_backend("tract"));
    assert_eq!(switch_to_tract.status, IpcStatus::Ok);
    assert_eq!(switch_to_tract.payload["success"], true);
    assert_eq!(switch_to_tract.payload["active_backend"], "tract");
    assert_eq!(switch_to_tract.payload["is_hardware_accelerated"], false);
    assert_eq!(switch_to_tract.payload["fallback"], false);

    let status_after_tract = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status_after_tract.payload["active_backend"], "tract");
    assert_eq!(status_after_tract.payload["isolation_model"], "pdfnet3-pro");

    for accelerator in ["openvino-npu", "tensorrt", "gpu"] {
        let resp = send(&mut daemon, IpcCommand::set_backend(accelerator));
        assert_eq!(resp.status, IpcStatus::Ok, "{accelerator}");
        assert_eq!(resp.payload["active_backend"], "tract", "{accelerator}");
        assert_eq!(resp.payload["is_hardware_accelerated"], false);
        assert_eq!(resp.payload["fallback"], true, "{accelerator}");
        assert!(resp.payload["fallback_reason"].is_string());
        let status = send(&mut daemon, IpcCommand::GetStatus);
        assert_eq!(status.payload["isolation_model"], "pdfnet3-pro");
        assert_eq!(status.payload["voice_profile_supported"], true);
        assert_eq!(status.payload["requested_backend"], accelerator);
    }
}

#[test]
fn service_falls_back_gracefully_when_requested_backend_fails() {
    let mut daemon = ServiceDaemon::new();

    // Point model_dir to nonexistent path and request OpenVINO NPU
    daemon.set_model_dir(std::path::PathBuf::from("/nonexistent/clearcore/models"));

    let resp = send(&mut daemon, IpcCommand::set_backend("openvino-npu"));
    assert_eq!(resp.status, IpcStatus::Ok);
    assert_eq!(resp.payload["success"], true);
    assert_eq!(resp.payload["fallback"], true);
    assert!(resp.payload["fallback_reason"].is_string());
    assert_eq!(resp.payload["active_backend"], "tract");
    assert_eq!(resp.payload["is_hardware_accelerated"], false);
}

#[test]
fn service_answers_get_backend_ipc() {
    let mut daemon = ServiceDaemon::new();
    let resp = send(&mut daemon, IpcCommand::GetBackend);
    assert_eq!(resp.status, IpcStatus::Ok);
    assert!(resp.payload["active_backend"].is_string());
}
