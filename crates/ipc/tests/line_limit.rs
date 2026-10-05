#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_ipc::enrollment_codes::ENROLL_PAYLOAD_TOO_LARGE;
use realtime_noise_ipc::line_limit::{LineRead, read_line_limited};
use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcServer, IpcStatus};
use serde_json::json;
use std::io::{BufReader, Cursor};

fn responses(out: &[u8]) -> Vec<IpcResponse> {
    std::str::from_utf8(out)
        .expect("utf8")
        .lines()
        .map(|l| IpcResponse::from_json(l).expect("response json"))
        .collect()
}

#[test]
fn short_line_is_returned() {
    let mut r = Cursor::new(b"hello\n".to_vec());
    let mut buf = String::from("stale");
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 64).unwrap(),
        LineRead::Line
    );
    assert_eq!(buf, "hello\n");
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 64).unwrap(),
        LineRead::Eof
    );
    assert!(buf.is_empty());
}

#[test]
fn oversized_line_is_discarded_and_next_line_is_read() {
    let mut data = vec![b'a'; 11];
    data.extend_from_slice(b"\nnext\n");
    // tiny internal buffer forces the discard across many fill_buf calls
    let mut r = BufReader::with_capacity(3, Cursor::new(data));
    let mut buf = String::new();
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 10).unwrap(),
        LineRead::TooLong
    );
    assert!(buf.is_empty());
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 10).unwrap(),
        LineRead::Line
    );
    assert_eq!(buf, "next\n");
}

#[test]
fn line_exactly_at_limit_is_accepted() {
    let mut r = Cursor::new(b"abcd\nz\n".to_vec());
    let mut buf = String::new();
    // the limit counts the bytes of the line including the newline
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 5).unwrap(),
        LineRead::Line
    );
    assert_eq!(buf, "abcd\n");
}

#[test]
fn eof_without_newline_returns_partial_then_eof() {
    let mut r = Cursor::new(b"partial".to_vec());
    let mut buf = String::new();
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 64).unwrap(),
        LineRead::Line
    );
    assert_eq!(buf, "partial");
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 64).unwrap(),
        LineRead::Eof
    );
}

#[test]
fn oversized_line_without_newline_then_eof() {
    let mut r = Cursor::new(vec![b'x'; 50]);
    let mut buf = String::new();
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 10).unwrap(),
        LineRead::TooLong
    );
    assert_eq!(
        read_line_limited(&mut r, &mut buf, 10).unwrap(),
        LineRead::Eof
    );
}

#[test]
fn invalid_utf8_is_invalid_data_with_fixed_message() {
    let mut r = Cursor::new(vec![0xff, 0xfe, b'\n']);
    let mut buf = String::new();
    let err = read_line_limited(&mut r, &mut buf, 64).unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(err.to_string(), "request line is not valid UTF-8");
}

fn get_status_line() -> String {
    let req = IpcRequest {
        version: realtime_noise_ipc::PROTOCOL_VERSION.to_string(),
        request_id: "ok-1".to_string(),
        command: IpcCommand::GetStatus,
        payload: json!({}),
    };
    format!("{}\n", req.to_json().unwrap())
}

#[test]
fn handle_stream_rejects_giant_line_and_keeps_serving() {
    let server = IpcServer::new();
    let mut input = vec![b'a'; 4096];
    input.push(b'\n');
    input.extend_from_slice(get_status_line().as_bytes());
    let mut out = Vec::new();
    server
        .handle_stream_with_limit(Cursor::new(input), &mut out, 1024, |_c, _p| {
            IpcResponse::success("ok-1", json!({"state": "Running"}))
        })
        .unwrap();
    let rs = responses(&out);
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0].status, IpcStatus::InvalidCommand);
    assert_eq!(rs[0].error.as_ref().unwrap().code, ENROLL_PAYLOAD_TOO_LARGE);
    assert_eq!(rs[1].status, IpcStatus::Ok);
    assert_eq!(rs[1].request_id, "ok-1");
}

#[test]
fn parse_error_does_not_echo_input() {
    let server = IpcServer::new();
    let resp = server.handle_line(r#"{"Secret":"TOKEN123""#, |_c, _p| {
        IpcResponse::success("x", json!({}))
    });
    let resp = IpcResponse::from_json(&resp).unwrap();
    let err = resp.error.unwrap();
    assert_eq!(err.code, "JSON_PARSE_ERROR");
    assert_eq!(err.message, "malformed request");
    for piece in ["TOKEN123", "Secret", "TOKEN"] {
        assert!(!err.message.contains(piece));
    }
}

#[test]
fn invalid_utf8_does_not_kill_the_server() {
    let server = IpcServer::new();
    let mut input = vec![0xff, 0xfe, 0xfd, b'\n'];
    input.extend_from_slice(get_status_line().as_bytes());
    let mut out = Vec::new();
    server
        .handle_stream(Cursor::new(input), &mut out, |_c, _p| {
            IpcResponse::success("ok-1", json!({}))
        })
        .unwrap();
    let rs = responses(&out);
    assert_eq!(rs.len(), 2);
    assert_eq!(rs[0].error.as_ref().unwrap().code, "JSON_PARSE_ERROR");
    assert_eq!(rs[1].status, IpcStatus::Ok);
}
