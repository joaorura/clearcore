#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_model::{
    ACTIVE_PROFILE_FILE_NAME, BandGains, FiLMVectors, ProfileStore, VoiceProfile,
};
use realtime_noise_service::ServiceDaemon;
use serde_json::json;
use std::io::Cursor;
use std::path::Path;

const SENTINEL_NAME: &str = "BIOMETRIC_SENTINEL_TOKEN_SECRET";

fn film_vector(base: f32, step: f32) -> Vec<f32> {
    (0..256_u16)
        .map(|i| step.mul_add(f32::from(i), base))
        .collect()
}

fn test_profile(id: &str, name: &str) -> VoiceProfile {
    let film = FiLMVectors::new(
        film_vector(0.5, 0.003),
        film_vector(-0.25, 0.001),
        film_vector(0.75, 0.002),
        film_vector(0.125, -0.0005),
    )
    .expect("film");
    let eq = BandGains::clamped(&[1.5; 32]).expect("eq");
    VoiceProfile::new(id, name, "2026-10-02T12:00:00Z", film, Some(eq)).expect("profile")
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

fn set_profile(daemon: &mut ServiceDaemon, json: &str) -> (String, IpcResponse) {
    send_raw(
        daemon,
        IpcCommand::SetVoiceProfile {
            profile_json: json.to_string(),
        },
    )
}

fn daemon_with_store(dir: &Path) -> ServiceDaemon {
    ServiceDaemon::with_profile_store(ProfileStore::new(dir))
}

#[test]
fn valid_profile_is_persisted_with_0600_and_reported_by_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon_with_store(&dir);

    let profile = test_profile("spk-1", SENTINEL_NAME);
    let (raw, response) = set_profile(&mut daemon, &profile.to_json().expect("json"));

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(
        response.payload,
        json!({"active_voice_profile_id": "spk-1"})
    );
    assert!(
        !raw.contains(SENTINEL_NAME),
        "success must not echo the name"
    );
    assert!(!raw.contains("gamma_enc"));

    let path = dir.join(ACTIVE_PROFILE_FILE_NAME);
    assert!(path.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], "spk-1");
    assert_eq!(status.payload["voice_profile_selected"], true);
    // The engine does not apply the profile to the audio yet, so it must never claim to.
    assert_eq!(status.payload["is_voice_profile_active"], false);
}

#[test]
fn status_without_profile_reports_inactive_and_keeps_legacy_fields() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut daemon = daemon_with_store(&temp.path().join("profiles"));

    let status = send(&mut daemon, IpcCommand::GetStatus);

    assert_eq!(status.payload["active_voice_profile_id"], json!(null));
    assert_eq!(status.payload["voice_profile_selected"], false);
    assert_eq!(status.payload["is_voice_profile_active"], false);
    for field in [
        "state",
        "is_terminal",
        "can_restart",
        "mode",
        "crash_count_15m",
        "total_crashes",
    ] {
        assert!(
            status.payload.get(field).is_some(),
            "legacy field {field} must remain"
        );
    }
}

#[test]
fn clear_removes_the_file_and_resets_status() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon_with_store(&dir);
    let profile = test_profile("spk-1", "Alice");
    set_profile(&mut daemon, &profile.to_json().expect("json"));
    assert!(dir.join(ACTIVE_PROFILE_FILE_NAME).exists());

    let response = send(&mut daemon, IpcCommand::ClearVoiceProfile);

    assert_eq!(response.status, IpcStatus::Ok);
    assert!(!dir.join(ACTIVE_PROFILE_FILE_NAME).exists());
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], json!(null));
    assert_eq!(status.payload["voice_profile_selected"], false);
}

#[test]
fn clear_without_store_is_ok_and_idempotent() {
    let mut daemon = ServiceDaemon::new();
    assert_eq!(
        send(&mut daemon, IpcCommand::ClearVoiceProfile).status,
        IpcStatus::Ok
    );
}

#[test]
fn invalid_json_is_rejected_without_writing_or_echoing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon_with_store(&dir);
    let garbage = format!("{{\"name\":\"{SENTINEL_NAME}\", not json");

    let (raw, response) = set_profile(&mut daemon, &garbage);

    assert_eq!(response.status, IpcStatus::InvalidCommand);
    assert_eq!(response.error.expect("error").code, "INVALID_COMMAND");
    assert!(!raw.contains(SENTINEL_NAME));
    assert!(!dir.join(ACTIVE_PROFILE_FILE_NAME).exists());
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["voice_profile_selected"], false);
}

