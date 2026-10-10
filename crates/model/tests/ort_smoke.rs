#![cfg(feature = "ort")]
#![allow(clippy::expect_used)]

//! Smoke gate for the ONNX Runtime (`ort` rc.13) streaming backend.
//!
//! Fixture (R10): the git-tracked stateful DeepFilterNet3 graphs in
//! `models/stateful/` — the exact bytes production loads, each pinned by
//! `APPROVED_STATEFUL_DIGESTS` inside `OrtBackend::new`.

use std::path::PathBuf;

use realtime_noise_contracts::HOP_SAMPLES;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, CpuProfile, InferenceBackend, InferenceError,
    OrtBackend,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn open_backend() -> Result<OrtBackend, Box<dyn std::error::Error>> {
    let root = repository_root();
    let manifest = ApprovedAssetManifest::verify(&root)?;
    Ok(OrtBackend::new(
        root.join("models/stateful"),
        manifest.asset_id(),
        manifest.asset_sha256(),
        CpuProfile::Avx2Minimum,
    )?)
}

#[test]
fn stateful_ort_backend_processes_one_finite_frame() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = open_backend()?;
    assert_eq!(backend.descriptor().backend, "ort");
    assert_eq!(backend.descriptor().runtime, "onnxruntime");
    assert!(!backend.supports_voice_profile());

    let output = backend.process(&[0.0; HOP_SAMPLES])?;

    assert!(output.samples.iter().all(|sample| sample.is_finite()));
    assert_eq!(
        output.algorithmic_latency_samples,
        ALGORITHM_LATENCY_SAMPLES
    );
    assert_eq!(
        backend.algorithmic_latency_samples(),
        ALGORITHM_LATENCY_SAMPLES
    );
    Ok(())
}

#[test]
fn stateful_ort_backend_is_non_silent_and_stateful() -> Result<(), Box<dyn std::error::Error>> {
    let mut backend = open_backend()?;
    let input = std::array::from_fn(|index| {
        let index = u16::try_from(index).unwrap_or_default();
        (f32::from(index) * 0.071).sin() * 0.8
    });

    let first = backend.process(&input)?;
    let mut later = first.clone();
    for _ in 0..19 {
        later = backend.process(&input)?;
    }

    assert!(later.samples.iter().any(|sample| sample.abs() > f32::EPSILON));
    assert!(first
        .samples
        .iter()
        .zip(later.samples)
        .any(|(&first, later)| (first - later).abs() > f32::EPSILON));
    Ok(())
}

#[test]
fn stateful_ort_backend_rejects_tampered_graph() -> Result<(), Box<dyn std::error::Error>> {
    let root = repository_root();
    let manifest = ApprovedAssetManifest::verify(&root)?;
    let source = root.join("models/stateful");
    let sandbox = tempfile::tempdir()?;
    for name in ["enc.onnx", "erb_dec.onnx", "df_dec.onnx"] {
        std::fs::copy(source.join(name), sandbox.path().join(name))?;
    }
    let enc_path = sandbox.path().join("enc.onnx");
    let mut bytes = std::fs::read(&enc_path)?;
    bytes[0] ^= 0xFF;
    std::fs::write(&enc_path, bytes)?;

    let error = OrtBackend::new(
        sandbox.path(),
        manifest.asset_id(),
        manifest.asset_sha256(),
        CpuProfile::Avx2Minimum,
    )
    .expect_err("a tampered graph must fail closed before loading");
    assert!(matches!(error, InferenceError::AssetNotApproved(_)));
    Ok(())
}
