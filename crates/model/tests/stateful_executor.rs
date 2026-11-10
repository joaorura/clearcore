#![cfg(feature = "tract")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]

//! Generic stateful executor (tract, and ONNX Runtime when `ort` is on) against
//! (a) the synthetic Pro v3 full stand-in `tests/fixtures/pdfnet3-pro-v3-full-synthetic.tar.gz`
//! (`tools/models/make_synthetic_pro_v3_full.py`, one graph, GRU + `LayerNorm` + Softmax + Einsum,
//! `speaker_embedding`/`speaker_present`) and (b) the bundled pDFNet3 Pro v3 full
//! (`models/pdfnet3/pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz`, trio, opset 12).

use std::path::PathBuf;

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::pdfnet3_bundle::{
    PDFNET3_PRO_ASSET_ID, PDFNET3_PRO_V3_FULL_SHA256, load_isolation_archive_from,
};
use realtime_noise_model::stateful_graph::{StatefulGraphBackend, build_executor};
use realtime_noise_model::tract_graph::TractGraphSession;
use realtime_noise_model::{
    BandGains, ConditioningKind, CpuProfile, FiLMVectors, InferenceBackend, PdfNet3DevArchive,
    SpeakerConditioning, VoiceProfile,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn synthetic() -> PdfNet3DevArchive {
    let bytes = std::fs::read(
        root().join("crates/model/tests/fixtures/pdfnet3-pro-v3-full-synthetic.tar.gz"),
    )
    .unwrap();
    let sha = {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        Sha256::digest(&bytes)
            .iter()
            .fold(String::new(), |mut hex, b| {
                let _ = write!(hex, "{b:02x}");
                hex
            })
    };
    PdfNet3DevArchive::verify(bytes, &sha).unwrap()
}

fn real() -> PdfNet3DevArchive {
    load_isolation_archive_from(&[root().join("models")]).unwrap()
}

fn tract_backend(archive: &PdfNet3DevArchive) -> StatefulGraphBackend<TractGraphSession> {
    let executor =
        build_executor(&archive.stateful_files().unwrap(), TractGraphSession::load).unwrap();
    StatefulGraphBackend::new(
        executor,
        archive.descriptor(
            "tract-stateful",
            "tract",
            "0.19.16",
            CpuProfile::Avx2Minimum,
        ),
    )
    .unwrap()
}

/// Deterministic unit-norm embedding.
fn embedding(seed: u32) -> Vec<f32> {
    let mut state = seed.wrapping_mul(2_654_435_761).max(1);
    let mut values: Vec<f32> = (0..192)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) - 0.5
        })
        .collect();
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    for v in &mut values { *v /= norm; }
    values
}

/// Speech-like test signal: harmonic stack with a syllabic envelope plus pseudo-random noise.
#[allow(clippy::suboptimal_flops)] // keep the reference arithmetic; `mul_add` changes rounding; the parity fixtures depend on it
fn signal(frames: usize) -> Vec<AudioFrame> {
    let mut noise = 0x1234_5678u32;
    (0..frames)
        .map(|f| {
            std::array::from_fn(|i| {
                let t = (f * HOP_SAMPLES + i) as f32 / 48_000.0;
                let envelope = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
                let voice: f32 = (1..6)
                    .map(|h| (2.0 * std::f32::consts::PI * 140.0 * h as f32 * t).sin() / h as f32)
                    .sum();
                noise ^= noise << 13;
                noise ^= noise >> 17;
                noise ^= noise << 5;
                0.2 * envelope * voice + 0.05 * ((noise as f32 / u32::MAX as f32) - 0.5)
            })
        })
        .collect()
}

fn run(backend: &mut dyn InferenceBackend, input: &[AudioFrame]) -> Vec<f32> {
    input
        .iter()
        .flat_map(|frame| backend.process(frame).unwrap().samples)
        .collect()
}

fn snr_db(reference: &[f32], test: &[f32]) -> f64 {
    let signal: f64 = reference.iter().map(|&v| f64::from(v).powi(2)).sum();
    let error: f64 = reference
        .iter()
        .zip(test)
        .map(|(&a, &b)| (f64::from(a) - f64::from(b)).powi(2))
        .sum();
    10.0 * (signal / error.max(1e-30)).log10()
}

fn conditioning(seed: u32) -> SpeakerConditioning {
    let gains: [f32; 32] = std::array::from_fn(|band| -3.0 + 6.0 * band as f32 / 31.0);
    let mut speaker_embedding = [0.0f32; 192];
    speaker_embedding.copy_from_slice(&embedding(seed));
    SpeakerConditioning {
        profile_id: "id".to_owned(),
        model_id: "pdfnet3-pro-v3-full".to_owned(),
        speaker_embedding,
        eq: Some(BandGains::from_array(gains).unwrap()),
        intensity_percent: 100,
    }
}

