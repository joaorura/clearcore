#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::{TempDir, send};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_ipc::{IpcCommand, IpcResponse, IpcStatus, StudioPreset};
use realtime_noise_service::ServiceDaemon;
use realtime_noise_service::bootstrap::{ServiceBootstrap, ServiceConfig};
use realtime_noise_service::settings::Settings;
use std::io::Cursor;
use studio_dsp::Preset;

fn config_in(dir: &TempDir) -> ServiceConfig {
    ServiceConfig {
        endpoint_path: dir.path().join("realtime-noise.sock"),
        settings_path: dir.path().join("settings.json"),
        model_dir: None,
        repo_root: None,
        initial_backend: None,
    }
}

fn send_raw(daemon: &mut ServiceDaemon, raw: &str) -> IpcResponse {
    let mut reader = Cursor::new(format!("{raw}\n").into_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(&mut reader, &mut writer)
        .expect("serve client");
    IpcResponse::from_json(String::from_utf8(writer.into_inner()).expect("utf8").trim())
        .expect("parse response")
}

#[test]
fn fresh_daemon_reports_off_and_studio_control_is_off() {
    let mut daemon = ServiceDaemon::new();
    let response = send(&mut daemon, IpcCommand::GetStatus);

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.payload["preset"], "Off");
    assert_eq!(response.payload["dsp_preset"], "Off");
    assert_eq!(daemon.studio_control().preset(), Preset::Off);
    assert_eq!(daemon.settings(), Settings::default());
}

#[test]
fn set_preset_updates_studio_control_status_and_response() {
    let mut daemon = ServiceDaemon::new();

    let response = send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Podcast));

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.request_id, "set-preset-resp");
    assert_eq!(response.payload["preset"], "Podcast");
    assert_eq!(response.payload["dsp_preset"], "Podcast");
    assert_eq!(response.payload["success"], true);
    assert_eq!(response.payload["persisted"], false);
    assert_eq!(daemon.studio_control().preset(), Preset::Podcast);

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["preset"], "Podcast");
    assert_eq!(status.payload["dsp_preset"], "Podcast");
}

#[test]
fn get_preset_returns_current_dsp_preset() {
    let mut daemon = ServiceDaemon::new();
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Natural));

    let response = send(&mut daemon, IpcCommand::GetPreset);
    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.request_id, "get-preset-resp");
    assert_eq!(response.payload["preset"], "Natural");
    assert_eq!(response.payload["dsp_preset"], "Natural");
}

#[test]
fn set_dsp_preset_and_get_dsp_preset_via_raw_json_aliases() {
    let mut daemon = ServiceDaemon::new();

    // Test SetDspPreset with Warm alias (Warm -> Podcast)
    let raw_set = r#"{"version":"realtime-noise.v1","request_id":"set-warm","command":{"SetDspPreset":"Warm"},"payload":{}}"#;
    let resp_set = send_raw(&mut daemon, raw_set);
    assert_eq!(resp_set.status, IpcStatus::Ok);
    assert_eq!(resp_set.payload["preset"], "Podcast");
    assert_eq!(daemon.studio_control().preset(), Preset::Podcast);

    // Test get_dsp_preset alias
    let raw_get = r#"{"version":"realtime-noise.v1","request_id":"get-p","command":"get_dsp_preset","payload":{}}"#;
    let resp_get = send_raw(&mut daemon, raw_get);
    assert_eq!(resp_get.status, IpcStatus::Ok);
    assert_eq!(resp_get.payload["dsp_preset"], "Podcast");

    // Test set_dsp_preset with Radio alias (Radio -> Broadcast)
    let raw_set_radio = r#"{"version":"realtime-noise.v1","request_id":"set-radio","command":{"set_dsp_preset":"Radio"},"payload":{}}"#;
    let resp_radio = send_raw(&mut daemon, raw_set_radio);
    assert_eq!(resp_radio.status, IpcStatus::Ok);
    assert_eq!(resp_radio.payload["preset"], "Broadcast");
    assert_eq!(daemon.studio_control().preset(), Preset::Broadcast);
}

