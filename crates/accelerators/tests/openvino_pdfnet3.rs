#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::suboptimal_flops
)]

//! pDFNet3 Pro on OpenVINO (bundled stateful export) against the tract reference (bundled tract
//! export), with and without a voice profile.
//!
//! The always-on tests only need the repository archives and skip when OpenVINO cannot be
//! loaded. The real parity/latency run is `#[ignore]`d and enabled with:
//!
//! ```text
//! CLEARCORE_OV_PARITY=1 \
//! OPENVINO_LIB_PATH=<dir with libopenvino_c.so.*> \
//! CLEARCORE_PARITY_WAV=<16-bit mono 48 kHz speech> \
//! [CLEARCORE_OV_PARITY_DEVICES=CPU,GPU,NPU] [CLEARCORE_OV_PARITY_FRAMES=1000] \
//! cargo test --release -p realtime-noise-accelerators --test openvino_pdfnet3 -- --ignored --nocapture
//! ```

use std::path::PathBuf;
use std::time::Instant;

use realtime_noise_accelerators::OpenVINOBackend;
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::enrollment::SpeakerEmbeddingModel;
use realtime_noise_model::pdfnet3_bundle::{load_stateful_archive_from, load_tract_archive_from};
use realtime_noise_model::{
    BandGains, CpuProfile, FILM_HIDDEN_DIM, FiLMVectors, InferenceBackend, InferenceError,
    NUM_ERB_BANDS, PDFNET3_PRO_ASSET_ID, VoiceProfile,
};

fn repo_models() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
}

fn openvino_or_skip() -> bool {
    if OpenVINOBackend::is_available() {
        true
    } else {
        eprintln!("OpenVINO runtime not loadable; skipping");
        false
    }
}

fn conditioned_profile() -> VoiceProfile {
    let wave = |i: usize, scale: f32| scale * ((i as f32) * 0.37).sin();
    let film = FiLMVectors::new(
        (0..FILM_HIDDEN_DIM).map(|i| 1.0 + wave(i, 0.15)).collect(),
        (0..FILM_HIDDEN_DIM).map(|i| wave(i, 0.05)).collect(),
        (0..FILM_HIDDEN_DIM).map(|i| 1.0 - wave(i, 0.1)).collect(),
        (0..FILM_HIDDEN_DIM).map(|i| wave(i + 7, 0.04)).collect(),
    )
    .unwrap();
    let eq = BandGains::from_array(std::array::from_fn(|band| {
        -3.0 + 6.0 * band as f32 / (NUM_ERB_BANDS - 1) as f32
    }))
    .unwrap();
    VoiceProfile::new(
        "ov-test",
        "OpenVINO test",
        "2026-11-09T12:00:00Z",
        film,
        Some(eq),
    )
    .unwrap()
}

#[test]
fn pdfnet3_pro_compiles_on_the_openvino_cpu_with_film_and_applies_profiles() {
    if !openvino_or_skip() {
        return;
    }
    let archive = load_stateful_archive_from(&[repo_models()]).unwrap();
    let mut backend = OpenVINOBackend::load_pdfnet3(&archive, "CPU").unwrap();
    assert!(backend.executes_inference());
    assert!(backend.supports_speaker_conditioning());
    assert!(InferenceBackend::supports_voice_profile(&backend));
    let descriptor = backend.descriptor();
    assert_eq!(descriptor.backend, "openvino");
    assert_eq!(descriptor.runtime, "openvino-cpu");
    assert_eq!(descriptor.asset_id, PDFNET3_PRO_ASSET_ID);
    assert_eq!(descriptor.asset_sha256, archive.sha256());

    let frame: AudioFrame = std::array::from_fn(|i| 0.1 * ((i as f32) * 0.05).sin());
    for _ in 0..5 {
        assert!(
            backend
                .process(&frame)
                .unwrap()
                .samples
                .iter()
                .all(|s| s.is_finite())
        );
    }
    let profile = conditioned_profile();
    backend.set_voice_profile(Some(&profile)).unwrap();
    assert_eq!(backend.active_voice_profile(), Some(&profile));
    assert!(backend.process(&frame).is_ok());
    backend.set_voice_profile(None).unwrap();
    assert_eq!(backend.active_voice_profile(), None);
}

