#![allow(warnings)]
#![allow(clippy::unwrap_used, clippy::manual_string_new)]

use realtime_noise_tools::golden_manifest::{
    CorpusCase, Manifest, ManifestError, ManifestV1, blocked_status, validate_corpus,
    validate_input_case,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::path::PathBuf;

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_manifest() -> ManifestV1 {
    let cases = vec![valid_case()];
    ManifestV1 {
        schema_version: 1,
        status: "APPROVED".to_owned(),
        corpus_sha256: sha256(&serde_json::to_vec(&cases).unwrap()),
        input_normalization: "peak".to_owned(),
        quality_metric: "PESQ".to_owned(),
        quality_metric_version: "1.0".to_owned(),
        quality_threshold: 2.5,
        quality_observed_value: 3.0,
        tolerance: realtime_noise_model::NumericalTolerance {
            absolute: 0.0,
            relative: 0.0,
        },
        cases,
    }
}

fn valid_case() -> CorpusCase {
    CorpusCase {
        case_id: "case-001".to_owned(),
        frames_path: PathBuf::from("frames/case-001.json"),
        input_sha256: "0".repeat(64),
        license: "CC0-1.0".to_owned(),
        redistribution_terms: "public-domain".to_owned(),
        authorization: "explicit".to_owned(),
        transcription_applicability: "none".to_owned(),
        snr_noise_class: "stationary".to_owned(),
        sample_rate_hz: 48000,
        channels: 1,
        frame_count: 2,
    }
}

fn valid_frames(case: &CorpusCase) -> Vec<Vec<f32>> {
    vec![vec![0.0; 480]; case.frame_count]
}

#[test]
fn valid_10_field_manifest_parses() {
    let manifest = valid_manifest();
    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: ManifestV1 = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.schema_version, 1);
    assert_eq!(parsed.status, "APPROVED");
}

#[test]
fn unknown_field_in_manifest_is_rejected() {
    let mut manifest = valid_manifest();
    let mut json = serde_json::to_value(&manifest).unwrap();
    json["unknown_field"] = json!(true);
    let result: Result<ManifestV1, _> = serde_json::from_value(json);
    assert!(result.is_err());
}

#[test]
fn valid_11_field_case_parses() {
    let case = valid_case();
    let json = serde_json::to_string(&case).unwrap();
    let parsed: CorpusCase = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.case_id, "case-001");
}

