//! Does the app's `models/enrollment/enrollment.onnx` (enrollment v3, ECAPA) live in the same
//! embedding space as the `speaker_embedding.f32` of the v7 vectors?
//!
//! The vector embedding is NOT computed from `enroll.wav`: `scripts/export_v7.py` takes the
//! per-speaker embedding of the training bank (`bank.emb[bank.idx(spk)]`, other utterances of the
//! same speaker). So an exact match is impossible by construction; what this checks is that the
//! app's model, on the vector's enrollment audio, lands next to the bank vector of the same
//! speaker (cosine 0.80-0.85 measured on 2026-11-15; a different ECAPA would be uncorrelated).
//! Exact identity needs the training encoder run on `enroll.wav` (open item of the hand-off).

#![cfg(feature = "tract")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use std::path::{Path, PathBuf};

use realtime_noise_model::enrollment_v3::{EmbeddingExtractor, OnnxEmbeddingModel};
use realtime_noise_model::resample::decimate_48k_to_16k;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_f32(path: &Path) -> Vec<f32> {
    std::fs::read(path)
        .unwrap()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

fn read_wav_f32(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        if &bytes[at..at + 4] == b"data" {
            return bytes[at + 8..at + 8 + size]
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
        }
        at += 8 + size + (size & 1);
    }
    panic!("no data");
}

fn normalize(v: &[f32]) -> Vec<f32> {
    let n: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    v.iter().map(|x| x / n).collect()
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let a = normalize(a);
    let b = normalize(b);
    a.iter().zip(&b).map(|(x, y)| x * y).sum()
}

/// The v7 FIR v1 (241 taps) as a 48 -> 16 kHz decimator: `y[n] = sum h[k] x[3n + 120 - k]`.
fn fir_decimate(x: &[f32], h: &[f32]) -> Vec<f32> {
    (0..x.len() / 3)
        .map(|n| {
            let mut y = 0.0f32;
            for (k, hk) in h.iter().enumerate() {
                let i = (3 * n + 120) as isize - k as isize;
                if i >= 0 && (i as usize) < x.len() {
                    y += hk * x[i as usize];
                }
            }
            y
        })
        .collect()
}

#[test]
fn app_enrollment_model_vs_vector_embedding() {
    let model_path = root().join("models/enrollment/enrollment.onnx");
    let mut model = OnnxEmbeddingModel::from_file(&model_path).unwrap();
    let asset = std::fs::read(
        root().join("models/pdfnet3/pdfnet3-pro-v7-a2-300k-stateful-asset-v1.tar.gz"),
    )
    .unwrap();
    let fir = realtime_noise_model::pdfnet3_v7::V7Asset::from_archive_bytes(&asset)
        .unwrap()
        .fir;
    let mut worst = f32::INFINITY;
    for case in ["alvo_interf_sir0_snr10", "alvo_snr0", "so_interf_snr20"] {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/pdfnet3_v7")
            .join(case);
        let expected = read_f32(&dir.join("speaker_embedding.f32"));
        let audio = read_wav_f32(&dir.join("enroll.wav"));
        for (label, mut a16) in [
            ("app decimator", decimate_48k_to_16k(&audio)),
            ("v7 FIR v1", fir_decimate(&audio, &fir)),
        ] {
            a16.truncate(192_000);
            let got = normalize(&model.extract_embedding(&a16).unwrap());
            let cos = cosine(&got, &expected);
            let max = got
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            println!(
                "{case} [{label}] 16k={} cos={cos:.6} max|d|={max:.3e}",
                a16.len()
            );
            if label == "app decimator" {
                worst = worst.min(cos);
            }
        }
    }
    println!("worst cosine (app path) = {worst:.6}");
    assert!(
        worst >= 0.75,
        "app ECAPA is far from the bank embedding: {worst}"
    );
}
