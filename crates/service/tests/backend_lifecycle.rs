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

#[test]
fn service_starts_with_auto_accelerator_and_reports_status() {
    let mut daemon = ServiceDaemon::new();
    let resp = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(resp.status, IpcStatus::Ok);

    let active_backend = resp.payload["active_backend"]
        .as_str()
        .expect("active_backend");
    assert!(!active_backend.is_empty());

    if realtime_noise_accelerators::OpenVINOBackend::is_available() {
        assert!(
            resp.payload["is_hardware_accelerated"]
                .as_bool()
                .unwrap_or(false)
        );
        assert!(active_backend.starts_with("openvino"));
        assert_eq!(resp.payload["backend_device"], "NPU");
        assert_eq!(resp.payload["backend_runtime"], "openvino-npu");
    }
}

#[test]
fn service_switches_backend_via_set_backend_ipc() {
    let mut daemon = ServiceDaemon::new();

    // Switch to Tract CPU explicitly
    let switch_to_tract = send(&mut daemon, IpcCommand::set_backend("tract"));
    assert_eq!(switch_to_tract.status, IpcStatus::Ok);
    assert_eq!(switch_to_tract.payload["success"], true);
    assert_eq!(switch_to_tract.payload["active_backend"], "tract");
    assert_eq!(switch_to_tract.payload["is_hardware_accelerated"], false);

    // Verify GetStatus reflects Tract CPU
    let status_after_tract = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status_after_tract.payload["active_backend"], "tract");
    assert_eq!(status_after_tract.payload["is_hardware_accelerated"], false);

    // Switch back to OpenVINO NPU
    if realtime_noise_accelerators::OpenVINOBackend::is_available() {
        let switch_to_npu = send(&mut daemon, IpcCommand::set_backend("openvino-npu"));
        assert_eq!(switch_to_npu.status, IpcStatus::Ok);
        assert_eq!(switch_to_npu.payload["success"], true);
        assert_eq!(switch_to_npu.payload["active_backend"], "openvino-npu");
        assert_eq!(switch_to_npu.payload["is_hardware_accelerated"], true);
        assert_eq!(switch_to_npu.payload["device"], "NPU");

        let status_after_npu = send(&mut daemon, IpcCommand::GetStatus);
        assert_eq!(status_after_npu.payload["active_backend"], "openvino-npu");
        assert_eq!(status_after_npu.payload["is_hardware_accelerated"], true);
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
