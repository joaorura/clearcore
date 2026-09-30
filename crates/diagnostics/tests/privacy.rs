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