#[test]
fn the_identity_profile_is_the_unconditioned_network() {
    if !openvino_or_skip() {
        return;
    }
    let archive = load_stateful_archive_from(&[repo_models()]).unwrap();
    let signal: Vec<AudioFrame> = (0..60)
        .map(|f| std::array::from_fn(|i| 0.2 * (((f * HOP_SAMPLES + i) as f32) * 0.031).sin()))
        .collect();
    let mut plain = OpenVINOBackend::load_pdfnet3(&archive, "CPU").unwrap();
    let mut identity = OpenVINOBackend::load_pdfnet3(&archive, "CPU").unwrap();
    let neutral = VoiceProfile::identity("n", "Neutral", "2026-11-09T12:00:00Z").unwrap();
    identity.set_voice_profile(Some(&neutral)).unwrap();
    for frame in &signal {
        assert_eq!(
            plain.process(frame).unwrap().samples,
            identity.process(frame).unwrap().samples
        );
    }
}

#[test]
fn an_invalid_profile_leaves_the_previous_one_active() {
    if !openvino_or_skip() {
        return;
    }
    let archive = load_stateful_archive_from(&[repo_models()]).unwrap();
    let mut backend = OpenVINOBackend::load_pdfnet3(&archive, "CPU").unwrap();
    let good = conditioned_profile();
    backend.set_voice_profile(Some(&good)).unwrap();
    let mut tampered = good.clone();
    tampered.film.gamma_enc[0] = 1.5;
    assert!(backend.set_voice_profile(Some(&tampered)).is_err());
    assert_eq!(backend.active_voice_profile(), Some(&good));
}

#[test]
fn the_base_graphs_refuse_a_conditioned_profile() {
    if !openvino_or_skip() {
        return;
    }
    let stateful = repo_models().join("stateful");
    if !stateful.join("enc.onnx").exists() {
        return;
    }
    let mut backend = OpenVINOBackend::load_stateful(&stateful, "CPU", "dfn3", "x").unwrap();
    assert!(!backend.supports_speaker_conditioning());
    assert!(!InferenceBackend::supports_voice_profile(&backend));
    assert!(matches!(
        backend.set_voice_profile(Some(&conditioned_profile())),
        Err(InferenceError::InputContract(_))
    ));
    let neutral = VoiceProfile::identity("n", "Neutral", "2026-11-09T12:00:00Z").unwrap();
    assert!(backend.set_voice_profile(Some(&neutral)).is_ok());
}

/// Reads a 16-bit PCM mono 48 kHz WAV file (data chunk only).
fn read_pcm16_wav(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if &bytes[offset..offset + 4] == b"data" {
            let end = (offset + 8 + size).min(bytes.len());
            return bytes[offset + 8..end]
                .chunks_exact(2)
                .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
                .collect();
        }
        offset += 8 + size;
    }
    panic!("{path}: no data chunk");
}

struct Run {
    output: Vec<f32>,
    per_frame_ms: Vec<f64>,
}

fn run(backend: &mut dyn InferenceBackend, signal: &[AudioFrame]) -> Run {
    let mut output = Vec::with_capacity(signal.len() * HOP_SAMPLES);
    let mut per_frame_ms = Vec::with_capacity(signal.len());
    for frame in signal {
        let start = Instant::now();
        let processed = backend.process(frame).unwrap();
        per_frame_ms.push(start.elapsed().as_secs_f64() * 1e3);
        output.extend_from_slice(&processed.samples);
    }
    Run {
        output,
        per_frame_ms,
    }
}

fn percentile(values: &[f64], p: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * p).round() as usize]
}