#[test]
fn synthetic_resolves_one_graph_two_states_and_embedding_conditioning() {
    let backend = tract_backend(&synthetic());
    let executor = backend.executor();
    assert_eq!(executor.graph_names(), vec!["pdfnet3_pro_v3_full"]);
    assert_eq!(executor.state_count(), 2);
    assert_eq!(
        executor.conditioning_kind(),
        ConditioningKind::SpeakerEmbedding
    );
    assert!(InferenceBackend::supports_voice_profile(&backend));
}

#[test]
fn states_are_fed_back_and_reset_restores_the_first_hop() {
    let mut backend = tract_backend(&synthetic());
    let executor = backend.executor_mut();
    let erb = [0.3f32; 32];
    let spec = [[0.1f32; 96], [-0.2f32; 96]];
    let first = executor.run_hop(&erb, &spec).unwrap();
    let second = executor.run_hop(&erb, &spec).unwrap();
    // Same features, different state: the GRU/delay-ring outputs were fed back.
    assert_ne!(first.erb_gains, second.erb_gains);
    assert_eq!(executor.frames(), 2);
    executor.reset();
    let again = executor.run_hop(&erb, &spec).unwrap();
    assert_eq!(first.erb_gains, again.erb_gains);
    assert_eq!(first.lsnr.to_bits(), again.lsnr.to_bits());
}

#[test]
fn embedding_changes_output_and_absent_speaker_is_the_fallback() {
    let mut backend = tract_backend(&synthetic());
    let erb = [0.3f32; 32];
    let spec = [[0.1f32; 96], [-0.2f32; 96]];
    let mut hop = |embedding: Option<&[f32]>, present: f32| {
        let executor = backend.executor_mut();
        executor.reset();
        executor
            .set_speaker_conditioning(embedding, present)
            .unwrap();
        executor.run_hop(&erb, &spec).unwrap()
    };
    let fallback = hop(None, 0.0);
    let a_absent = hop(Some(&embedding(1)), 0.0);
    let a = hop(Some(&embedding(1)), 1.0);
    let b = hop(Some(&embedding(2)), 1.0);
    // speaker_present = 0 is exactly the no-profile fallback, whatever the embedding.
    assert_eq!(fallback.erb_gains, a_absent.erb_gains);
    assert_eq!(fallback.lsnr.to_bits(), a_absent.lsnr.to_bits());
    // With the speaker present, the embedding changes the output, and two speakers differ.
    assert_ne!(fallback.erb_gains, a.erb_gains);
    assert_ne!(a.erb_gains, b.erb_gains);
}

#[test]
fn conditioning_api_is_coherent_with_the_graph() -> TestResult {
    let mut backend = tract_backend(&synthetic());
    assert!(InferenceBackend::supports_speaker_embedding(&backend));
    let input = signal(30);
    let neutral = run(&mut backend, &input);
    backend.reset()?;
    InferenceBackend::set_speaker_conditioning(&mut backend, Some(&conditioning(7)))?;
    assert_eq!(
        backend
            .active_speaker_conditioning()
            .map(|c| c.profile_id.as_str()),
        Some("id")
    );
    let conditioned = run(&mut backend, &input);
    assert!(
        snr_db(&neutral, &conditioned) < 40.0,
        "conditioning must change the output"
    );
    // A FiLM (pre-v3) profile cannot be applied to this graph: refused, previous kept.
    let film = FiLMVectors::new(
        vec![1.1; 256],
        vec![0.0; 256],
        vec![1.0; 256],
        vec![0.0; 256],
    )?;
    let legacy = VoiceProfile::new("old", "antigo", "2026-10-01T00:00:00Z", film, None)?;
    assert!(backend.set_voice_profile(Some(&legacy)).is_err());
    assert!(backend.active_speaker_conditioning().is_some());
    // A non-normalised embedding is refused too.
    let mut bad = conditioning(7);
    bad.speaker_embedding[0] += 1.0;
    assert!(backend.set_speaker_conditioning(Some(&bad)).is_err());
    // Intensity 0 is the neutral voice (speaker_present = 0), EQ aside.
    let mut zero = conditioning(7);
    zero.intensity_percent = 0;
    zero.eq = None;
    backend.set_speaker_conditioning(Some(&zero))?;
    backend.reset()?;
    assert_eq!(run(&mut backend, &input), neutral);
    // speaker_present is binary: any intensity > 0 is the full speaker (same output as 100).
    backend.set_speaker_conditioning(Some(&conditioning(7)))?;
    backend.reset()?;
    let full = run(&mut backend, &input);
    let mut half = conditioning(7);
    half.intensity_percent = 50;
    assert!((half.speaker_present() - 1.0).abs() < f32::EPSILON);
    backend.set_speaker_conditioning(Some(&half))?;
    backend.reset()?;
    assert_eq!(run(&mut backend, &input), full);
    // Removing the conditioning restores the neutral output.
    backend.set_speaker_conditioning(None)?;
    assert!(backend.active_speaker_conditioning().is_none());
    backend.reset()?;
    assert_eq!(run(&mut backend, &input), neutral);
    Ok(())
}