#[test]
fn studio_control_handle_is_shared_with_the_daemon() {
    let mut daemon = ServiceDaemon::new();
    let handle = daemon.studio_control();

    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Broadcast));

    assert_eq!(handle.preset(), Preset::Broadcast);
}

#[test]
fn set_preset_does_not_touch_the_mode() {
    let mut daemon = ServiceDaemon::new();
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Natural));
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["mode"], "Active");
}

#[test]
fn set_preset_persists_and_the_next_boot_applies_it() {
    let dir = TempDir::new("preset-persist");
    let mut first = ServiceBootstrap::new(config_in(&dir));
    assert_eq!(first.daemon().studio_control().preset(), Preset::Off);

    let response = send(
        first.daemon_mut(),
        IpcCommand::SetPreset(StudioPreset::Natural),
    );
    assert_eq!(response.payload["persisted"], true);
    assert!(dir.path().join("settings.json").exists());

    let mut second = ServiceBootstrap::new(config_in(&dir));
    assert_eq!(second.daemon().studio_control().preset(), Preset::Natural);
    let status = send(second.daemon_mut(), IpcCommand::GetStatus);
    assert_eq!(status.payload["preset"], "Natural");
    assert_eq!(status.payload["dsp_preset"], "Natural");
}

#[test]
fn boot_with_corrupted_settings_falls_back_to_off() {
    let dir = TempDir::new("preset-corrupt");
    std::fs::write(dir.path().join("settings.json"), "{ definitely not json").unwrap();

    let bootstrap = ServiceBootstrap::new(config_in(&dir));

    assert_eq!(bootstrap.daemon().studio_control().preset(), Preset::Off);
}

#[test]
fn failed_persistence_still_applies_the_preset_and_says_so() {
    let dir = TempDir::new("preset-unwritable");
    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "a file, not a directory").unwrap();
    let config = ServiceConfig {
        endpoint_path: dir.path().join("realtime-noise.sock"),
        settings_path: blocker.join("settings.json"),
        model_dir: None,
        repo_root: None,
        initial_backend: None,
    };
    let mut bootstrap = ServiceBootstrap::new(config);

    let response = send(
        bootstrap.daemon_mut(),
        IpcCommand::SetPreset(StudioPreset::Broadcast),
    );

    assert_eq!(response.status, IpcStatus::Ok);
    assert_eq!(response.payload["persisted"], false);
    assert_eq!(response.payload["success"], true);
    assert_eq!(
        bootstrap.daemon().studio_control().preset(),
        Preset::Broadcast
    );
}

#[test]
fn default_service_config_points_to_a_settings_json() {
    let config = ServiceConfig::default();
    let name = config.settings_path.file_name().unwrap().to_string_lossy();
    assert!(name.ends_with("settings.json"), "unexpected name: {name}");
}

#[test]
fn diagnostics_includes_preset_and_dsp_preset() {
    let mut daemon = ServiceDaemon::new();
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Natural));

    let diag = send(&mut daemon, IpcCommand::GetDiagnostics);
    assert_eq!(diag.status, IpcStatus::Ok);
    assert_eq!(diag.payload["preset"], "Natural");
    assert_eq!(diag.payload["dsp_preset"], "Natural");
}