/// `(max |a - b|, SNR of b against a in dB)` after `skip` warm-up samples.
fn compare(reference: &[f32], candidate: &[f32], skip: usize) -> (f64, f64) {
    let (mut max_err, mut signal, mut noise) = (0.0f64, 0.0f64, 0.0f64);
    for (a, b) in reference[skip..].iter().zip(&candidate[skip..]) {
        let (a, b) = (f64::from(*a), f64::from(*b));
        max_err = max_err.max((a - b).abs());
        signal += a * a;
        noise += (a - b) * (a - b);
    }
    (max_err, 10.0 * (signal / noise.max(1e-30)).log10())
}

/// A profile from the bundled enrollment model on the first seconds of the recording, so the
/// conditioned comparison uses realistic `FiLM` vectors; the synthetic profile otherwise.
fn enrolled_profile(speech_48k: &[f32]) -> VoiceProfile {
    let model_path = repo_models().join("enrollment/enrollment.onnx");
    let take = speech_48k.len().min(48_000 * 8);
    let enrolled = realtime_noise_model::enrollment::OnnxEnrollmentModel::from_file(&model_path)
        .and_then(|mut model| {
            model.extract(&realtime_noise_model::resample::decimate_48k_to_16k(
                &speech_48k[..take],
            ))
        });
    let eq = conditioned_profile().eq;
    match enrolled
        .map(|raw| FiLMVectors::new(raw.gamma_enc, raw.beta_enc, raw.gamma_df, raw.beta_df))
    {
        Ok(Ok(film)) => {
            eprintln!("profile: FiLM from models/enrollment/enrollment.onnx + EQ ramp -3..+3 dB");
            VoiceProfile::new("enrolled", "Enrolled", "2026-11-09T12:00:00Z", film, eq).unwrap()
        }
        other => {
            eprintln!(
                "profile: synthetic FiLM (enrollment unavailable: {:?})",
                other.err()
            );
            conditioned_profile()
        }
    }
}

