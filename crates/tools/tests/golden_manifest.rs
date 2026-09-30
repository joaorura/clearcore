use realtime_noise_tools::golden_manifest::{
    CorpusCase, CorpusManifest, validate_corpus, validate_input_case,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::path::PathBuf;

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn valid_manifest() -> CorpusManifest {
    let cases = vec![valid_case()];
    CorpusManifest {
        schema_version: 1,
        status: "APPROVED".to_owned(),
        corpus_sha256: sha256(&serde_json::to_vec(&cases).unwrap()),
        input_normalization: "peak".to_owned(),
        quality_metric: "PESQ".to_owned(),
        quality_metric_version: "1.0".to_owned(),
        quality_threshold: 2.5,
        quality_observed_value: 3.0,
        tolerance: realtime_noise_model::NumericalTolerance { absolute: 0.0, relative: 0.0 },
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
    let parsed: CorpusManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.schema_version, 1);
    assert_eq!(parsed.status, "APPROVED");
}

#[test]
fn unknown_field_in_manifest_is_rejected() {
    let mut manifest = valid_manifest();
    let mut json = serde_json::to_value(&manifest).unwrap();
    json["unknown_field"] = json!(true);
    let result: Result<CorpusManifest, _> = serde_json::from_value(json);
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
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_non_approved_status() {
    let mut manifest = valid_manifest();
    manifest.status = "PENDING".to_owned();
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_empty_cases() {
    let mut manifest = valid_manifest();
    manifest.cases.clear();
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_non_finite_quality_threshold() {
    let mut manifest = valid_manifest();
    manifest.quality_threshold = f64::NAN;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_non_finite_quality_observed_value() {
    let mut manifest = valid_manifest();
    manifest.quality_observed_value = f64::INFINITY;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_quality_observed_below_threshold() {
    let mut manifest = valid_manifest();
    manifest.quality_observed_value = 2.0;
    manifest.quality_threshold = 2.5;
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_duplicate_case_id() {
    let mut manifest = valid_manifest();
    let case = valid_case();
    manifest.cases.push(case);
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_empty_case_id() {
    let mut manifest = valid_manifest();
    manifest.cases[0].case_id = "".to_owned();
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_digest_mismatch() {
    let mut manifest = valid_manifest();
    manifest.corpus_sha256 = "0".repeat(64);
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_absolute_frames_path() {
    let mut manifest = valid_manifest();
    manifest.cases[0].frames_path = PathBuf::from("/absolute/path.json");
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_parent_dir_in_frames_path() {
    let mut manifest = valid_manifest();
    manifest.cases[0].frames_path = PathBuf::from("../escape.json");
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_corpus_rejects_non_finite_provenance_fields() {
    let mut manifest = valid_manifest();
    manifest.cases[0].license = "".to_owned();
    manifest.corpus_sha256 = sha256(&serde_json::to_vec(&manifest.cases).unwrap());
    let root = env::temp_dir();
    let err = validate_corpus(&root, &manifest).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_input_case_rejects_wrong_frame_count() {
    let case = valid_case();
    let frames = vec![vec![0.0; 480]; 3];
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_input_case_rejects_wrong_frame_length() {
    let case = valid_case();
    let frames = vec![vec![0.0; 479]; 2];
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_input_case_rejects_non_finite_frames() {
    let case = valid_case();
    let mut frames = valid_frames(&case);
    frames[0][0] = f32::NAN;
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}

#[test]
fn validate_input_case_rejects_frames_sha256_mismatch() {
    let case = valid_case();
    let frames = valid_frames(&case);
    let err = validate_input_case(&case, &frames).unwrap_err();
    assert_eq!(err.0, "BLOCKED_PENDING_GOLDEN");
}