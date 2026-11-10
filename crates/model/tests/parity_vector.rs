#![cfg(feature = "tract")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]

//! Daemon × virtual-microphone parity vector for the pDFNet3 Pro v3 full.
//!
//! `write_parity_vector` (ignored; run on demand) records what the DAEMON produces — the bundled
//! archive through `PdfNet3DevArchive::instantiate` (tract, generic stateful executor) — for one
//! input and one speaker conditioning, so the filter (ONNX Runtime) can assert SNR >= 60 dB
//! against it. `check_parity_vector` re-runs the daemon and the ONNX Runtime executor against the
//! files.
//!
//! ```text
//! CLEARCORE_PARITY_DIR=<out dir> CLEARCORE_PARITY_WAV=<48 kHz mono 16-bit wav> \
//! CLEARCORE_PARITY_EMBEDDING=<json list of 192 floats> \
//! cargo test --release -p realtime-noise-model --features tract,ort --test parity_vector -- --ignored
//! ```
//!
//! Files (raw little-endian `f32`, mono 48 kHz, whole 480-sample hops, output aligned with the
//! input sample by sample — the 1440-sample algorithmic latency is inside the output):
//! `input.f32`, `expected_daemon_neutral.f32` (no profile), `expected_daemon_profile.f32`
//! (conditioning applied before the first hop), `conditioning.json`, `manifest.json`.

use std::path::{Path, PathBuf};

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::pdfnet3_bundle::{
    PDFNET3_PRO_V3_FULL_FILE, PDFNET3_PRO_V3_FULL_SHA256, load_isolation_archive_from,
};
use realtime_noise_model::{
    BandGains, CpuProfile, InferenceBackend, PdfNet3DevArchive, SpeakerConditioning,
};

/// Hop ranges set to digital zero (exercise the silence shortcut), as `parity_mixture`.
const SILENT_HOPS: [(usize, usize); 2] = [(0, 20), (300, 320)];

fn archive() -> PdfNet3DevArchive {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models");
    load_isolation_archive_from(&[root]).unwrap()
}

fn read_wav_48k_mono_i16(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    // Minimal RIFF walk: find the `data` chunk (format checked by the caller's contract).
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let id = &bytes[offset..offset + 4];
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if id == b"data" {
            return bytes[offset + 8..offset + 8 + size]
                .chunks_exact(2)
                .map(|s| f32::from(i16::from_le_bytes([s[0], s[1]])) / 32768.0)
                .collect();
        }
        offset += 8 + size + (size & 1);
    }
    panic!("no data chunk");
}

fn write_f32(path: &Path, samples: &[f32]) {
    let bytes: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    std::fs::write(path, bytes).unwrap();
}

fn read_f32(path: &Path) -> Vec<f32> {
    std::fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}

