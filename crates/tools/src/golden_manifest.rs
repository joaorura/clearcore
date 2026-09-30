#![allow(warnings)]
#![allow(
    clippy::missing_errors_doc,
    clippy::use_self,
    clippy::too_many_lines,
    clippy::items_after_statements,
    clippy::missing_const_for_fn
)]

use realtime_noise_model::frames_sha256;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub enum ManifestError {
    SchemaMismatch,
    Block(&'static str, String),
}

pub fn blocked_status(err: &ManifestError) -> &'static str {
    match err {
        ManifestError::SchemaMismatch => "BLOCKED_SCHEMA_MISMATCH",
        ManifestError::Block(status, _) => status,
    }
}

pub fn blocked_reason(err: &ManifestError) -> String {
    match err {
        ManifestError::SchemaMismatch => "schema version mismatch or unknown fields".to_string(),
        ManifestError::Block(_, reason) => reason.clone(),
    }
}

#[derive(Deserialize)]
struct VersionProbe {
    schema_version: Option<u32>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestV1 {
    pub schema_version: u32,
    pub status: String,
    pub corpus_sha256: String,
    pub input_normalization: String,
    pub quality_metric: String,
    pub quality_metric_version: String,
    pub quality_threshold: f64,
    pub quality_observed_value: f64,
    pub tolerance: realtime_noise_model::NumericalTolerance,
    pub cases: Vec<CorpusCase>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestV2 {
    pub schema_version: u32,
    pub status: String,
    pub corpus_sha256: String,
    pub input_normalization: String,
    pub quality_metric: String,
    pub quality_metric_version: String,
    pub quality_threshold: f64,
    pub quality_observed_value: f64,
    pub tolerance: realtime_noise_model::NumericalTolerance,
    pub source_lock_sha256: String,
    pub sources: Vec<SourceRecord>,
    pub cases: Vec<CorpusCaseV2>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub source_id: String,
    pub origin: String,
    pub revision: String,
    pub source_sha256: String,
    pub license: String,
    pub redistribution_terms: String,
    pub processing_authorization: String,
    pub attribution: String,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusCase {
    pub case_id: String,
    pub frames_path: PathBuf,
    pub input_sha256: String,
    pub license: String,
    pub redistribution_terms: String,
    pub authorization: String,
    pub transcription_applicability: String,
    pub snr_noise_class: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub frame_count: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusCaseV2 {
    pub case_id: String,
    pub frames_path: PathBuf,
    pub input_sha256: String,
    pub source_id: String,
    pub transcription_applicability: String,
    pub snr_noise_class: String,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub frame_count: usize,
}

pub enum Manifest {
    V1(ManifestV1),
    V2(ManifestV2),
}

impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self, ManifestError> {
        let probe: VersionProbe =
            serde_json::from_slice(bytes).map_err(|_| ManifestError::SchemaMismatch)?;
        match probe.schema_version {
            Some(1) => {
                let v1: ManifestV1 =
                    serde_json::from_slice(bytes).map_err(|_| ManifestError::SchemaMismatch)?;
                Ok(Manifest::V1(v1))
            }
            Some(2) => {
                let v2: ManifestV2 =
                    serde_json::from_slice(bytes).map_err(|_| ManifestError::SchemaMismatch)?;
                Ok(Manifest::V2(v2))
            }
            _ => Err(ManifestError::SchemaMismatch),
        }
    }
}

pub fn validate_corpus(root: &Path, manifest: &Manifest) -> Result<(), ManifestError> {
    match manifest {
        Manifest::V1(corpus) => {
            if corpus.schema_version != 1
                || corpus.status != "APPROVED"
                || corpus.cases.is_empty()
                || !corpus.quality_threshold.is_finite()
                || !corpus.quality_observed_value.is_finite()
                || corpus.quality_observed_value < corpus.quality_threshold
            {
                return Err(ManifestError::Block(
                    "BLOCKED_PENDING_GOLDEN",
                    "corpus manifest is not approved and complete".to_string(),
                ));
            }
            let mut previous: Option<&str> = None;
            for case in &corpus.cases {
                if previous.is_some_and(|value| value >= case.case_id.as_str())
                    || case.case_id.trim().is_empty()
                    || case.sample_rate_hz != realtime_noise_contracts::SAMPLE_RATE_HZ
                    || case.channels != realtime_noise_contracts::CHANNELS
                    || case.frame_count == 0
                    || case.frames_path.is_absolute()
                    || case
                        .frames_path
                        .components()
                        .any(|component| matches!(component, Component::ParentDir))
                    || [
                        &case.license,
                        &case.redistribution_terms,
                        &case.authorization,
                        &case.transcription_applicability,
                        &case.snr_noise_class,
                    ]
                    .iter()
                    .any(|value| value.trim().is_empty())
                {
                    return Err(ManifestError::Block(
                        "BLOCKED_PENDING_GOLDEN",
                        "corpus case provenance or ordering is invalid".to_string(),
                    ));
                }
                let path = root.join("fixtures/corpus").join(&case.frames_path);
                if !path.starts_with(root.join("fixtures/corpus")) {
                    return Err(ManifestError::Block(
                        "BLOCKED_PENDING_GOLDEN",
                        "corpus path escapes its root".to_string(),
                    ));
                }
                previous = Some(case.case_id.as_str());
            }
            let digest = sha256(
                &serde_json::to_vec(&corpus.cases)
                    .map_err(|e| ManifestError::Block("BLOCKED_PENDING_GOLDEN", e.to_string()))?,
            );
            if digest != corpus.corpus_sha256 {
                return Err(ManifestError::Block(
                    "BLOCKED_PENDING_GOLDEN",
                    "corpus manifest digest mismatch".to_string(),
                ));
            }
            Ok(())
        }
        Manifest::V2(corpus) => {
            if corpus.schema_version != 2
                || corpus.status != "APPROVED"
                || corpus.cases.is_empty()
                || !corpus.quality_threshold.is_finite()
                || !corpus.quality_observed_value.is_finite()
                || corpus.quality_observed_value < corpus.quality_threshold
            {
                return Err(ManifestError::Block(
                    "BLOCKED_PENDING_GOLDEN",
                    "corpus manifest is not approved and complete".to_string(),
                ));
            }

            // source-lock.json check
            let lock_path = root.join("fixtures/corpus/source-lock.json");
            match std::fs::read(&lock_path) {
                Ok(bytes) => {
                    if sha256(&bytes) != corpus.source_lock_sha256 {
                        return Err(ManifestError::Block(
                            "BLOCKED_PENDING_GOLDEN",
                            "source-lock.json digest mismatch".to_string(),
                        ));
                    }
                }
                Err(_) => {
                    return Err(ManifestError::Block(
                        "BLOCKED_PENDING_GOLDEN",
                        "source-lock.json missing".to_string(),
                    ));
                }
            }

            let mut previous: Option<&str> = None;
            for case in &corpus.cases {
                if previous.is_some_and(|value| value >= case.case_id.as_str())
                    || case.case_id.trim().is_empty()
                    || case.sample_rate_hz != realtime_noise_contracts::SAMPLE_RATE_HZ
                    || case.channels != realtime_noise_contracts::CHANNELS
                    || case.frame_count == 0
                    || case.frames_path.is_absolute()
                    || case
                        .frames_path
                        .components()
                        .any(|component| matches!(component, Component::ParentDir))
                    || [
                        &case.source_id,
                        &case.transcription_applicability,
                        &case.snr_noise_class,
                    ]
                    .iter()
                    .any(|value| value.trim().is_empty())
                {
                    return Err(ManifestError::Block(
                        "BLOCKED_PENDING_GOLDEN",
                        "corpus case provenance or ordering is invalid".to_string(),
                    ));
                }

                // source validation
                let mut found = false;
                for src in &corpus.sources {
                    if src.source_id == case.source_id {
                        found = true;
                        if src.source_id.trim().is_empty()
                            || src.origin.trim().is_empty()
                            || src.revision.trim().is_empty()
                            || src.license.trim().is_empty()
                            || src.redistribution_terms.trim().is_empty()
                            || src.processing_authorization.trim().is_empty()
                            || src.attribution.trim().is_empty()
                            || src.source_sha256.len() != 64
                            || !src
                                .source_sha256
                                .chars()
                                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
                        {
                            return Err(ManifestError::Block(
                                "BLOCKED_PENDING_GOLDEN",
                                "invalid source record".to_string(),
                            ));
                        }
                        break;
                    }
                }
                if !found {
                    return Err(ManifestError::Block(
                        "BLOCKED_PENDING_GOLDEN",
                        "source_id not found".to_string(),
                    ));
                }

                // duplicate source id check
                let mut ids = std::collections::HashSet::new();
                for src in &corpus.sources {
                    if !ids.insert(&src.source_id) {
                        return Err(ManifestError::Block(
                            "BLOCKED_PENDING_GOLDEN",
                            "duplicate source_id".to_string(),
                        ));
                    }
                }

                let path = root.join("fixtures/corpus").join(&case.frames_path);
                if !path.starts_with(root.join("fixtures/corpus")) {
                    return Err(ManifestError::Block(
                        "BLOCKED_PENDING_GOLDEN",
                        "corpus path escapes its root".to_string(),
                    ));
                }
                previous = Some(case.case_id.as_str());
            }

            let digest = compute_corpus_sha256_v2(corpus)?;
            if digest != corpus.corpus_sha256 {
                return Err(ManifestError::Block(
                    "BLOCKED_PENDING_GOLDEN",
                    "corpus manifest digest mismatch".to_string(),
                ));
            }
            Ok(())
        }
    }
}

pub fn compute_corpus_sha256_v2(corpus: &ManifestV2) -> Result<String, ManifestError> {
    #[derive(Serialize)]
    struct PayloadV2<'a> {
        schema_version: u32,
        status: &'a String,
        input_normalization: &'a String,
        quality_metric: &'a String,
        quality_metric_version: &'a String,
        quality_threshold: f64,
        quality_observed_value: f64,
        tolerance: &'a realtime_noise_model::NumericalTolerance,
        source_lock_sha256: &'a String,
        sources: &'a Vec<SourceRecord>,
        cases: &'a Vec<CorpusCaseV2>,
    }
    let payload = PayloadV2 {
        schema_version: corpus.schema_version,
        status: &corpus.status,
        input_normalization: &corpus.input_normalization,
        quality_metric: &corpus.quality_metric,
        quality_metric_version: &corpus.quality_metric_version,
        quality_threshold: corpus.quality_threshold,
        quality_observed_value: corpus.quality_observed_value,
        tolerance: &corpus.tolerance,
        source_lock_sha256: &corpus.source_lock_sha256,
        sources: &corpus.sources,
        cases: &corpus.cases,
    };
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|e| ManifestError::Block("BLOCKED_PENDING_GOLDEN", e.to_string()))?;
    let mut prefix = b"hippocamp-corpus-v2:".to_vec();
    prefix.extend_from_slice(&payload_bytes);
    Ok(sha256(&prefix))
}

pub fn validate_input_case(case: &CorpusCase, frames: &[Vec<f32>]) -> Result<(), ManifestError> {
    if frames.len() != case.frame_count
        || frames.iter().any(|frame| {
            frame.len() != realtime_noise_contracts::HOP_SAMPLES
                || frame.iter().any(|sample| !sample.is_finite())
        })
        || frames_sha256(frames) != case.input_sha256
    {
        Err(ManifestError::Block(
            "BLOCKED_PENDING_GOLDEN",
            "corpus frames do not match their approved binding".to_string(),
        ))
    } else {
        Ok(())
    }
}
pub fn validate_input_case_v2(
    case: &CorpusCaseV2,
    frames: &[Vec<f32>],
) -> Result<(), ManifestError> {
    if frames.len() != case.frame_count
        || frames.iter().any(|frame| {
            frame.len() != realtime_noise_contracts::HOP_SAMPLES
                || frame.iter().any(|sample| !sample.is_finite())
        })
        || frames_sha256(frames) != case.input_sha256
    {
        Err(ManifestError::Block(
            "BLOCKED_PENDING_GOLDEN",
            "corpus frames do not match their approved binding".to_string(),
        ))
    } else {
        Ok(())
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}