#[test]
#[ignore = "needs OpenVINO, a speech WAV and CLEARCORE_OV_PARITY=1"]
fn parity_and_latency_against_tract_on_every_device() {
    if std::env::var("CLEARCORE_OV_PARITY").as_deref() != Ok("1") {
        eprintln!("CLEARCORE_OV_PARITY != 1; skipping");
        return;
    }
    let wav = std::env::var("CLEARCORE_PARITY_WAV").expect("CLEARCORE_PARITY_WAV");
    let frames: usize = std::env::var("CLEARCORE_OV_PARITY_FRAMES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000);
    let devices: Vec<String> = std::env::var("CLEARCORE_OV_PARITY_DEVICES")
        .unwrap_or_else(|_| "CPU,GPU,NPU".to_owned())
        .split(',')
        .map(|device| device.trim().to_owned())
        .filter(|device| !device.is_empty())
        .collect();
    let probe = realtime_noise_runtime_openvino::probe_runtime();
    eprintln!(
        "OpenVINO {} from {:?} ({}), devices {:?}",
        probe.version.as_deref().unwrap_or("?"),
        probe.library,
        probe.state.code(),
        probe.devices
    );

    let speech = read_pcm16_wav(&wav);
    let signal: Vec<AudioFrame> = speech
        .chunks_exact(HOP_SAMPLES)
        .take(frames)
        .map(|chunk| std::array::from_fn(|i| chunk[i]))
        .collect();
    assert!(signal.len() >= 200, "recording too short");
    eprintln!("signal: {} hops of {wav}", signal.len());
    let profile = if std::env::var("CLEARCORE_OV_PARITY_PROFILE").as_deref() == Ok("synthetic") {
        eprintln!("profile: synthetic FiLM (gamma 1 +- 0.15, beta +- 0.05) + EQ ramp -3..+3 dB");
        conditioned_profile()
    } else {
        enrolled_profile(&speech)
    };
    let tract_archive = load_tract_archive_from(&[repo_models()]).unwrap();
    let stateful = load_stateful_archive_from(&[repo_models()]).unwrap();
    let skip = 50 * HOP_SAMPLES;

    let film_only = VoiceProfile::new(
        "film",
        "FiLM only",
        "2026-11-09T12:00:00Z",
        profile.film.clone(),
        None,
    )
    .unwrap();
    let eq_only = VoiceProfile::new(
        "eq",
        "EQ only",
        "2026-11-09T12:00:00Z",
        FiLMVectors::identity(),
        profile.eq.clone(),
    )
    .unwrap();
    let cases: [(&str, Option<&VoiceProfile>); 4] = [
        ("sem perfil", None),
        ("perfil só FiLM", Some(&film_only)),
        ("perfil só EQ", Some(&eq_only)),
        ("perfil FiLM+EQ", Some(&profile)),
    ];
    for (label, conditioned) in cases {
        let mut tract = tract_archive.instantiate(CpuProfile::Avx2Minimum).unwrap();
        if let Some(profile) = conditioned {
            tract.set_voice_profile(Some(profile)).unwrap();
        }
        let reference = run(&mut tract, &signal);
        eprintln!(
            "[{label}] tract: p50 {:.3} ms p99 {:.3} ms max {:.3} ms",
            percentile(&reference.per_frame_ms, 0.5),
            percentile(&reference.per_frame_ms, 0.99),
            percentile(&reference.per_frame_ms, 1.0)
        );
        // ONNX Runtime on the same stateful export: what the virtual-microphone filter runs.
        let mut ort = stateful.instantiate_ort(CpuProfile::Avx2Minimum).unwrap();
        if let Some(profile) = conditioned {
            ort.set_voice_profile(Some(profile)).unwrap();
        }
        let ort_run = run(&mut ort, &signal);
        let (ort_max, ort_snr) = compare(&reference.output, &ort_run.output, skip);
        eprintln!(
            "[{label}] ort (stateful, CPU) vs tract: max|err| {ort_max:.5}, SNR {ort_snr:.1} dB, p50 {:.3} ms",
            percentile(&ort_run.per_frame_ms, 0.5)
        );
        for device in &devices {
            let started = Instant::now();
            let mut backend = match OpenVINOBackend::load_pdfnet3(&stateful, device) {
                Ok(backend) => backend,
                Err(error) => {
                    eprintln!("[{label}] {device}: load failed: {error}");
                    continue;
                }
            };
            let load_ms = started.elapsed().as_secs_f64() * 1e3;
            if let Some(profile) = conditioned {
                backend.set_voice_profile(Some(profile)).unwrap();
            }
            let candidate = run(&mut backend, &signal);
            let (max_err, snr) = compare(&reference.output, &candidate.output, skip);
            eprintln!(
                "[{label}] {device} ({}): load {load_ms:.0} ms, max|err| {max_err:.5}, SNR {snr:.1} dB, \
                 p50 {:.3} ms p99 {:.3} ms max {:.3} ms",
                backend.precision().as_str(),
                percentile(&candidate.per_frame_ms, 0.5),
                percentile(&candidate.per_frame_ms, 0.99),
                percentile(&candidate.per_frame_ms, 1.0)
            );
            let (ort_max_err, ort_snr) = compare(&ort_run.output, &candidate.output, skip);
            eprintln!(
                "[{label}] {device} vs ort (same stateful export): max|err| {ort_max_err:.5}, SNR {ort_snr:.1} dB"
            );
            assert!(candidate.output.iter().all(|s| s.is_finite()));
            if std::env::var("CLEARCORE_OV_PARITY_STRICT").as_deref() == Ok("0") {
                continue;
            }
            // Gates (loose floors; the measured values are printed above):
            // - against ONNX Runtime on the same stateful export, wherever both apply the same
            //   conditioning (ORT ignores the profile EQ): catches any feeding/ordering bug;
            // - against tract where the two exports agree (no FiLM): catches DSP/EQ bugs.
            // A one-hop misalignment already drops to about -3 dB on speech.
            let fp16 = backend.precision().as_str() == "fp16";
            let floor = if fp16 { 20.0 } else { 40.0 };
            if conditioned.is_none_or(|p| p.eq.is_none()) {
                assert!(ort_snr > floor, "{device} {label} vs ort: {ort_snr:.1} dB");
            }
            if conditioned.is_none_or(|p| p.film.is_identity()) {
                assert!(snr > 20.0, "{device} {label} vs tract: {snr:.1} dB");
            }
        }
    }
}