fn frames(samples: &[f32]) -> Vec<AudioFrame> {
    samples
        .chunks_exact(HOP_SAMPLES)
        .map(|hop| hop.try_into().unwrap())
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

fn eq_ramp() -> [f32; 32] {
    std::array::from_fn(|band| -3.0 + 6.0 * band as f32 / 31.0)
}

fn conditioning(embedding: &[f32]) -> SpeakerConditioning {
    let mut speaker_embedding = [0.0f32; 192];
    speaker_embedding.copy_from_slice(embedding);
    SpeakerConditioning {
        profile_id: "parity".to_owned(),
        model_id: "pdfnet3-pro-v3-full".to_owned(),
        speaker_embedding,
        eq: Some(BandGains::from_array(eq_ramp()).unwrap()),
        intensity_percent: 100,
    }
}

fn daemon_outputs(
    input: &[AudioFrame],
    conditioning: &SpeakerConditioning,
) -> (Vec<f32>, Vec<f32>) {
    let archive = archive();
    let mut neutral = archive.instantiate(CpuProfile::Avx2Minimum).unwrap();
    let mut profiled = archive.instantiate(CpuProfile::Avx2Minimum).unwrap();
    profiled
        .set_speaker_conditioning(Some(conditioning))
        .unwrap();
    (run(&mut neutral, input), run(&mut profiled, input))
}

fn dir() -> PathBuf {
    PathBuf::from(std::env::var_os("CLEARCORE_PARITY_DIR").expect("CLEARCORE_PARITY_DIR"))
}

#[test]
#[ignore = "writes the parity vector; needs CLEARCORE_PARITY_DIR/_WAV/_EMBEDDING"]
fn write_parity_vector() {
    let out = dir();
    std::fs::create_dir_all(&out).unwrap();
    let wav =
        PathBuf::from(std::env::var_os("CLEARCORE_PARITY_WAV").expect("CLEARCORE_PARITY_WAV"));
    let embedding: Vec<f32> = serde_json::from_slice(
        &std::fs::read(std::env::var_os("CLEARCORE_PARITY_EMBEDDING").expect("EMBEDDING")).unwrap(),
    )
    .unwrap();
    let mut samples = read_wav_48k_mono_i16(&wav);
    samples.truncate(samples.len() / HOP_SAMPLES * HOP_SAMPLES);
    for (start, end) in SILENT_HOPS {
        let end = (end * HOP_SAMPLES).min(samples.len());
        samples[(start * HOP_SAMPLES).min(end)..end].fill(0.0);
    }
    let conditioning = conditioning(&embedding);
    let (neutral, profiled) = daemon_outputs(&frames(&samples), &conditioning);
    write_f32(&out.join("input.f32"), &samples);
    write_f32(&out.join("expected_daemon_neutral.f32"), &neutral);
    write_f32(&out.join("expected_daemon_profile.f32"), &profiled);
    let conditioning_json = serde_json::json!({
        "profile_id": conditioning.profile_id,
        "model_id": conditioning.model_id,
        "speaker_embedding": embedding,
        "eq_gains_db": eq_ramp(),
        "intensity_percent": conditioning.intensity_percent,
        "speaker_present": conditioning.speaker_present(),
    });
    std::fs::write(
        out.join("conditioning.json"),
        serde_json::to_vec_pretty(&conditioning_json).unwrap(),
    )
    .unwrap();
    let manifest = serde_json::json!({
        "producer": "daemon: realtime-noise-model PdfNet3DevArchive::instantiate (tract 0.19.16, generic stateful executor)",
        "asset": PDFNET3_PRO_V3_FULL_FILE,
        "asset_sha256": PDFNET3_PRO_V3_FULL_SHA256,
        "source_wav": wav.file_name().map(|n| n.to_string_lossy().into_owned()),
        "embedding_source": "enrollment-v3 enrollment.onnx (sha256 82a292b9...) on the CLEAN twin of the wav, 16 kHz",
        "sample_rate": 48000,
        "hop": HOP_SAMPLES,
        "hops": samples.len() / HOP_SAMPLES,
        "format": "raw little-endian f32 mono; output aligned with input (1440-sample latency inside)",
        "silent_hops": SILENT_HOPS,
        "pipeline": {
            "dsp": "DspPipeline::default (fft 960, hop 480, 32 ERB, 96 DF bins, order 5, lookahead 2, alpha 0.99, post-filter off, no attenuation limit)",
            "model": "enc, erb_dec and df_dec on EVERY hop; every state advances every hop",
            "stage_selection": "lsnr < -10 -> zero; lsnr > 30 -> unprocessed; 20 < lsnr <= 30 -> ERB only; else ERB+DF (emulate_gating_offline, runtime_path.py:342-360)",
            "digital_silence": "hop RMS < 1e-7 -> zero output, neither STFT nor model advance",
            "eq": "profile EQ (32 ERB gains dB -> per-bin factors) applied right before the iSTFT",
            "conditioning": "speaker_embedding = embedding, speaker_present binary: 0.0 at intensity 0, 1.0 otherwise (here 1.0); neutral: zeros and 0.0",
        },
        "acceptance": "SNR(expected, filter output) >= 60 dB over the whole file, for both outputs",
    });
    std::fs::write(
        out.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    eprintln!(
        "wrote {} hops to {}",
        samples.len() / HOP_SAMPLES,
        out.display()
    );
}

#[test]
#[ignore = "checks a written parity vector; needs CLEARCORE_PARITY_DIR"]
fn check_parity_vector() {
    let out = dir();
    let input = read_f32(&out.join("input.f32"));
    let neutral = read_f32(&out.join("expected_daemon_neutral.f32"));
    let profiled = read_f32(&out.join("expected_daemon_profile.f32"));
    let json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("conditioning.json")).unwrap()).unwrap();
    let embedding: Vec<f32> = json["speaker_embedding"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap() as f32)
        .collect();
    let conditioning = conditioning(&embedding);
    let hops = frames(&input);
    let (daemon_neutral, daemon_profile) = daemon_outputs(&hops, &conditioning);
    let replay = (
        snr_db(&neutral, &daemon_neutral),
        snr_db(&profiled, &daemon_profile),
    );
    eprintln!(
        "daemon replay: neutral {:.1} dB, profile {:.1} dB",
        replay.0, replay.1
    );
    assert!(replay.0 >= 100.0 && replay.1 >= 100.0);
    #[cfg(feature = "ort")]
    {
        let archive = archive();
        let mut ort_neutral = archive
            .instantiate_stateful_ort(CpuProfile::Avx2Minimum)
            .unwrap();
        let mut ort_profile = archive
            .instantiate_stateful_ort(CpuProfile::Avx2Minimum)
            .unwrap();
        ort_profile
            .set_speaker_conditioning(Some(&conditioning))
            .unwrap();
        let ort = (
            snr_db(&neutral, &run(&mut ort_neutral, &hops)),
            snr_db(&profiled, &run(&mut ort_profile, &hops)),
        );
        eprintln!(
            "ONNX Runtime executor vs daemon: neutral {:.1} dB, profile {:.1} dB",
            ort.0, ort.1
        );
        assert!(ort.0 >= 60.0 && ort.1 >= 60.0);
    }
    // The profile must actually change the output.
    assert!(snr_db(&neutral, &profiled) < 40.0);
}
