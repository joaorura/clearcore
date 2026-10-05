#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Spec §8: a diagnostics bundle never carries voice samples, WAVs or profiles.

use realtime_noise_diagnostics::{DiagnosticExporter, RawDiagnosticInput, sanitize_text};
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

const REDACTION: &str = "[REDACTED_VOICE_PROFILE]";

#[test]
fn artifact_paths_are_dropped_in_every_spelling() {
    let leaking = [
        "samples directory /tmp/p/profiles/samples is a symlink",
        "profile directory /Users/alice/Library/Application Support/clearcore/profiles is a symlink",
        r"profile directory C:\Users\alice\AppData\Roaming\clearcore\profiles is a symlink",
        "cannot open /tmp/p/profiles/profile.tmp.1.2",
        "cannot open /tmp/p/profiles/samples/abc.tmp.4242.99887766",
        "cannot rename abc.tmp.4242.99887766 to abc.wav",
        "write failed in profiles: abc.tmp.4242.99887766",
        "failed to read /tmp/p/PROFILES/SAMPLES/A.WAV",
        r"failed to read C:\p\Samples\a.WAV",
        "cannot parse ACTIVE_PROFILE.JSON",
        "ENOENT Voice_Samples.json",
        "samples missing under profiles",
    ];
    for cause in leaking {
        let out = sanitize_text(cause);
        assert_eq!(out, REDACTION, "not dropped: {cause} -> {out}");
    }
    for cause in [
        "samples directory /tmp/p/profiles/samples is a symlink",
        "profile directory /Users/alice/Library/Application Support/clearcore/profiles",
        "cannot open /tmp/p/profiles/profile.tmp.1.2",
    ] {
        let out = sanitize_text(cause);
        assert!(!out.contains("profiles") && !out.contains("alice"), "{out}");
    }
}

#[test]
fn macos_user_paths_are_redacted() {
    let out = sanitize_text("config read failed at /Users/alice/Library/foo/bar.toml");
    assert!(!out.contains("alice"), "{out}");
}

#[test]
fn long_base64_blobs_without_markers_are_dropped() {
    let blob = "UklGR".repeat(20);
    assert_eq!(sanitize_text(&format!("decode failed: {blob}")), REDACTION);
    let slashy = format!("{}/{}", "a".repeat(40), "b".repeat(40));
    assert_eq!(sanitize_text(&format!("blob {slashy}")), REDACTION);
}

#[test]
fn clean_causes_pass_through_intact() {
    for cause in [
        "deadline miss p99 7.2 ms",
        "inference timeout",
        "device lost: sample rate changed",
    ] {
        assert_eq!(sanitize_text(cause), cause);
    }
    // A legitimate 64-char lowercase hex digest is not base64 content.
    let digest = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    assert_eq!(
        sanitize_text(&format!("digest {digest}")),
        format!("digest {digest}")
    );
}
