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
