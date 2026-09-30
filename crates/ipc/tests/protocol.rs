#![forbid(unsafe_code)]

use realtime_noise_ipc::{handle_request, IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use serde_json::json;

#[test]
fn incompatible_ipc_version_is_rejected_closed() {
    let req = IpcRequest {
        version: "realtime-noise.v0".to_string(),
        request_id: "req-test-1".to_string(),
        command: IpcCommand::GetStatus,
        payload: json!({}),
    };

    let response = handle_request(&req, |_cmd, _payload| {
        IpcResponse::success("req-test-1", json!({"state": "Running"}))
    });

    assert_eq!(response.status, IpcStatus::VersionMismatch);
}
