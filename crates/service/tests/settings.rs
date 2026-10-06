#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use common::TempDir;
use realtime_noise_ipc::StudioPreset;
use realtime_noise_service::settings::{
    SETTINGS_VERSION, Settings, convert_dsp_preset_to_ipc, convert_ipc_preset_to_dsp,
    persist_settings, settings_path_from,
};
use std::path::{Path, PathBuf};
use studio_dsp::Preset;

const ALL_PRESETS: [Preset; 4] = [
    Preset::Off,
    Preset::Natural,
    Preset::Podcast,
    Preset::Broadcast,
];

fn write_settings(dir: &TempDir, text: &str) -> PathBuf {
    let path = dir.path().join("settings.json");
    std::fs::write(&path, text).unwrap();
    path
}

#[test]
fn defaults_are_version_one_with_preset_off() {
    let settings = Settings::default();
    assert_eq!(settings.version, SETTINGS_VERSION);
    assert_eq!(SETTINGS_VERSION, 1);
    assert_eq!(settings.preset, Preset::Off);
    assert_eq!(settings.backend, None);
}

#[test]
fn missing_file_yields_defaults() {
    let dir = TempDir::new("settings-missing");
    let settings = Settings::load(&dir.path().join("settings.json"));
    assert_eq!(settings, Settings::default());
}

#[test]
fn corrupted_or_unusable_files_yield_defaults() {
    let dir = TempDir::new("settings-corrupt");
    let cases = [
        "",
        "not json",
        "{\"version\":1,\"preset\":",
        "{\"version\":1,\"preset\":\"Loud\"}",
        "{\"version\":0,\"preset\":\"Podcast\"}",
        "{\"version\":99,\"preset\":\"Podcast\"}",
        "{\"preset\":\"Podcast\"}",
        "[]",
        "null",
    ];
    for text in cases {
        let path = write_settings(&dir, text);
        assert_eq!(Settings::load(&path), Settings::default(), "case: {text:?}");
    }
}

#[test]
fn version_one_without_preset_field_uses_off() {
    let dir = TempDir::new("settings-no-preset");
    let path = write_settings(&dir, "{\"version\":1}");
    assert_eq!(Settings::load(&path).preset, Preset::Off);
}

#[test]
fn valid_file_is_read() {
    let dir = TempDir::new("settings-valid");
    let path = write_settings(&dir, "{\"version\":1,\"preset\":\"Broadcast\"}");
    let settings = Settings::load(&path);
    assert_eq!(settings.preset, Preset::Broadcast);
    assert_eq!(settings.version, SETTINGS_VERSION);
}

#[test]
fn save_then_load_round_trips_every_preset() {
    let dir = TempDir::new("settings-roundtrip");
    let path = dir.path().join("settings.json");
    for preset in ALL_PRESETS {
        let settings = Settings {
            version: SETTINGS_VERSION,
            preset,
            backend: None,
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
    }
}

#[test]
fn save_writes_versioned_json_creates_parents_and_leaves_no_temporary_file() {
    let dir = TempDir::new("settings-layout");
    let path = dir
        .path()
        .join("nested")
        .join("clearcore")
        .join("settings.json");
    Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Podcast,
        backend: None,
    }
    .save(&path)
    .unwrap();

    let value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(value["version"], 1);
    assert_eq!(value["preset"], "Podcast");
    assert!(!path.with_file_name("settings.json.tmp").exists());
}

#[cfg(unix)]
#[test]
fn save_produces_a_private_file_even_over_a_permissive_one() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new("settings-perms");
    let path = write_settings(&dir, "{}");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();

    Settings::default().save(&path).unwrap();

    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn save_replaces_a_stale_temporary_file() {
    let dir = TempDir::new("settings-stale-tmp");
    let path = dir.path().join("settings.json");
    let stale = dir.path().join("settings.json.tmp");
    std::fs::write(&stale, "garbage left by a crashed run").unwrap();

    Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Natural,
        backend: None,
    }
    .save(&path)
    .unwrap();

    assert!(!stale.exists());
    assert_eq!(Settings::load(&path).preset, Preset::Natural);
}

#[test]
fn failed_save_keeps_the_previous_file_intact() {
    let dir = TempDir::new("settings-failed-save");
    let path = dir.path().join("settings.json");
    Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Natural,
        backend: None,
    }
    .save(&path)
    .unwrap();

    std::fs::create_dir(dir.path().join("settings.json.tmp")).unwrap();
    let result = Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Broadcast,
        backend: None,
    }
    .save(&path);

    assert!(result.is_err());
    assert_eq!(Settings::load(&path).preset, Preset::Natural);
}

#[test]
fn persist_settings_reports_success_failure_and_absence_of_a_path() {
    let dir = TempDir::new("settings-persist");
    let settings = Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Podcast,
        backend: None,
    };

    assert!(persist_settings(
        settings.clone(),
        Some(&dir.path().join("settings.json"))
    ));
    assert!(!persist_settings(settings.clone(), None));

    let blocker = dir.path().join("blocker");
    std::fs::write(&blocker, "a file, not a directory").unwrap();
    assert!(!persist_settings(
        settings,
        Some(&blocker.join("settings.json"))
    ));
}

#[test]
fn settings_round_trips_backend_field() {
    let dir = TempDir::new("settings-backend-roundtrip");
    let path = dir.path().join("settings.json");
    let settings = Settings {
        version: SETTINGS_VERSION,
        preset: Preset::Broadcast,
        backend: Some("nvidia-tensorrt".to_string()),
    };
    settings.save(&path).unwrap();
    assert_eq!(Settings::load(&path), settings);

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"backend\": \"nvidia-tensorrt\""));
}

#[test]
fn settings_loads_backend_from_valid_json() {
    let dir = TempDir::new("settings-backend-load");
    let path = write_settings(
        &dir,
        "{\"version\":1,\"preset\":\"Natural\",\"backend\":\"openvino-gpu\"}",
    );
    let loaded = Settings::load(&path);
    assert_eq!(loaded.preset, Preset::Natural);
    assert_eq!(loaded.backend, Some("openvino-gpu".to_string()));
}

#[test]
fn preset_conversions_are_inverse_of_each_other() {
    let pairs = [
        (StudioPreset::Off, Preset::Off),
        (StudioPreset::Natural, Preset::Natural),
        (StudioPreset::Podcast, Preset::Podcast),
        (StudioPreset::Broadcast, Preset::Broadcast),
    ];
    for (wire, dsp) in pairs {
        assert_eq!(convert_ipc_preset_to_dsp(wire), dsp);
        assert_eq!(convert_dsp_preset_to_ipc(dsp), wire);
    }
}

#[test]
fn settings_path_uses_the_clearcore_subdirectory_of_the_config_base() {
    let base = PathBuf::from("/home/user/.config");
    let path = settings_path_from(Some(base.clone()), Path::new("/tmp"));
    assert_eq!(path, base.join("clearcore").join("settings.json"));
}

#[test]
fn settings_path_falls_back_to_the_temp_dir_without_a_config_base() {
    let path = settings_path_from(None, Path::new("/tmp"));
    assert_eq!(path, PathBuf::from("/tmp").join("clearcore-settings.json"));
}