#[test]
fn validate_corpus_rejects_schema_version_not_1() {
    let mut manifest = valid_manifest();
    manifest.schema_version = 2;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_non_approved_status() {
    let mut manifest = valid_manifest();
    manifest.status = "PENDING".to_owned();
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_empty_cases() {
    let mut manifest = valid_manifest();
    manifest.cases.clear();
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_non_finite_quality_threshold() {
    let mut manifest = valid_manifest();
    manifest.quality_threshold = f64::NAN;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_non_finite_quality_observed_value() {
    let mut manifest = valid_manifest();
    manifest.quality_observed_value = f64::INFINITY;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_quality_observed_below_threshold() {
    let mut manifest = valid_manifest();
    manifest.quality_observed_value = 2.0;
    manifest.quality_threshold = 2.5;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_duplicate_case_id() {
    let mut manifest = valid_manifest();
    let case = valid_case();
    manifest.cases.push(case);
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_empty_case_id() {
    let mut manifest = valid_manifest();
    manifest.cases[0].case_id = "".to_owned();
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_digest_mismatch() {
    let mut manifest = valid_manifest();
    manifest.corpus_sha256 = "0".repeat(64);
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_absolute_frames_path() {
    let mut manifest = valid_manifest();
    manifest.cases[0].frames_path = PathBuf::from("/absolute/path.json");
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_parent_dir_in_frames_path() {
    let mut manifest = valid_manifest();
    manifest.cases[0].frames_path = PathBuf::from("../escape.json");
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_rejects_non_finite_provenance_fields() {
    let mut manifest = valid_manifest();
    manifest.cases[0].license = "".to_owned();
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &Manifest::V1(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_input_case_rejects_wrong_frame_count() {
    let case = valid_case();
    let frames = vec![vec![0.0; 480]; 3];
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_input_case_rejects_wrong_frame_length() {
    let case = valid_case();
    let frames = vec![vec![0.0; 479]; 2];
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_input_case_rejects_non_finite_frames() {
    let case = valid_case();
    let mut frames = valid_frames(&case);
    frames[0][0] = f32::NAN;
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_input_case_rejects_frames_sha256_mismatch() {
    let case = valid_case();
    let frames = valid_frames(&case);
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn validate_corpus_v2_rejects_missing_source_lock() {
    let source = realtime_noise_tools::golden_manifest::SourceRecord {
        source_id: "src-001".to_owned(),
        origin: "https://example.com/asset.wav".to_owned(),
        revision: "v1.0".to_owned(),
        source_sha256: "0".repeat(64),
        license: "CC0-1.0".to_owned(),
        redistribution_terms: "public-domain".to_owned(),
        processing_authorization: "explicit".to_owned(),
        attribution: "Test Author".to_owned(),
    };
    let case = realtime_noise_tools::golden_manifest::CorpusCaseV2 {
        case_id: "case-001".to_owned(),
        frames_path: PathBuf::from("frames/case-001.json"),
        input_sha256: "0".repeat(64),
        source_id: "src-001".to_owned(),
        transcription_applicability: "none".to_owned(),
        snr_noise_class: "stationary".to_owned(),
        sample_rate_hz: 48000,
        channels: 1,
        frame_count: 2,
    };
    let manifest = realtime_noise_tools::golden_manifest::ManifestV2 {
        schema_version: 2,
        status: "APPROVED".to_owned(),
        corpus_sha256: "0".repeat(64),
        input_normalization: "peak".to_owned(),
        quality_metric: "PESQ".to_owned(),
        quality_metric_version: "1.0".to_owned(),
        quality_threshold: 2.5,
        quality_observed_value: 3.0,
        tolerance: realtime_noise_model::NumericalTolerance { absolute: 0.0, relative: 0.0 },
        source_lock_sha256: "0".repeat(64),
        sources: vec![source],
        cases: vec![case],
    };
    let temp_root = env::temp_dir().join(format!(
        "hippocamp-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&temp_root).unwrap();
    let err = validate_corpus(&temp_root, &Manifest::V2(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_reason(&err),
        "source-lock.json missing"
    );
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn validate_corpus_v2_rejects_source_lock_mismatch() {
    let source = realtime_noise_tools::golden_manifest::SourceRecord {
        source_id: "src-001".to_owned(),
        origin: "https://example.com/asset.wav".to_owned(),
        revision: "v1.0".to_owned(),
        source_sha256: "0".repeat(64),
        license: "CC0-1.0".to_owned(),
        redistribution_terms: "public-domain".to_owned(),
        processing_authorization: "explicit".to_owned(),
        attribution: "Test Author".to_owned(),
    };
    let case = realtime_noise_tools::golden_manifest::CorpusCaseV2 {
        case_id: "case-001".to_owned(),
        frames_path: PathBuf::from("frames/case-001.json"),
        input_sha256: "0".repeat(64),
        source_id: "src-001".to_owned(),
        transcription_applicability: "none".to_owned(),
        snr_noise_class: "stationary".to_owned(),
        sample_rate_hz: 48000,
        channels: 1,
        frame_count: 2,
    };
    let manifest = realtime_noise_tools::golden_manifest::ManifestV2 {
        schema_version: 2,
        status: "APPROVED".to_owned(),
        corpus_sha256: "0".repeat(64),
        input_normalization: "peak".to_owned(),
        quality_metric: "PESQ".to_owned(),
        quality_metric_version: "1.0".to_owned(),
        quality_threshold: 2.5,
        quality_observed_value: 3.0,
        tolerance: realtime_noise_model::NumericalTolerance { absolute: 0.0, relative: 0.0 },
        source_lock_sha256: "a".repeat(64),
        sources: vec![source],
        cases: vec![case],
    };
    let temp_root = env::temp_dir().join(format!(
        "hippocamp-test-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let corpus_dir = temp_root.join("fixtures/corpus");
    std::fs::create_dir_all(&corpus_dir).unwrap();
    std::fs::write(corpus_dir.join("source-lock.json"), b"content").unwrap();
    let err = validate_corpus(&temp_root, &Manifest::V2(manifest)).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_reason(&err),
        "source-lock.json digest mismatch"
    );
    let _ = std::fs::remove_dir_all(&temp_root);
}

#[test]
fn validate_input_case_v2_accepts_valid_frames() {
    let mut case = realtime_noise_tools::golden_manifest::CorpusCaseV2 {
        case_id: "case-v2-001".to_owned(),
        frames_path: PathBuf::from("frames/case-001.json"),
        input_sha256: String::new(),
        source_id: "src-001".to_owned(),
        transcription_applicability: "none".to_owned(),
        snr_noise_class: "stationary".to_owned(),
        sample_rate_hz: 48000,
        channels: 1,
        frame_count: 2,
    };
    let frames = vec![vec![0.0f32; 480]; case.frame_count];
    case.input_sha256 = realtime_noise_model::frames_sha256(&frames);
    assert!(realtime_noise_tools::golden_manifest::validate_input_case_v2(&case, &frames).is_ok());
}

#[test]
fn validate_input_case_v2_rejects_mismatched_sha256() {
    let case = realtime_noise_tools::golden_manifest::CorpusCaseV2 {
        case_id: "case-v2-001".to_owned(),
        frames_path: PathBuf::from("frames/case-001.json"),
        input_sha256: "0".repeat(64),
        source_id: "src-001".to_owned(),
        transcription_applicability: "none".to_owned(),
        snr_noise_class: "stationary".to_owned(),
        sample_rate_hz: 48000,
        channels: 1,
        frame_count: 2,
    };
    let frames = vec![vec![0.0f32; 480]; case.frame_count];
    let err = realtime_noise_tools::golden_manifest::validate_input_case_v2(&case, &frames).unwrap_err();
    assert_eq!(
        realtime_noise_tools::golden_manifest::blocked_status(&err),
        "BLOCKED_PENDING_GOLDEN"
    );
}

#[test]
fn finalize_corpus_manifest() {
    let path = std::path::Path::new("fixtures/corpus/corpus-manifest.json");
    let manifest_path = if path.exists() {
        path.to_path_buf()
    } else {
        std::path::Path::new("../../fixtures/corpus/corpus-manifest.json").to_path_buf()
    };
    if let Ok(bytes) = std::fs::read(&manifest_path) {
        if let Ok(mut manifest) = serde_json::from_slice::<realtime_noise_tools::golden_manifest::ManifestV2>(&bytes) {
            let digest = realtime_noise_tools::golden_manifest::compute_corpus_sha256_v2(&manifest).unwrap();
            manifest.corpus_sha256 = digest;
            let formatted = serde_json::to_string_pretty(&manifest).unwrap();
            std::fs::write(&manifest_path, formatted).unwrap();
        }
    }
}

#[test]
fn real_corpus_manifest_v2_passes_validation() {
    let path = std::path::Path::new("fixtures/corpus/corpus-manifest.json");
    let root = if path.exists() {
        std::path::Path::new(".")
    } else {
        std::path::Path::new("../..")
    };
    let manifest_path = root.join("fixtures/corpus/corpus-manifest.json");
    if manifest_path.exists() {
        let manifest_bytes = std::fs::read(&manifest_path).unwrap();
        let manifest = realtime_noise_tools::golden_manifest::Manifest::parse(&manifest_bytes).unwrap();
        assert!(realtime_noise_tools::golden_manifest::validate_corpus(root, &manifest).is_ok());
    }
}
