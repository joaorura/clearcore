#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cognitive_complexity
)]

use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_model::ProfileStore;
use realtime_noise_service::{IntakeTake, ServiceDaemon, VOICE_EMBEDDING_DIM, VoiceSample};
use serde_json::json;
use std::io::Cursor;
use std::path::Path;

fn make_unit_vector(idx: usize) -> Vec<f32> {
    let mut v = vec![0.0f32; VOICE_EMBEDDING_DIM];
    v[idx % VOICE_EMBEDDING_DIM] = 1.0;
    v
}

fn send_raw(daemon: &mut ServiceDaemon, command: IpcCommand) -> (String, IpcResponse) {
    let request = IpcRequest::new(command, json!({}));
    let mut reader = Cursor::new(format!("{}\n", request.to_json().expect("ser")).into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(&mut reader, &mut writer)
        .expect("serve");
    let raw = String::from_utf8(writer.into_inner()).expect("utf8");
    let response = IpcResponse::from_json(raw.trim()).expect("parse");
    (raw, response)
}

fn send(daemon: &mut ServiceDaemon, command: IpcCommand) -> IpcResponse {
    send_raw(daemon, command).1
}

fn daemon_with_store(dir: &Path) -> ServiceDaemon {
    ServiceDaemon::with_profile_store(ProfileStore::new(dir))
}

#[test]
fn test_voice_samples_and_intake_ipc_lifecycle() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon_with_store(&dir);

    // Initial status: no samples, no intake takes, no profile
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["voice_samples_count"], 0);
    assert_eq!(status.payload["has_voice_profile"], false);
    assert_eq!(status.payload["intake_pending_count"], 0);

    // List samples when empty
    let list_resp = send(&mut daemon, IpcCommand::ListVoiceSamples);
    assert_eq!(list_resp.status, IpcStatus::Ok);
    assert_eq!(list_resp.payload["total_count"], 0);
    assert_eq!(list_resp.payload["has_profile"], false);

    // The averaged-embedding command is deprecated: a fixed error, never an embedding.
    let emb_resp = send(&mut daemon, IpcCommand::GetVoiceProfileEmbedding);
    assert_eq!(emb_resp.status, IpcStatus::InternalError);
    assert_eq!(
        emb_resp.error.as_ref().expect("error").code,
        "ENROLL_FAILED"
    );

    // Add first sample via IPC
    let sample1 = VoiceSample::new(
        "sample-1",
        "2026-10-05T01:00:00Z",
        "Frase Inicial 1",
        None,
        make_unit_vector(0),
    )
    .expect("sample1");
    // `AddVoiceSample` ingests raw PCM through a background job (see `enrollment_flow.rs`);
    // this test seeds a legacy stored sample directly through the manager.
    daemon
        .voice_samples_mut()
        .add_sample(sample1)
        .expect("add sample1");

    // Status after adding sample 1: a sample is not a profile (none is stored).
    let status2 = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status2.payload["voice_samples_count"], 1);
    assert_eq!(status2.payload["has_voice_profile"], false);

    // Add candidate intake take via IPC
    let take1 = IntakeTake::new(
        "take-100",
        "2026-10-05T01:10:00Z",
        5.2,
        21.0,
        None,
        make_unit_vector(1),
    )
    .expect("take1");
    // Takes now arrive as audio through a job (see `enrollment_flow.rs`); a legacy take is
    // seeded through the engine.
    daemon
        .voice_intake_mut()
        .add_take(take1)
        .expect("seed take1");

    // Status shows 1 pending take
    let status3 = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status3.payload["intake_pending_count"], 1);

    // List intake suggestions
    let list_intake = send(&mut daemon, IpcCommand::ListIntakeSuggestions);
    assert_eq!(list_intake.status, IpcStatus::Ok);
    assert_eq!(list_intake.payload["count"], 1);
    assert_eq!(list_intake.payload["suggestions"][0]["id"], "take-100");

    // Approve intake suggestion
    let approve_resp = send(
        &mut daemon,
        IpcCommand::ApproveIntakeSuggestion {
            id: "take-100".to_string(),
            name: Some("Sugestão Aprovada Reunião".to_string()),
        },
    );
    assert_eq!(approve_resp.status, IpcStatus::Ok);
    assert_eq!(approve_resp.payload["approved"], true);
    assert_eq!(approve_resp.payload["sample_id"], "take-100");
    assert_eq!(approve_resp.payload["has_profile"], false);

    // Status: now 2 samples in gallery, 0 pending takes
    let status4 = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status4.payload["voice_samples_count"], 2);
    assert_eq!(status4.payload["intake_pending_count"], 0);

    // List samples now contains 2 samples
    let list_resp2 = send(&mut daemon, IpcCommand::ListVoiceSamples);
    assert_eq!(list_resp2.payload["total_count"], 2);

    // Delete first sample
    let del_resp = send(
        &mut daemon,
        IpcCommand::DeleteVoiceSample {
            id: "sample-1".to_string(),
        },
    );
    assert_eq!(del_resp.status, IpcStatus::Ok);
    assert_eq!(del_resp.payload["deleted"], true);
    assert_eq!(del_resp.payload["has_profile"], false);

    let status5 = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status5.payload["voice_samples_count"], 1);
}