#[test]
fn digital_silence_outputs_zero_without_advancing_the_model() {
    let mut backend = tract_backend(&synthetic());
    let silence = [0.0f32; HOP_SAMPLES];
    let out = backend.process(&silence).unwrap();
    assert!(out.samples.iter().all(|&s| s == 0.0));
    assert_eq!(backend.executor().frames(), 0);
    let tiny = [1e-9f32; HOP_SAMPLES];
    backend.process(&tiny).unwrap();
    assert_eq!(backend.executor().frames(), 0);
    backend.process(&signal(1)[0]).unwrap();
    assert_eq!(backend.executor().frames(), 1);
}

#[test]
fn real_v3_full_loads_on_tract_through_the_archive() -> TestResult {
    let archive = real();
    assert_eq!(archive.sha256(), PDFNET3_PRO_V3_FULL_SHA256);
    assert!(archive.is_manifest_export());
    let mut backend = archive.instantiate(CpuProfile::Avx2Minimum)?;
    assert!(backend.is_stateful_export());
    assert!(backend.supports_speaker_conditioning());
    assert!(InferenceBackend::supports_voice_profile(&backend));
    assert_eq!(backend.descriptor().asset_id, PDFNET3_PRO_ASSET_ID);
    let input = signal(50);
    let neutral = run(&mut backend, &input);
    assert!(neutral.iter().all(|s| s.is_finite()));
    assert!(InferenceBackend::supports_speaker_embedding(&backend));
    InferenceBackend::set_speaker_conditioning(&mut backend, Some(&conditioning(3)))?;
    let fresh = archive.instantiate(CpuProfile::Avx2Minimum)?;
    drop(fresh);
    Ok(())
}

#[test]
fn real_v3_full_executor_wiring() {
    let backend = tract_backend(&real());
    let executor = backend.executor();
    assert_eq!(executor.graph_names(), vec!["enc", "erb_dec", "df_dec"]);
    // enc: enc_h, feat_erb_buf, feat_spec_buf; erb_dec: erb_h; df_dec: df_h, c0_buf.
    assert_eq!(executor.state_count(), 6);
    assert_eq!(
        executor.conditioning_kind(),
        ConditioningKind::SpeakerEmbedding
    );
}

#[test]
fn real_v3_full_embedding_changes_output_and_absent_is_fallback() {
    let mut backend = tract_backend(&real());
    let input = signal(40);
    let mut run_with = |embedding: Option<Vec<f32>>, present: f32| {
        backend.reset().unwrap();
        backend
            .executor_mut()
            .set_speaker_conditioning(embedding.as_deref(), present)
            .unwrap();
        run(&mut backend, &input)
    };
    let fallback = run_with(None, 0.0);
    let absent = run_with(Some(embedding(5)), 0.0);
    let present = run_with(Some(embedding(5)), 1.0);
    let other = run_with(Some(embedding(6)), 1.0);
    assert!(
        snr_db(&fallback, &absent) > 100.0,
        "{}",
        snr_db(&fallback, &absent)
    );
    assert!(snr_db(&fallback, &present) < 60.0);
    assert!(snr_db(&present, &other) < 80.0);
}

#[cfg(feature = "ort")]
mod ort_parity {
    use super::*;

    fn ort_backend(
        archive: &PdfNet3DevArchive,
    ) -> StatefulGraphBackend<realtime_noise_model::ort_graph::OrtGraphSession> {
        archive
            .instantiate_stateful_ort(CpuProfile::Avx2Minimum)
            .unwrap()
    }

    fn parity(archive: &PdfNet3DevArchive, frames: usize) -> (f64, f64) {
        let input = signal(frames);
        let mut tract = tract_backend(archive);
        let mut ort = ort_backend(archive);
        let neutral = snr_db(&run(&mut tract, &input), &run(&mut ort, &input));
        tract.reset().unwrap();
        ort.reset().unwrap();
        tract
            .set_speaker_conditioning(Some(&conditioning(9)))
            .unwrap();
        ort.set_speaker_conditioning(Some(&conditioning(9)))
            .unwrap();
        let conditioned = snr_db(&run(&mut tract, &input), &run(&mut ort, &input));
        (neutral, conditioned)
    }

    #[test]
    fn synthetic_tract_and_onnx_runtime_agree() {
        let (neutral, conditioned) = parity(&synthetic(), 60);
        assert!(
            neutral >= 60.0 && conditioned >= 60.0,
            "{neutral} / {conditioned} dB"
        );
    }

    #[test]
    fn real_v3_full_tract_and_onnx_runtime_agree() {
        let (neutral, conditioned) = parity(&real(), 300);
        eprintln!("pro v3 full tract x ORT: neutral {neutral:.1} dB, profile {conditioned:.1} dB");
        assert!(
            neutral >= 60.0 && conditioned >= 60.0,
            "{neutral} / {conditioned} dB"
        );
    }
}
