#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Spec §8: a diagnostics bundle never carries voice samples, WAVs or profiles.

use realtime_noise_diagnostics::{DiagnosticExporter, RawDiagnosticInput};
use std::fs;

const AUDIO_SENTINEL: &[u8] = b"SENTINEL_AUDIO_BYTES";
const PROFILE_FILES: [&str; 5] = [
    "active_profile.json",
    "voice_samples.json",
    "intake_suggestions.json",
    "profile.bin",
    "a.wav",
];

fn input(causes: Vec<String>) -> RawDiagnosticInput {
    RawDiagnosticInput {
        device_id: "mic-1".to_string(),
        generation: 3,
        causes,
        last_attempt: Some(1),
        pcm_samples: Vec::new(),
        embeddings: Vec::new(),
        meeting_title: None,
        transcript_snippet: None,
        latency_p50_us: 1200,
        latency_p95_us: 2800,
        latency_p99_us: 4500,
        budget_measured_us: 1500,
        budget_configured_us: 3000,
        budget_derived_us: 800,
        budget_unobservable_us: 200,
    }
}

fn assert_clean(output: &str, dir: &str) {
    let sentinel = String::from_utf8_lossy(AUDIO_SENTINEL).into_owned();
    assert!(
        !output.contains(&sentinel),
        "audio sentinel leaked: {output}"
    );
    assert!(
        !output.contains(dir),
        "profile directory path leaked: {output}"
    );
    for name in PROFILE_FILES {
        assert!(!output.contains(name), "{name} leaked: {output}");
    }
    assert!(!output.contains(".wav"), "wav reference leaked: {output}");
    assert!(
        !output.contains("samples/"),
        "samples path leaked: {output}"
    );
}

fn export_both(exporter: &DiagnosticExporter, input: &RawDiagnosticInput) -> [String; 2] {
    let json = exporter.export_json(input).expect("json");
    let archive =
        String::from_utf8(exporter.export_archive(input).expect("archive")).expect("utf8");
    [json, archive]
}

#[test]
fn diagnostics_exclude_voice_samples_wavs_and_profiles() {
    let temp = tempfile::tempdir().expect("tempdir");
    let profile_dir = temp.path().join("profiles");
    fs::create_dir_all(profile_dir.join("samples")).expect("mkdir");
    for name in [
        "active_profile.json",
        "voice_samples.json",
        "intake_suggestions.json",
        "profile.bin",
    ] {
        fs::write(profile_dir.join(name), AUDIO_SENTINEL).expect("write");
    }
    fs::write(profile_dir.join("samples").join("a.wav"), AUDIO_SENTINEL).expect("write wav");
    let dir = profile_dir.display().to_string();
    let sentinel = String::from_utf8_lossy(AUDIO_SENTINEL).into_owned();

    let exporter = DiagnosticExporter::new("voice-exclusion-salt");

    // The exporter reads no disk: with a clean input the profile directory cannot show up.
    let clean = input(vec!["inference timeout".to_string()]);
    for output in export_both(&exporter, &clean) {
        assert_clean(&output, &dir);
    }

    // Error paths may name files of the profile directory (and even dump what they read).
    let dirty = input(vec![
        format!("failed to read {dir}/samples/a.wav"),
        format!(
            "failed to parse {}",
            profile_dir.join("active_profile.json").display()
        ),
        format!("bad voice_samples.json at {dir}: {sentinel}"),
        format!("cannot open {}", profile_dir.join("profile.bin").display()),
        format!("intake_suggestions.json unreadable: {sentinel}"),
    ]);
    for output in export_both(&exporter, &dirty) {
        assert_clean(&output, &dir);
    }
}