#[test]
fn test_intake_discard_via_ipc() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon_with_store(&dir);

    // Only audio inside the private `samples/` directory is ever deleted.
    std::fs::create_dir_all(dir.join("samples")).expect("samples dir");
    let audio_file = dir.join("samples").join("discard_call.wav");
    std::fs::write(&audio_file, b"sample wav audio bytes").expect("write");
    assert!(audio_file.is_file());

    let take = IntakeTake::new(
        "take-bad",
        "2026-10-05T02:00:00Z",
        4.5,
        14.2,
        Some(audio_file.to_str().expect("str").to_string()),
        make_unit_vector(5),
    )
    .expect("take");

    daemon.voice_intake_mut().add_take(take).expect("seed take");

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["intake_pending_count"], 1);

    // Discard via IPC
    let discard_resp = send(
        &mut daemon,
        IpcCommand::DiscardIntakeSuggestion {
            id: "take-bad".to_string(),
        },
    );
    assert_eq!(discard_resp.status, IpcStatus::Ok);
    assert_eq!(discard_resp.payload["discarded"], true);

    // File was removed
    assert!(!audio_file.exists());

    let status_after = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status_after.payload["intake_pending_count"], 0);
}

#[test]
fn test_invalid_json_payloads_rejected_safely() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut daemon = daemon_with_store(&temp.path().join("profiles"));

    // Invalid base64 is rejected with the fixed audio code and stores nothing.
    let bad_sample = send(
        &mut daemon,
        IpcCommand::AddVoiceSample {
            name: "n".to_string(),
            pcm_f32_le_b64: "not-base64!".to_string(),
            sample_rate: 48_000,
            device_label: "Mic".to_string(),
            device_id_hash: "h".to_string(),
        },
    );
    assert_eq!(bad_sample.status, IpcStatus::InvalidCommand);
    assert_eq!(
        bad_sample.error.as_ref().expect("error").code,
        "ENROLL_INVALID_AUDIO"
    );
    assert_eq!(daemon.voice_samples().list_samples().len(), 0);

    let bad_take = send(
        &mut daemon,
        IpcCommand::AddIntakeSuggestion {
            take_json: "not-json".to_string(),
        },
    );
    assert_eq!(bad_take.status, IpcStatus::InvalidCommand);

    let not_found_approve = send(
        &mut daemon,
        IpcCommand::ApproveIntakeSuggestion {
            id: "non-existent".to_string(),
            name: None,
        },
    );
    assert_eq!(not_found_approve.status, IpcStatus::InvalidCommand);
}
