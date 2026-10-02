#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_diagnostics::{DiagnosticExporter, DiagnosticPayload, RawDiagnosticInput};

#[test]
fn diagnostic_export_excludes_pcm_embeddings_and_meeting_names() {
    let exporter = DiagnosticExporter::new("test-installation-salt-12345");

    let dirty_input = RawDiagnosticInput {
        device_id: "hw-mic-usb-046d-0825-device-serial-999".to_string(),
        generation: 42,
        causes: vec![
            "Crash: buffer overrun during 'Quarterly Board Meeting (Confidential)' session"
                .to_string(),
            "Transcript leak test: 'We need to fire the CFO immediately'".to_string(),
        ],
        last_attempt: Some(3),
        pcm_samples: vec![0.123, -0.456, 0.789, 0.001, -0.999],
        embeddings: vec![0.85, 0.12, 0.44, 0.99, -0.32],
        meeting_title: Some("Top Secret Strategy Discussion".to_string()),
        transcript_snippet: Some("Shall we acquire Company X for $1B?".to_string()),
        latency_p50_us: 1200,
        latency_p95_us: 2800,
        latency_p99_us: 4500,
        budget_measured_us: 1500,
        budget_configured_us: 3000,
        budget_derived_us: 800,
        budget_unobservable_us: 200,
    };

    let payload: DiagnosticPayload = exporter.export_payload(&dirty_input);
    let json_archive: String = exporter
        .export_json(&dirty_input)
        .expect("serialization should succeed");

    // 1. Strictly verify device ID is salted-hashed, NOT raw.
    assert_ne!(payload.device_id_hash, dirty_input.device_id);
    assert!(!json_archive.contains("hw-mic-usb-046d-0825-device-serial-999"));
    assert_eq!(payload.device_id_hash.len(), 64);

    // 2. Strictly verify meeting titles are absent.
    assert!(!json_archive.contains("Quarterly Board Meeting"));
    assert!(!json_archive.contains("Top Secret Strategy Discussion"));
    assert!(!json_archive.contains("Confidential"));

    // 3. Strictly verify transcripts are absent.
    assert!(!json_archive.contains("We need to fire the CFO"));
    assert!(!json_archive.contains("Shall we acquire Company X"));

    // 4. Strictly verify raw audio samples and embedding vectors are absent.
    assert!(!json_archive.contains("0.123"));
    assert!(!json_archive.contains("-0.456"));
    assert!(!json_archive.contains("0.789"));
    assert!(!json_archive.contains("pcm_samples"));
    assert!(!json_archive.contains("pcm"));
    assert!(!json_archive.contains("embeddings"));
    assert!(!json_archive.contains("embedding"));

    // 5. Verify allowed diagnostic metadata is retained.
    assert_eq!(payload.generation, 42);
    assert_eq!(payload.last_attempt, Some(3));
    assert_eq!(payload.latencies.p50_us, 1200);
    assert_eq!(payload.latencies.p95_us, 2800);
    assert_eq!(payload.latencies.p99_us, 4500);
    assert_eq!(payload.budget.measured_us, 1500);
    assert_eq!(payload.budget.configured_us, 3000);
    assert_eq!(payload.budget.derived_us, 800);
    assert_eq!(payload.budget.unobservable_us, 200);
}

mod voice_profile_guard {
    use realtime_noise_diagnostics::{DiagnosticExporter, RawDiagnosticInput};
    use realtime_noise_model::{BandGains, FiLMVectors, ProfileStore, VoiceProfile};

    const SENTINEL: &str = "BIOMETRIC_SENTINEL_TOKEN_SECRET";

    fn vector(base: f32, step: f32) -> Vec<f32> {
        (0..256_u16)
            .map(|i| step.mul_add(f32::from(i), base))
            .collect()
    }

    fn sentinel_profile() -> VoiceProfile {
        let film = FiLMVectors::new(
            vector(0.7391, 0.0037),
            vector(-0.4127, 0.0011),
            vector(0.8653, 0.0023),
            vector(0.2519, -0.0007),
        )
        .expect("film");
        let eq = BandGains::clamped(&[3.1417; 32]).expect("eq");
        VoiceProfile::new(
            "spk-privacy-1",
            SENTINEL,
            "2026-10-02T12:00:00Z",
            film,
            Some(eq),
        )
        .expect("profile")
    }