#[test]
fn tampered_profile_is_rejected_without_writing_or_echoing() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon_with_store(&dir);
    let original = test_profile("spk-1", "Alice");
    let tampered = original
        .to_json()
        .expect("json")
        .replace("Alice", SENTINEL_NAME);

    let (raw, response) = set_profile(&mut daemon, &tampered);

    assert_eq!(response.status, IpcStatus::InvalidCommand);
    assert!(!raw.contains(SENTINEL_NAME));
    assert!(!raw.contains(&original.integrity_hash));
    assert!(!raw.contains("gamma_enc"));
    assert!(!dir.join(ACTIVE_PROFILE_FILE_NAME).exists());
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], json!(null));
}

#[test]
fn rejected_profile_keeps_the_previous_active_profile() {
    let temp = tempfile::tempdir().expect("tempdir");
    let mut daemon = daemon_with_store(&temp.path().join("profiles"));
    set_profile(
        &mut daemon,
        &test_profile("spk-1", "Alice").to_json().expect("json"),
    );

    let (_, response) = set_profile(&mut daemon, "{}");

    assert_eq!(response.status, IpcStatus::InvalidCommand);
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], "spk-1");
}

#[test]
fn set_without_store_reports_no_profile_store() {
    let mut daemon = ServiceDaemon::new();
    let json = test_profile("spk-1", SENTINEL_NAME)
        .to_json()
        .expect("json");

    let (raw, response) = set_profile(&mut daemon, &json);

    assert_eq!(response.status, IpcStatus::InternalError);
    assert_eq!(response.error.expect("error").code, "NO_PROFILE_STORE");
    assert!(!raw.contains(SENTINEL_NAME));
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["voice_profile_selected"], false);
}

#[test]
fn io_failure_while_saving_is_a_generic_internal_error() {
    let temp = tempfile::tempdir().expect("tempdir");
    let blocker = temp.path().join("not-a-directory");
    std::fs::write(&blocker, b"x").expect("blocker");
    let mut daemon = daemon_with_store(&blocker.join("profiles"));
    let json = test_profile("spk-1", SENTINEL_NAME)
        .to_json()
        .expect("json");

    let (raw, response) = set_profile(&mut daemon, &json);

    assert_eq!(response.status, IpcStatus::InternalError);
    assert_eq!(response.error.expect("error").code, "INTERNAL_ERROR");
    assert!(!raw.contains(SENTINEL_NAME));
    assert!(!raw.contains("not-a-directory"), "no filesystem path leaks");
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["voice_profile_selected"], false);
}

#[test]
fn restart_with_same_directory_reloads_the_profile() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    {
        let mut daemon = daemon_with_store(&dir);
        set_profile(
            &mut daemon,
            &test_profile("spk-7", "Alice").to_json().expect("json"),
        );
    }

    let mut restarted = daemon_with_store(&dir);

    let status = send(&mut restarted, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], "spk-7");
    assert_eq!(status.payload["voice_profile_selected"], true);
    // The engine does not apply the profile to the audio yet, so it must never claim to.
    assert_eq!(status.payload["is_voice_profile_active"], false);
}

#[cfg(unix)]
#[test]
fn preexisting_world_readable_profile_is_not_loaded() {
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let store = ProfileStore::new(&dir);
    store
        .save_active(&test_profile("spk-1", "Alice"))
        .expect("save");
    std::fs::set_permissions(
        store.active_profile_path(),
        std::fs::Permissions::from_mode(0o644),
    )
    .expect("chmod");

    let mut daemon = ServiceDaemon::with_profile_store(store);

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], json!(null));
    assert_eq!(status.payload["voice_profile_selected"], false);
}

#[test]
fn preexisting_tampered_profile_is_not_loaded() {
    let temp = tempfile::tempdir().expect("tempdir");
    let dir = temp.path().join("profiles");
    let store = ProfileStore::new(&dir);
    store
        .save_active(&test_profile("spk-1", "Alice"))
        .expect("save");
    let path = store.active_profile_path();
    let tampered = std::fs::read_to_string(&path)
        .expect("read")
        .replace("Alice", "Imposter");
    std::fs::write(&path, tampered).expect("write");

    let mut daemon = ServiceDaemon::with_profile_store(store);

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["voice_profile_selected"], false);
}
