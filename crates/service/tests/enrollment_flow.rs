#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

mod common;

use common::TempDir;
use realtime_noise_ipc::enrollment_codes::{ENROLL_PAYLOAD_TOO_LARGE, MAX_REQUEST_LINE_BYTES};
use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_service::ServiceDaemon;
use serde_json::json;
use std::io::Cursor;

fn request_line(command: IpcCommand) -> String {
    format!(
        "{}\n",
        IpcRequest::new(command, json!({})).to_json().expect("ser")
    )
}

fn responses(raw: &[u8]) -> Vec<IpcResponse> {
    String::from_utf8(raw.to_vec())
        .expect("utf8")
        .lines()
        .map(|l| IpcResponse::from_json(l).expect("parse"))
        .collect()
}

#[test]
fn oversized_request_line_is_rejected_and_the_connection_survives() {
    let _temp = TempDir::new("enroll-oversized");
    let mut daemon =
        ServiceDaemon::with_supervisor(realtime_noise_supervisor::EngineSupervisor::default());
    let mut input = vec![b'x'; MAX_REQUEST_LINE_BYTES + 1];
    input.push(b'\n');
    input.extend_from_slice(request_line(IpcCommand::GetStatus).as_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(Cursor::new(input), &mut writer)
        .expect("serve");
    let replies = responses(&writer.into_inner());
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].status, IpcStatus::InvalidCommand);
    assert_eq!(
        replies[0].error.as_ref().expect("error").code,
        ENROLL_PAYLOAD_TOO_LARGE
    );
    assert_eq!(replies[1].status, IpcStatus::Ok);
    assert!(replies[1].payload.get("state").is_some());
}

#[test]
fn invalid_utf8_line_gets_a_fixed_parse_error_and_the_connection_survives() {
    let mut daemon =
        ServiceDaemon::with_supervisor(realtime_noise_supervisor::EngineSupervisor::default());
    let mut input = vec![0xff, 0xfe, b'S', b'E', b'C', b'R', b'E', b'T', b'\n'];
    input.extend_from_slice(request_line(IpcCommand::GetStatus).as_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(Cursor::new(input), &mut writer)
        .expect("serve");
    let raw = writer.into_inner();
    assert!(!String::from_utf8_lossy(&raw).contains("SECRET"));
    let replies = responses(&raw);
    assert_eq!(replies.len(), 2);
    assert_eq!(
        replies[0].error.as_ref().expect("error").code,
        "JSON_PARSE_ERROR"
    );
    assert_eq!(replies[1].status, IpcStatus::Ok);
}