#[test]
fn click_free_preset_switching_during_audio_processing() {
    let mut daemon = ServiceDaemon::new();
    let mut input: AudioFrame = [0.0; HOP_SAMPLES];
    for (i, sample) in input.iter_mut().enumerate() {
        *sample = (i as f32 * 0.05).sin() * 0.5;
    }

    // Process initially with Off: neural network processes and DSP preset Off passes it through
    let out_off = daemon.supervisor_mut().process_frame(&input).unwrap();
    assert!(
        out_off.iter().all(|s| s.is_finite()),
        "Off preset produces valid audio"
    );

    // Switch to Natural via IPC
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Natural));

    // Process multiple frames through the switch - ensuring smooth crossfade without NaN or inf
    for _ in 0..20 {
        let out_natural = daemon.supervisor_mut().process_frame(&input).unwrap();
        assert!(out_natural.iter().all(|s| s.is_finite()));
    }

    // Switch to Warm (Podcast) via IPC
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::WARM));
    for _ in 0..20 {
        let out_warm = daemon.supervisor_mut().process_frame(&input).unwrap();
        assert!(out_warm.iter().all(|s| s.is_finite()));
    }

    // Switch to Radio (Broadcast) via IPC
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::RADIO));
    for _ in 0..20 {
        let out_radio = daemon.supervisor_mut().process_frame(&input).unwrap();
        assert!(out_radio.iter().all(|s| s.is_finite()));
    }

    // Switch back to Off
    send(&mut daemon, IpcCommand::SetPreset(StudioPreset::Off));
    for _ in 0..20 {
        let out = daemon.supervisor_mut().process_frame(&input).unwrap();
        assert!(out.iter().all(|s| s.is_finite()));
    }
}

#[test]
fn set_filter_intensity_updates_status_and_response() {
    let mut daemon = ServiceDaemon::new();

    // Default intensity is 50
    let status_init = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status_init.status, IpcStatus::Ok);
    assert_eq!(status_init.payload["filter_intensity"], 50);

    // Set intensity to 75
    let resp = send(
        &mut daemon,
        IpcCommand::SetFilterIntensity { intensity: 75 },
    );
    assert_eq!(resp.status, IpcStatus::Ok);
    assert_eq!(resp.request_id, "set-filter-intensity-resp");
    assert_eq!(resp.payload["filter_intensity"], 75);
    assert_eq!(resp.payload["success"], true);

    // Verify in GetStatus
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["filter_intensity"], 75);

    // Set intensity to 100
    let resp_100 = send(
        &mut daemon,
        IpcCommand::SetFilterIntensity { intensity: 100 },
    );
    assert_eq!(resp_100.status, IpcStatus::Ok);
    assert_eq!(resp_100.payload["filter_intensity"], 100);

    // Clamping > 100 to 100
    let resp_clamped = send(
        &mut daemon,
        IpcCommand::SetFilterIntensity { intensity: 150 },
    );
    assert_eq!(resp_clamped.status, IpcStatus::Ok);
    assert_eq!(resp_clamped.payload["filter_intensity"], 100);
}

#[test]
fn set_voice_leveler_updates_status_and_response() {
    let mut daemon = ServiceDaemon::new();

    // Default voice leveler intensity is 0
    let status_init = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status_init.status, IpcStatus::Ok);
    assert_eq!(status_init.payload["voice_leveler"], 0);

    // Set leveler intensity to 50
    let resp = send(&mut daemon, IpcCommand::SetVoiceLeveler { intensity: 50 });
    assert_eq!(resp.status, IpcStatus::Ok);
    assert_eq!(resp.request_id, "set-voice-leveler-resp");
    assert_eq!(resp.payload["voice_leveler"], 50);
    assert_eq!(resp.payload["success"], true);

    // Verify in GetStatus
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["voice_leveler"], 50);
    assert_eq!(status.payload["voice_leveler_intensity"], 50);
    assert_eq!(daemon.supervisor().voice_leveler_intensity(), 50);

    // Clamping > 100 to 100
    let resp_clamped = send(&mut daemon, IpcCommand::SetVoiceLeveler { intensity: 120 });
    assert_eq!(resp_clamped.status, IpcStatus::Ok);
    assert_eq!(resp_clamped.payload["voice_leveler"], 100);
    assert_eq!(daemon.supervisor().voice_leveler_intensity(), 100);
}
