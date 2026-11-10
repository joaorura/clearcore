//! pDFNet3 Pro v3 full in the daemon: the single manifest export runs on tract (generic stateful
//! executor) and on OpenVINO, and the supervisor's speaker-conditioning API (the data of the
//! service's `VoiceConditioning`: embedding [192] + EQ + intensity) is coherent with
//! `supports_speaker_embedding`, survives backend switches and refuses `FiLM` profiles.
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use realtime_noise_model::{
    BandGains, FILM_HIDDEN_DIM, FiLMVectors, PDFNET3_PRO_ASSET_ID, SpeakerConditioning,
    VoiceProfile, pdfnet3_bundle,
};
use realtime_noise_supervisor::{EngineSupervisor, ISOLATION_MODEL_PDFNET3_PRO};

fn models() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
}

fn supervisor() -> EngineSupervisor {
    let archive = pdfnet3_bundle::load_isolation_archive_from(&[models()]).unwrap();
    let mut supervisor = EngineSupervisor::default();
    supervisor.require_isolation_model(Ok(archive));
    supervisor
}

fn conditioning(id: &str) -> SpeakerConditioning {
    let mut speaker_embedding = [0.0f32; 192];
    for (index, value) in speaker_embedding.iter_mut().enumerate() {
        *value = ((index as f32) * 0.37).sin();
    }
    let norm = speaker_embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    speaker_embedding.iter_mut().for_each(|v| *v /= norm);
    SpeakerConditioning {
        profile_id: id.to_owned(),
        model_id: "pdfnet3-pro-v3-full".to_owned(),
        speaker_embedding,
        eq: Some(BandGains::from_array([2.0; 32]).unwrap()),
        intensity_percent: 80,
    }
}

fn film_profile() -> VoiceProfile {
    let film = FiLMVectors::new(
        vec![1.1; FILM_HIDDEN_DIM],
        vec![0.02; FILM_HIDDEN_DIM],
        vec![0.9; FILM_HIDDEN_DIM],
        vec![-0.02; FILM_HIDDEN_DIM],
    )
    .unwrap();
    VoiceProfile::new("old", "Speaker", "2026-11-09T12:00:00Z", film, None).unwrap()
}

#[test]
fn v3_full_runs_on_tract_and_takes_the_speaker_conditioning() {
    let mut supervisor = supervisor();
    let info = supervisor.select_backend("auto", None, None);
    assert_eq!(info.name, "tract");
    assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_PDFNET3_PRO);
    let descriptor = supervisor.active_backend_descriptor().unwrap();
    assert_eq!(descriptor.asset_id, PDFNET3_PRO_ASSET_ID);
    assert_eq!(descriptor.backend, "tract-stateful");
    assert!(supervisor.supports_speaker_embedding());
    supervisor
        .apply_speaker_conditioning(Some(&conditioning("p1")))
        .unwrap();
    assert_eq!(supervisor.active_conditioning_id(), Some("p1"));
    assert!(supervisor.active_conditioning_has_eq());
    // A FiLM (v1) profile cannot condition this model: refused, conditioning kept.
    assert!(supervisor.set_voice_profile(Some(&film_profile())).is_err());
    assert_eq!(supervisor.active_conditioning_id(), Some("p1"));
    // Reapplied on a backend switch.
    supervisor.select_backend("tract", None, None);
    assert_eq!(supervisor.active_conditioning_id(), Some("p1"));
    let frame = [0.1f32; realtime_noise_contracts::HOP_SAMPLES];
    assert!(supervisor.process_frame(&frame).is_ok());
    // Neutral voice.
    supervisor.apply_speaker_conditioning(None).unwrap();
    assert_eq!(supervisor.active_conditioning_id(), None);
    assert!(!supervisor.active_conditioning_has_eq());
}

#[test]
fn v3_full_runs_on_openvino_cpu_when_requested() {
    let probe = realtime_noise_runtime_openvino::probe_runtime_cached();
    if !(probe.state.is_usable() && probe.devices.iter().any(|d| d == "CPU")) {
        eprintln!("OpenVINO CPU unavailable; skipping");
        return;
    }
    let mut supervisor = supervisor();
    supervisor
        .apply_speaker_conditioning(None)
        .expect_err("no backend yet");
    let info = supervisor.select_backend("openvino-cpu", None, None);
    assert_eq!(info.name, "openvino-cpu", "{info:?}");
    assert!(!info.is_fallback);
    assert!(supervisor.supports_speaker_embedding());
    supervisor
        .apply_speaker_conditioning(Some(&conditioning("p2")))
        .unwrap();
    assert_eq!(supervisor.active_backend_device().as_deref(), Some("CPU"));
    // Back to tract: the conditioning follows.
    supervisor.select_backend("tract", None, None);
    assert_eq!(supervisor.active_conditioning_id(), Some("p2"));
}
