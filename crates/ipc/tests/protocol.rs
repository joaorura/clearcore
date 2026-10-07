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
    assert!(error.message == "unsupported protocol version");
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

#[test]
fn set_voice_profile_roundtrips_through_json() {
    let command = IpcCommand::SetVoiceProfile {
        profile_json: r#"{"id":"spk-1"}"#.to_string(),
    };
    let request = IpcRequest::new(command.clone(), json!({}));

    let wire = request.to_json().expect("serialize");
    let parsed = IpcRequest::from_json(&wire).expect("deserialize");

    assert_eq!(parsed.command, command);
    assert_eq!(parsed.version, PROTOCOL_VERSION);
    let wire_value: serde_json::Value = serde_json::from_str(&wire).expect("json");
    assert_eq!(
        wire_value["command"],
        json!({"SetVoiceProfile": {"profile_json": r#"{"id":"spk-1"}"#}})
    );
}

#[test]
fn clear_voice_profile_roundtrips_through_json() {
    let request = IpcRequest::new(IpcCommand::ClearVoiceProfile, json!({}));

    let wire = request.to_json().expect("serialize");
    let parsed = IpcRequest::from_json(&wire).expect("deserialize");

    assert_eq!(parsed.command, IpcCommand::ClearVoiceProfile);
    let wire_value: serde_json::Value = serde_json::from_str(&wire).expect("json");
    assert_eq!(wire_value["command"], json!("ClearVoiceProfile"));
}

#[test]
fn set_voice_isolation_roundtrips_through_json() {
    let cmd = IpcCommand::SetVoiceIsolation { enabled: true };
    let req = IpcRequest::new(cmd.clone(), json!({}));
    let wire = req.to_json().expect("serialize");
    let parsed = IpcRequest::from_json(&wire).expect("deserialize");
    assert_eq!(parsed.command, cmd);

    let wire_value: serde_json::Value = serde_json::from_str(&wire).expect("json");
    assert_eq!(
        wire_value["command"],
        json!({"SetVoiceIsolation": {"enabled": true}})
    );
}

#[test]
fn existing_commands_keep_their_wire_format() {
    assert_eq!(
        serde_json::to_value(IpcCommand::GetStatus).expect("serialize"),
        json!("GetStatus")
    );
    assert_eq!(
        serde_json::to_value(IpcCommand::SetMode(DenoiseMode::Bypass)).expect("serialize"),
        json!({"SetMode": "Bypass"})
    );
    assert_eq!(
        serde_json::to_value(IpcCommand::RestartGeneration).expect("serialize"),
        json!("RestartGeneration")
    );
    assert_eq!(
        serde_json::to_value(IpcCommand::GetDiagnostics).expect("serialize"),
        json!("GetDiagnostics")
    );
    assert_eq!(
        serde_json::to_value(IpcCommand::Shutdown).expect("serialize"),
        json!("Shutdown")
    );
}

#[test]
fn set_backend_roundtrips_through_both_json_formats() {
    // 1. Direct string format: {"SetBackend": "openvino"}
    let cmd1 = IpcCommand::set_backend("openvino");
    let req1 = IpcRequest::new(cmd1, json!({}));
    let wire1 = req1.to_json().expect("serialize req1");
    let parsed1 = IpcRequest::from_json(&wire1).expect("deserialize req1");
    match parsed1.command {
        IpcCommand::SetBackend(payload) => assert_eq!(payload.as_str(), "openvino"),
        _ => panic!("Expected SetBackend"),
    }

    // 2. Named struct format: {"SetBackend": {"backend": "tract"}}
    let raw_named = json!({
        "version": PROTOCOL_VERSION,
        "request_id": "req-b-1",
        "command": {"SetBackend": {"backend": "tract"}},
        "payload": {}
    })
    .to_string();
    let parsed2 = IpcRequest::from_json(&raw_named).expect("deserialize named");
    match parsed2.command {
        IpcCommand::SetBackend(payload) => assert_eq!(payload.as_str(), "tract"),
        _ => panic!("Expected SetBackend"),
    }

    // 3. GetBackend command
    let get_req = IpcRequest::new(IpcCommand::GetBackend, json!({}));
    let get_wire = get_req.to_json().expect("serialize get_req");
    let parsed_get = IpcRequest::from_json(&get_wire).expect("deserialize get_req");
    assert_eq!(parsed_get.command, IpcCommand::GetBackend);
}

#[test]
fn debug_of_set_voice_profile_never_prints_the_biometric_payload() {
    let secret = r#"{"gamma_enc":[0.123456789],"embedding":"SECRET-BIOMETRIC"}"#;
    let command = IpcCommand::SetVoiceProfile {
        profile_json: secret.to_owned(),
    };
    let request = IpcRequest::new(command.clone(), json!({}));

    for rendered in [
        format!("{command:?}"),
        format!("{command:#?}"),
        format!("{request:?}"),
        format!("{request:#?}"),
    ] {
        assert!(!rendered.contains("SECRET-BIOMETRIC"), "{rendered}");
        assert!(!rendered.contains("0.123456789"), "{rendered}");
        assert!(rendered.contains("SetVoiceProfile"), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }
    // Other variants keep their ordinary Debug output.
    assert_eq!(format!("{:?}", IpcCommand::GetStatus), "GetStatus");
    assert_eq!(
        format!("{:?}", IpcCommand::SetMode(DenoiseMode::Mute)),
        "SetMode(Mute)"
    );
    assert_eq!(
        format!("{:?}", IpcCommand::set_backend("auto")),
        "SetBackend(\"auto\")"
    );
    assert_eq!(format!("{:?}", IpcCommand::GetBackend), "GetBackend");
}

#[test]
fn debug_of_voice_sample_and_intake_payloads_is_redacted() {
    let secret = r#"{"embedding":[0.987654321],"audio":"SECRET_AUDIO_WAV"}"#;
    let add_sample = IpcCommand::AddVoiceSample {
        name: "Secret Name".to_owned(),
        pcm_f32_le_b64: secret.to_owned(),
        sample_rate: 48_000,
        device_label: "Secret Mic".to_owned(),
        device_id_hash: "h".to_owned(),
    };
    let add_intake = IpcCommand::AddIntakeSuggestion {
        take_json: secret.to_owned(),
    };

    for cmd in [&add_sample, &add_intake] {
        let rendered = format!("{cmd:?}");
        assert!(!rendered.contains("SECRET_AUDIO_WAV"), "{rendered}");
        assert!(!rendered.contains("0.987654321"), "{rendered}");
        assert!(rendered.contains("redacted"), "{rendered}");
    }

    // List and delete commands
    assert_eq!(
        format!("{:?}", IpcCommand::ListVoiceSamples),
        "ListVoiceSamples"
    );
    assert_eq!(
        format!(
            "{:?}",
            IpcCommand::DeleteVoiceSample {
                id: "sample-1".to_string()
            }
        ),
        "DeleteVoiceSample { id: \"sample-1\" }"
    );
    assert_eq!(
        format!("{:?}", IpcCommand::ListIntakeSuggestions),
        "ListIntakeSuggestions"
    );
    assert_eq!(
        format!(
            "{:?}",
            IpcCommand::DiscardIntakeSuggestion {
                id: "take-1".to_string()
            }
        ),
        "DiscardIntakeSuggestion { id: \"take-1\" }"
    );
}

#[test]
fn voice_samples_and_intake_commands_roundtrip_through_json() {
    let commands = vec![
        IpcCommand::ListVoiceSamples,
        IpcCommand::AddVoiceSample {
            name: "s1".to_string(),
            pcm_f32_le_b64: "AAAA".to_string(),
            sample_rate: 48_000,
            device_label: "Mic".to_string(),
            device_id_hash: "ab12".to_string(),
        },
        IpcCommand::DeleteVoiceSample {
            id: "s1".to_string(),
        },
        IpcCommand::GetVoiceProfileEmbedding,
        IpcCommand::ListIntakeSuggestions,
        IpcCommand::AddIntakeSuggestion {
            take_json: r#"{"id":"t1"}"#.to_string(),
        },
        IpcCommand::ApproveIntakeSuggestion {
            id: "t1".to_string(),
            name: Some("Approved Take".to_string()),
        },
        IpcCommand::DiscardIntakeSuggestion {
            id: "t1".to_string(),
        },
    ];

    for cmd in commands {
        let req = IpcRequest::new(cmd.clone(), json!({}));
        let wire = req.to_json().expect("serialize");
        let parsed = IpcRequest::from_json(&wire).expect("deserialize");
        assert_eq!(parsed.command, cmd);
    }
}

#[test]
fn enrollment_commands_roundtrip_through_json() {
    let cmds = vec![
        IpcCommand::AddVoiceSample {
            name: "n".into(),
            pcm_f32_le_b64: "AAAA".into(),
            sample_rate: 48_000,
            device_label: "Mic".into(),
            device_id_hash: "ab12".into(),
        },
        IpcCommand::BuildVoiceProfile {
            name: "João".into(),
        },
        IpcCommand::GetEnrollmentJob {
            job_id: "job-1".into(),
        },
    ];
    for c in cmds {
        let json = serde_json::to_string(&c).unwrap();
        let back: IpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{c:?}"), format!("{back:?}"));
    }
}

#[test]
fn debug_of_enrollment_commands_never_prints_audio_or_names() {
    let c = IpcCommand::AddVoiceSample {
        name: "Secret Name".into(),
        pcm_f32_le_b64: "SECRETAUDIO".into(),
        sample_rate: 48_000,
        device_label: "Secret Mic".into(),
        device_id_hash: "h".into(),
    };
    let d = format!("{c:?}");
    assert!(!d.contains("SECRETAUDIO") && !d.contains("Secret Name") && !d.contains("Secret Mic"));
    let b = format!(
        "{:?}",
        IpcCommand::BuildVoiceProfile {
            name: "Secret Name".into()
        }
    );
    assert!(!b.contains("Secret Name"));
    assert_eq!(
        format!(
            "{:?}",
            IpcCommand::GetEnrollmentJob {
                job_id: "job-1".into()
            }
        ),
        "GetEnrollmentJob { job_id: \"job-1\" }"
    );
}
