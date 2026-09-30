#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_ipc::{
    DenoiseMode, IpcCommand, IpcRequest, IpcResponse, IpcServer, IpcStatus, PROTOCOL_VERSION,
    handle_request,
};
use serde_json::json;
use std::io::Cursor;

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
    assert_eq!(response.request_id, "req-test-1");
    let error = response.error.expect("error detail should be present");
    assert_eq!(error.code, "VERSION_MISMATCH");
    assert!(error.message.contains("Incompatible protocol version"));
}

#[test]
fn compatible_ipc_version_dispatches_successfully() {
    let req = IpcRequest {
        version: PROTOCOL_VERSION.to_string(),
        request_id: "req-test-2".to_string(),
        command: IpcCommand::SetMode(DenoiseMode::Bypass),
        payload: json!({}),
    };

    let response = handle_request(&req, |cmd, _payload| match cmd {
        IpcCommand::SetMode(mode) => {
            assert_eq!(*mode, DenoiseMode::Bypass);
            IpcResponse::success("req-test-2", json!({"applied": true}))
        }
        _ => panic!("Unexpected command"),
    });

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.payload["applied"], true);
}

#[test]
fn roundtrip_over_stream_transport() {
    let server = IpcServer::new();

    let request = IpcRequest {
        version: PROTOCOL_VERSION.to_string(),
        request_id: "client-roundtrip-1".to_string(),
        command: IpcCommand::GetStatus,
        payload: json!({}),
    };

    let mut client_to_server = Vec::new();
    let mut server_to_client = Vec::new();

    let req_json = request.to_json().expect("serialize");
    client_to_server.extend_from_slice(req_json.as_bytes());
    client_to_server.push(b'\n');

    server
        .handle_stream(
            Cursor::new(&client_to_server),
            &mut server_to_client,
            |cmd, _payload| {
                assert_eq!(*cmd, IpcCommand::GetStatus);
                IpcResponse::success("client-roundtrip-1", json!({"state": "Running"}))
            },
        )
        .expect("handle_stream");

    let response: IpcResponse =
        IpcResponse::from_json(std::str::from_utf8(&server_to_client).expect("utf8").trim())
            .expect("from_json");
    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.payload["state"], "Running");
}

#[test]
fn malformed_json_returns_invalid_command() {
    let server = IpcServer::new();
    let raw = "not a json string";
    let resp_str = server.handle_line(raw, |_cmd, _payload| IpcResponse::success("x", json!({})));

    let resp = IpcResponse::from_json(&resp_str).expect("parse error response");
    assert_eq!(resp.status, IpcStatus::InvalidCommand);
    let err = resp.error.expect("error detail");
    assert_eq!(err.code, "JSON_PARSE_ERROR");
}
