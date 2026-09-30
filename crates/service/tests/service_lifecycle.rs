#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::io::Cursor;
use std::path::Path;
use realtime_noise_ipc::{DenoiseMode, IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_service::install::{
    generate_launchd_plist, generate_systemd_unit, generate_windows_task_cmd,
};
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
    assert_eq!(daemon.served_client_count(), 2);
}

#[test]
fn service_processes_set_mode_and_reports_in_status() {
    let mut daemon = ServiceDaemon::new();

    // SetMode to Bypass
    let set_mode_req = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "mode-req-1".to_string(),
        command: IpcCommand::SetMode(DenoiseMode::Bypass),
        payload: json!({}),
    };
    let mut reader = Cursor::new(format!("{}\n", set_mode_req.to_json().expect("ser")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon.serve_client(&mut reader, &mut writer).expect("serve");

    let resp_str = String::from_utf8(writer.into_inner()).expect("utf8");
    let resp: IpcResponse = IpcResponse::from_json(resp_str.trim()).expect("parse");
    assert_eq!(resp.status, IpcStatus::Ok);

    // GetStatus should reflect Bypass
    let status_req = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "stat-req-1".to_string(),
        command: IpcCommand::GetStatus,
        payload: json!({}),
    };
    let mut reader = Cursor::new(format!("{}\n", status_req.to_json().expect("ser")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon.serve_client(&mut reader, &mut writer).expect("serve");

    let resp_str = String::from_utf8(writer.into_inner()).expect("utf8");
    let resp: IpcResponse = IpcResponse::from_json(resp_str.trim()).expect("parse");
    assert_eq!(resp.status, IpcStatus::Ok);
    assert_eq!(resp.payload["mode"], "Bypass");
}

#[test]
fn service_diagnostics_and_restart_recovery() {
    let mut daemon = ServiceDaemon::new();

    // Cause 6 crashes on the supervisor
    let now = std::time::Instant::now();
    for i in 1..=6 {
        daemon
            .supervisor_mut()
            .record_crash("inference timeout", now + std::time::Duration::from_secs(i));
    }
    assert!(daemon.supervisor().is_terminal());

    // Check diagnostics via IPC
    let diag_req = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "diag-req-1".to_string(),
        command: IpcCommand::GetDiagnostics,
        payload: json!({}),
    };
    let mut reader = Cursor::new(format!("{}\n", diag_req.to_json().expect("ser")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon.serve_client(&mut reader, &mut writer).expect("serve");
    let resp: IpcResponse = IpcResponse::from_json(
        String::from_utf8(writer.into_inner()).expect("utf8").trim(),
    )
    .expect("parse");
    assert_eq!(resp.status, IpcStatus::Ok);
    let diags = resp.payload["diagnostics"].as_array().expect("array");
    assert_eq!(diags.len(), 6);

    // Send RestartGeneration command to recover from TerminalSafeState
    let restart_req = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "restart-req-1".to_string(),
        command: IpcCommand::RestartGeneration,
        payload: json!({}),
    };
    let mut reader = Cursor::new(format!("{}\n", restart_req.to_json().expect("ser")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon.serve_client(&mut reader, &mut writer).expect("serve");
    let resp: IpcResponse = IpcResponse::from_json(
        String::from_utf8(writer.into_inner()).expect("utf8").trim(),
    )
    .expect("parse");
    assert_eq!(resp.status, IpcStatus::Ok);
    assert!(!daemon.supervisor().is_terminal());
}

#[test]
fn service_shutdown_command() {
    let mut daemon = ServiceDaemon::new();
    assert!(!daemon.is_shutdown());

    let shutdown_req = IpcRequest {
        version: "realtime-noise.v1".to_string(),
        request_id: "shut-1".to_string(),
        command: IpcCommand::Shutdown,
        payload: json!({}),
    };
    let mut reader = Cursor::new(format!("{}\n", shutdown_req.to_json().expect("ser")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon.serve_client(&mut reader, &mut writer).expect("serve");
    let resp: IpcResponse = IpcResponse::from_json(
        String::from_utf8(writer.into_inner()).expect("utf8").trim(),
    )
    .expect("parse");
    assert_eq!(resp.status, IpcStatus::Ok);
    assert!(daemon.is_shutdown());
}

#[test]
fn service_packaging_unit_generators() {
    let dummy_bin = Path::new("/opt/clearcore/realtime-noise-service");

    let systemd = generate_systemd_unit(dummy_bin);
    assert!(systemd.contains("Description=Realtime Noise Suppression User Service"));
    assert!(systemd.contains("/opt/clearcore/realtime-noise-service --run"));
    assert!(systemd.contains("WantedBy=default.target"));

    let launchd = generate_launchd_plist(dummy_bin);
    assert!(launchd.contains("<string>com.clearcore.realtime-noise</string>"));
    assert!(launchd.contains("<string>/opt/clearcore/realtime-noise-service</string>"));
    assert!(launchd.contains("<string>--run</string>"));

    let win_cmd = generate_windows_task_cmd(dummy_bin);
    assert!(win_cmd.contains("schtasks.exe /create /tn \"RealtimeNoiseService\""));
    assert!(win_cmd.contains("--run"));
}
