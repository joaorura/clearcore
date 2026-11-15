//! The pDFNet3 Pro v7 around the bundled isolation backend: a v7 conditioning (with speaker
//! tokens) switches the hops to the v7, a v3 one (no tokens) is refused, intensity 0 and `None`
//! go back to the base model.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;

use realtime_noise_model::{SpeakerConditioning, pdfnet3_bundle};
use realtime_noise_supervisor::EngineSupervisor;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn vector(file: &str) -> Vec<f32> {
    std::fs::read(
        root()
            .join("crates/model/tests/fixtures/pdfnet3_v7/alvo_snr0")
            .join(file),
    )
    .unwrap()
    .chunks_exact(4)
    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
    .collect()
}

fn conditioning(tokens: bool, intensity_percent: u8) -> SpeakerConditioning {
    let embedding = vector("speaker_embedding.f32");
    SpeakerConditioning {
        profile_id: "p7".to_owned(),
        model_id: "pdfnet3-pro-v7-a2-300k".to_owned(),
        speaker_embedding: embedding.try_into().unwrap(),
        eq: None,
        intensity_percent,
        speaker_tokens: tokens.then(|| Arc::from(vector("tokens.f32"))),
    }
}

fn supervisor() -> EngineSupervisor {
    let models = root().join("models");
    let mut supervisor = EngineSupervisor::default();
    supervisor.require_isolation_model(Ok(pdfnet3_bundle::load_isolation_archive_from(
        std::slice::from_ref(&models),
    )
    .unwrap()));
    supervisor.set_v7_asset(Some(pdfnet3_bundle::load_v7_asset_from(&[models]).unwrap()));
    let info = supervisor.select_backend("auto", None, None);
    assert_eq!(info.name, "tract");
    supervisor
}

#[test]
fn v7_conditioning_switches_to_the_v7_and_back() {
    let mut supervisor = supervisor();
    assert!(supervisor.v7_available());
    assert_eq!(supervisor.active_isolation_variant(), "pdfnet3-pro-v3");
    let base_latency = supervisor.backend().unwrap().algorithmic_latency_samples();

    // A profile built for the v3 model (no tokens) is refused; nothing changes.
    assert!(
        supervisor
            .apply_speaker_conditioning(Some(&conditioning(false, 100)))
            .is_err()
    );
    assert_eq!(supervisor.active_conditioning_id(), None);

    supervisor
        .apply_speaker_conditioning(Some(&conditioning(true, 100)))
        .unwrap();
    assert_eq!(supervisor.active_isolation_variant(), "pdfnet3-pro-v7");
    let backend = supervisor.backend().unwrap();
    assert_eq!(
        backend.algorithmic_latency_samples() - base_latency,
        21 * 480,
        "v7 adds 21 frames"
    );
    assert_eq!(backend.descriptor().asset_id, "pdfnet3-pro-v7");
    let frame = [0.05f32; 480];
    for _ in 0..30 {
        let out = supervisor.process_frame(&frame).unwrap();
        assert!(out.iter().all(|s| s.is_finite()));
    }

    supervisor
        .apply_speaker_conditioning(Some(&conditioning(true, 0)))
        .unwrap();
    assert_eq!(supervisor.active_isolation_variant(), "pdfnet3-pro-v3");
    assert_eq!(
        supervisor.backend().unwrap().algorithmic_latency_samples(),
        base_latency
    );
    supervisor
        .apply_speaker_conditioning(Some(&conditioning(true, 100)))
        .unwrap();
    supervisor.apply_speaker_conditioning(None).unwrap();
    assert_eq!(supervisor.active_isolation_variant(), "pdfnet3-pro-v3");

    // A backend switch keeps the v7 profile applied.
    supervisor
        .apply_speaker_conditioning(Some(&conditioning(true, 100)))
        .unwrap();
    let _ = supervisor.select_backend("tract", None, None);
    assert_eq!(supervisor.active_conditioning_id(), Some("p7"));
    assert_eq!(supervisor.active_isolation_variant(), "pdfnet3-pro-v7");
}
