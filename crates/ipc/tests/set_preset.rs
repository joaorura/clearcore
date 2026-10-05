#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_ipc::{
    DenoiseMode, IpcCommand, IpcRequest, IpcResponse, IpcServer, IpcStatus, PROTOCOL_VERSION,
    StudioPreset,
};
use serde_json::json;

const SET_PRESET_REQUEST: &str =
    include_str!("../../../fixtures/wire/ipc-v1-set-preset-request.json");

#[test]
fn set_preset_request_matches_golden_wire_bytes() {
    let request = IpcRequest {
        version: PROTOCOL_VERSION.to_string(),
        request_id: "golden-set-preset".to_string(),
        command: IpcCommand::SetPreset(StudioPreset::Podcast),
        payload: json!({}),
    };

    assert_eq!(request.to_json().unwrap(), SET_PRESET_REQUEST.trim_end());
    assert_eq!(
        IpcRequest::from_json(SET_PRESET_REQUEST.trim_end()).unwrap(),
        request
    );
}

#[test]
fn every_preset_round_trips_through_the_wire_format() {
    let cases = [
        (StudioPreset::Off, "Off"),
        (StudioPreset::Natural, "Natural"),
        (StudioPreset::Podcast, "Podcast"),
        (StudioPreset::Broadcast, "Broadcast"),
    ];
    for (preset, name) in cases {
        let request = IpcRequest {
            version: PROTOCOL_VERSION.to_string(),
            request_id: "rt".to_string(),
            command: IpcCommand::SetPreset(preset),
            payload: json!({}),
        };
        let wire = request.to_json().unwrap();
        assert!(
            wire.contains(&format!("\"command\":{{\"SetPreset\":\"{name}\"}}")),
            "unexpected wire form: {wire}"
        );
        assert_eq!(IpcRequest::from_json(&wire).unwrap(), request);
    }
}

#[test]
fn preset_aliases_deserialize_correctly() {
    // Warm and warm -> Podcast
    let req_warm: IpcRequest = serde_json::from_str(
        r#"{"version":"realtime-noise.v1","request_id":"w1","command":{"SetDspPreset":"Warm"},"payload":{}}"#,
    ).unwrap();
    assert_eq!(
        req_warm.command,
        IpcCommand::SetPreset(StudioPreset::Podcast)
    );

    // Radio and radio -> Broadcast
    let req_radio: IpcRequest = serde_json::from_str(
        r#"{"version":"realtime-noise.v1","request_id":"r1","command":{"set_dsp_preset":"Radio"},"payload":{}}"#,
    ).unwrap();
    assert_eq!(
        req_radio.command,
        IpcCommand::SetPreset(StudioPreset::Broadcast)
    );

    // GetPreset aliases
    let req_get: IpcRequest = serde_json::from_str(
        r#"{"version":"realtime-noise.v1","request_id":"g1","command":"get_dsp_preset","payload":{}}"#,
    ).unwrap();
    assert_eq!(req_get.command, IpcCommand::GetPreset);
}

#[test]
fn preset_defaults_to_off() {
    assert_eq!(StudioPreset::default(), StudioPreset::Off);
}

#[test]
fn invalid_preset_is_rejected_without_reaching_the_handler() {
    let server = IpcServer::new();
    for bad in ["Loud", "unknown_xyz", "Off "] {
        let raw = format!(
            "{{\"version\":\"realtime-noise.v1\",\"request_id\":\"bad\",\"command\":{{\"SetPreset\":\"{bad}\"}},\"payload\":{{}}}}"
        );
        let mut handler_called = false;
        let response_json = server.handle_line(&raw, |_cmd, _payload| {
            handler_called = true;
            IpcResponse::success("x", json!({}))
        });

        assert!(!handler_called, "handler must not run for preset {bad:?}");
        let response = IpcResponse::from_json(&response_json).unwrap();
        assert_eq!(response.status, IpcStatus::InvalidCommand);
        let error = response.error.unwrap();
        assert_eq!(error.code, "JSON_PARSE_ERROR");
        assert_eq!(error.message, "malformed request");
    }
}

#[test]
fn existing_v1_commands_keep_their_wire_form() {
    let cases = [
        r#"{"version":"realtime-noise.v1","request_id":"a","command":"GetStatus","payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"b","command":{"SetMode":"Bypass"},"payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"c","command":"RestartGeneration","payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"d","command":"GetDiagnostics","payload":{}}"#,
        r#"{"version":"realtime-noise.v1","request_id":"e","command":"Shutdown","payload":{}}"#,
    ];
    let expected = [
        IpcCommand::GetStatus,
        IpcCommand::SetMode(DenoiseMode::Bypass),
        IpcCommand::RestartGeneration,
        IpcCommand::GetDiagnostics,
        IpcCommand::Shutdown,
    ];
    for (raw, command) in cases.iter().zip(expected) {
        assert_eq!(IpcRequest::from_json(raw).unwrap().command, command);
    }
}
