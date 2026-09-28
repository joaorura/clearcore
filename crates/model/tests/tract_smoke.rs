#![cfg(feature = "tract")]

use std::path::PathBuf;

use realtime_noise_contracts::HOP_SAMPLES;
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, CpuProfile, InferenceBackend, TractBackend,
};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn approved_tract_backend_processes_one_finite_frame() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = ApprovedAssetManifest::verify(&repository_root())?;
    let mut backend = TractBackend::new(&manifest, CpuProfile::Avx2Minimum)?;
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
fn approved_tract_backend_is_non_silent_and_stateful() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = ApprovedAssetManifest::verify(&repository_root())?;
    let mut backend = TractBackend::new(&manifest, CpuProfile::Avx2Minimum)?;
    let input = std::array::from_fn(|index| {
        let index = u16::try_from(index).unwrap_or_default();
        (f32::from(index) * 0.071).sin() * 0.8
    });

    let first = backend.process(&input)?;
    let mut later = first.clone();
    for _ in 0..19 {
        later = backend.process(&input)?;
    }

    assert!(
        later
            .samples
            .iter()
            .any(|sample| sample.abs() > f32::EPSILON)
    );
    assert!(
        first
            .samples
            .iter()
            .zip(later.samples)
            .any(|(&first, later)| (first - later).abs() > f32::EPSILON)
    );
    Ok(())
}