    fn dirty_input(profile: &VoiceProfile) -> RawDiagnosticInput {
        let profile_json = profile.to_json().expect("profile json");
        RawDiagnosticInput {
            device_id: format!("device-for-{SENTINEL}"),
            generation: 7,
            causes: vec![
                // The whole profile dump, as an error path might (wrongly) log it.
                format!("failed to apply profile {profile_json}"),
                // The name as `{:?}`/quoted formatting would log it.
                format!("profile {SENTINEL:?} rejected by engine"),
                // Compact JSON with only the FiLM vectors.
                serde_json::to_string(&profile.film).expect("film json"),
            ],
            last_attempt: Some(1),
            pcm_samples: vec![0.1, -0.2, 0.3],
            embeddings: profile.film.gamma_enc.clone(),
            meeting_title: Some(format!("{SENTINEL} {profile_json}")),
            transcript_snippet: Some(format!("{SENTINEL} {}", profile.integrity_hash)),
            latency_p50_us: 1200,
            latency_p95_us: 2800,
            latency_p99_us: 4500,
            budget_measured_us: 1500,
            budget_configured_us: 3000,
            budget_derived_us: 800,
            budget_unobservable_us: 200,
        }
    }

    fn assert_no_voice_profile_material(export: &str, profile: &VoiceProfile) {
        assert!(!export.contains(SENTINEL), "sentinel leaked: {export}");
        assert!(
            !export.contains("BIOMETRIC_SENTINEL"),
            "sentinel fragment leaked: {export}"
        );
        for field in [
            "gamma_enc",
            "beta_enc",
            "gamma_df",
            "beta_df",
            "integrity_hash",
            "spk-privacy-1",
        ] {
            assert!(!export.contains(field), "{field} leaked: {export}");
        }
        assert!(
            !export.contains(&profile.integrity_hash),
            "integrity hash value leaked: {export}"
        );
        // Distinctive FiLM / EQ values must not survive in any form.
        for value in ["0.7391", "0.4127", "0.8653", "0.2519", "3.1417"] {
            assert!(
                !export.contains(value),
                "profile value {value} leaked: {export}"
            );
        }
    }

    #[test]
    fn export_json_never_contains_voice_profile_material() {
        let profile = sentinel_profile();
        let exporter = DiagnosticExporter::new("privacy-guard-salt");

        let export = exporter
            .export_json(&dirty_input(&profile))
            .expect("export");

        assert_no_voice_profile_material(&export, &profile);
    }

    #[test]
    fn export_archive_never_contains_voice_profile_material() {
        let profile = sentinel_profile();
        let exporter = DiagnosticExporter::new("privacy-guard-salt");

        let archive = exporter
            .export_archive(&dirty_input(&profile))
            .expect("export");

        let text = String::from_utf8(archive).expect("utf8");
        assert_no_voice_profile_material(&text, &profile);
    }

    #[test]
    fn profile_directory_contents_are_not_part_of_the_export() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = ProfileStore::new(temp.path().join("profiles"));
        let profile = sentinel_profile();
        store.save_active(&profile).expect("save");
        assert!(store.active_profile_path().is_file());

        // A clean input: nothing in it refers to the profile. The exporter reads no disk, so the
        // profile that exists in the store must not influence its output in any way.
        let mut clean = dirty_input(&profile);
        clean.device_id = "mic-1".to_string();
        clean.causes = vec!["inference timeout".to_string()];
        clean.embeddings = Vec::new();
        clean.meeting_title = None;
        clean.transcript_snippet = None;
        let export = DiagnosticExporter::new("privacy-guard-salt")
            .export_json(&clean)
            .expect("export");

        assert_no_voice_profile_material(&export, &profile);
        let directory = store.dir().display().to_string();
        assert!(!export.contains(&directory));
        assert!(!export.contains("active_profile"));
        let value: serde_json::Value = serde_json::from_str(&export).expect("json");
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "budget",
                "causes",
                "device_id_hash",
                "generation",
                "last_attempt",
                "latencies",
                "schema_version",
                "timestamp_utc"
            ]
        );
    }
}
