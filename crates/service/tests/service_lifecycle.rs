#![forbid(unsafe_code)]

use std::io::Cursor;
use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_service::ServiceDaemon;
use serde_json::json;

#[test]
fn service_survives_control_client_disconnect_and_serves_next_client() {
    let mut daemon = ServiceDaemon::new();

    // Client 1: connect, send GetStatus, receive response, disconnect
    let req1 = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "client-1-req".to_string(),
        command: IpcCommand::GetStatus,
        payload: json!({}),
    };
    let client1_in = format!("{}\n", req1.to_json().expect("serialize req1"));
    let mut client1_reader = Cursor::new(client1_in.into_bytes());
    let mut client1_writer = Cursor::new(Vec::new());

    daemon
        .serve_client(&mut client1_reader, &mut client1_writer)
        .expect("Client 1 session should succeed");

    let client1_out = String::from_utf8(client1_writer.into_inner()).expect("utf8");
    let resp1: IpcResponse = IpcResponse::from_json(client1_out.trim()).expect("parse resp1");
    assert_eq!(resp1.status, IpcStatus::Ok);

    // Client 1 is now disconnected (Cursor ended).
    // Client 2 connects to the same service instance.
    let req2 = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "client-2-req".to_string(),
        command: IpcCommand::GetStatus,
        payload: json!({}),
    };
    let client2_in = format!("{}\n", req2.to_json().expect("serialize req2"));
    let mut client2_reader = Cursor::new(client2_in.into_bytes());
    let mut client2_writer = Cursor::new(Vec::new());

    daemon
        .serve_client(&mut client2_reader, &mut client2_writer)
        .expect("Client 2 session should succeed after client 1 disconnect");

    let client2_out = String::from_utf8(client2_writer.into_inner()).expect("utf8");
    let resp2: IpcResponse = IpcResponse::from_json(client2_out.trim()).expect("parse resp2");
    assert_eq!(resp2.status, IpcStatus::Ok);
    assert_eq!(resp2.request_id, "status-resp");
}
