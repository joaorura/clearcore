#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]

//! The generic stateful executor on `OpenVINO` (CPU, GPU, NPU when listed) against tract, for the
//! synthetic Pro v3 full stand-in and the bundled pDFNet3 Pro v3 full. Skipped when no
//! `libopenvino_c` is found (`OPENVINO_LIB_PATH` points at the pip runtime in CI/dev).

use std::path::PathBuf;

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::pdfnet3_bundle::load_isolation_archive_from;
use realtime_noise_model::stateful_graph::{StatefulGraphBackend, build_executor};
use realtime_noise_model::tract_graph::TractGraphSession;
use realtime_noise_model::{
    BandGains, CpuProfile, InferenceBackend, PdfNet3DevArchive, SpeakerConditioning,
};
use realtime_noise_runtime_openvino::{build_openvino_executor, is_openvino_available};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn synthetic() -> PdfNet3DevArchive {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(
        root().join("crates/model/tests/fixtures/pdfnet3-pro-v3-full-synthetic.tar.gz"),
    )
    .unwrap();
    let sha: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    PdfNet3DevArchive::verify(bytes, &sha).unwrap()
}

fn real() -> PdfNet3DevArchive {
    load_isolation_archive_from(&[root().join("models")]).unwrap()
}

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
    values.iter_mut().for_each(|v| *v /= norm);
    values
}

fn conditioning() -> SpeakerConditioning {
    let gains: [f32; 32] = std::array::from_fn(|band| -3.0 + 6.0 * band as f32 / 31.0);
    let mut speaker_embedding = [0.0f32; 192];
    speaker_embedding.copy_from_slice(&embedding(9));
    SpeakerConditioning {
        profile_id: "id".to_owned(),
        model_id: "pdfnet3-pro-v3-full".to_owned(),
        speaker_embedding,
        eq: Some(BandGains::from_array(gains).unwrap()),
        intensity_percent: 100,
    }
}

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

fn devices() -> Vec<String> {
    realtime_noise_runtime_openvino::available_devices()
        .into_iter()
        .filter(|d| ["CPU", "GPU", "NPU"].contains(&d.as_str()))
        .collect()
}

/// (device, neutral SNR, profile SNR, p50 ms, max ms) of OpenVINO against tract.
fn compare(archive: &PdfNet3DevArchive, frames: usize) -> Vec<(String, f64, f64, f64, f64)> {
    let files = archive.stateful_files().unwrap();
    let input = signal(frames);
    let descriptor = |runtime| archive.descriptor("x", runtime, "x", CpuProfile::Avx2Minimum);
    let mut tract = StatefulGraphBackend::new(
        build_executor(&files, TractGraphSession::load).unwrap(),
        descriptor("tract"),
    )
    .unwrap();
    let reference_neutral = run(&mut tract, &input);
    tract.reset().unwrap();
    tract
        .set_speaker_conditioning(Some(&conditioning()))
        .unwrap();
    let reference_profile = run(&mut tract, &input);
    let mut results = Vec::new();
    for device in devices() {
        let mut ov = StatefulGraphBackend::new(
            build_openvino_executor(&files, &device).unwrap(),
            descriptor("openvino"),
        )
        .unwrap();
        assert!(InferenceBackend::supports_voice_profile(&ov));
        let mut times = Vec::new();
        let neutral: Vec<f32> = input
            .iter()
            .flat_map(|frame| {
                let start = std::time::Instant::now();
                let out = ov.process(frame).unwrap().samples;
                times.push(start.elapsed().as_secs_f64() * 1e3);
                out
            })
            .collect();
        ov.reset().unwrap();
        ov.set_speaker_conditioning(Some(&conditioning())).unwrap();
        let conditioned = run(&mut ov, &input);
        times.sort_by(f64::total_cmp);
        results.push((
            device,
            snr_db(&reference_neutral, &neutral),
            snr_db(&reference_profile, &conditioned),
            times[times.len() / 2],
            times[times.len() - 1],
        ));
    }
    results
}

fn check(name: &str, results: &[(String, f64, f64, f64, f64)]) {
    for (device, neutral, conditioned, p50, max) in results {
        eprintln!(
            "{name} OpenVINO {device} x tract: neutral {neutral:.1} dB, profile {conditioned:.1} dB, \
             hop p50 {p50:.2} ms max {max:.2} ms"
        );
        // FP32 on the CPU matches tract; FP16 (GPU/NPU) is bounded by half precision.
        let floor = if device == "CPU" { 60.0 } else { 20.0 };
        assert!(
            *neutral >= floor && *conditioned >= floor,
            "{device}: {neutral} / {conditioned}"
        );
    }
}

#[test]
fn synthetic_on_openvino_matches_tract() {
    if !is_openvino_available() {
        eprintln!("OpenVINO not found; skipping");
        return;
    }
    check("synthetic", &compare(&synthetic(), 60));
}

#[test]
fn pro_v3_full_on_openvino_matches_tract() {
    if !is_openvino_available() {
        eprintln!("OpenVINO not found; skipping");
        return;
    }
    check("pro-v3-full", &compare(&real(), 300));
}
