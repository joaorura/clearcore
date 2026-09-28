use std::path::PathBuf;

use realtime_noise_contracts::{CHANNELS, HOP_SAMPLES, SAMPLE_RATE_HZ};
use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, BackendDescriptor, GoldenCase, GoldenFixture, GoldenProvenance,
    InferenceError, NumericalTolerance, frames_sha256,
};
use sha2::{Digest, Sha256};

fn descriptor() -> BackendDescriptor {
    BackendDescriptor {
        backend: "tract",
        backend_version: "deep_filter-v0.5.6",
        runtime: "tract",
        runtime_version: "0.19.16",
        asset_id: "df-compatible-release-asset-v1".to_owned(),
        asset_sha256: "a".repeat(64),
        cpu_profile: "avx2-minimum",
    }
}

fn golden_fixture() -> Result<GoldenFixture, serde_json::Error> {
    let input_frames = vec![vec![0.25; HOP_SAMPLES]];
    let output_frames = vec![vec![0.5; HOP_SAMPLES]];
    let provenance = GoldenProvenance {
        schema_version: 1,
        asset_id: "df-compatible-release-asset-v1".to_owned(),
        asset_sha256: "a".repeat(64),
        candidate_record_sha256: "b".repeat(64),
        legal_review_record_sha256: "c".repeat(64),
        key_id: "sha256:key".to_owned(),
        backend: "tract".to_owned(),
        backend_version: "deep_filter-v0.5.6".to_owned(),
        runtime: "tract".to_owned(),
        runtime_version: "0.19.16".to_owned(),
        cpu_profile: "avx2-minimum".to_owned(),
        corpus_sha256: "d".repeat(64),
        generator_revision: "revision".to_owned(),
        generator_command: "generate-golden --fresh".to_owned(),
        generated_at_utc: "2026-09-28T00:00:00Z".to_owned(),
        host_os: "linux".to_owned(),
        host_cpu: "reference".to_owned(),
        isolated_offline: true,
        sample_rate_hz: SAMPLE_RATE_HZ,
        channels: CHANNELS,
        hop_samples: HOP_SAMPLES,
        algorithmic_latency_samples: ALGORITHM_LATENCY_SAMPLES,
        input_normalization: "f32-unit-range".to_owned(),
        quality_metric: "frozen-metric".to_owned(),
        quality_metric_version: "1".to_owned(),
        quality_threshold: 0.8,
        quality_observed_value: 0.9,
        tolerance: NumericalTolerance {
            absolute: 0.000_001,
            relative: 0.000_01,
        },
    };
    let provenance_sha256 = format!("{:x}", Sha256::digest(serde_json::to_vec(&provenance)?));
    Ok(GoldenFixture {
        provenance,
        provenance_sha256,
        cases: vec![GoldenCase {
            case_id: "case-a".to_owned(),
            input_sha256: frames_sha256(&input_frames),
            output_sha256: frames_sha256(&output_frames),
            frame_count: 1,
            input_frames,
            output_frames,
        }],
    })
}

#[test]
fn missing_frozen_golden_is_a_truthful_block() -> Result<(), Box<dyn std::error::Error>> {
    let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/golden/frozen-reference.json");
    let Err(error) = GoldenFixture::read(&missing) else {
        return Err("no frozen golden may be committed before qualification".into());
    };
    assert!(matches!(error, InferenceError::GoldenPending(_)));
    Ok(())
}

#[test]
fn valid_fixture_binds_descriptor_and_compares_every_sample()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture = golden_fixture()?;
    fixture.validate_for(&descriptor())?;
    fixture.compare_case("case-a", &[vec![0.5; HOP_SAMPLES]])?;
    assert!(
        fixture
            .compare_case("case-a", &[vec![0.6; HOP_SAMPLES]])
            .is_err()
    );
    Ok(())
}

#[test]
fn fixture_rejects_reordered_cases_and_checksum_drift() -> Result<(), Box<dyn std::error::Error>> {
    let mut fixture = golden_fixture()?;
    let mut second = fixture.cases[0].clone();
    second.case_id = "case-0".to_owned();
    fixture.cases.push(second);
    assert!(fixture.validate().is_err());

    let mut fixture = golden_fixture()?;
    fixture.cases[0].input_frames[0][0] = 0.75;
    assert!(fixture.validate().is_err());
    Ok(())
}

#[test]
fn fixture_rejects_descriptor_and_quality_drift() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = golden_fixture()?;
    let mut wrong_descriptor = descriptor();
    wrong_descriptor.runtime_version = "different";
    assert!(fixture.validate_for(&wrong_descriptor).is_err());

    let mut fixture = golden_fixture()?;
    fixture.provenance.quality_threshold = 0.95;
    fixture.provenance_sha256 = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&fixture.provenance)?)
    );
    assert!(fixture.validate().is_err());
    Ok(())
}
