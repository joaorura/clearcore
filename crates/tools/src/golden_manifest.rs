use std::path::{Component, Path, PathBuf};
use realtime_noise_model::frames_sha256;
use serde::Deserialize;
use sha2::Digest;

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusManifest {
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

#[derive(Deserialize, serde::Serialize)]
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

pub fn validate_corpus(root: &Path, corpus: &CorpusManifest) -> Result<(), (&'static str, String)> {
    if corpus.schema_version != 1
        || corpus.status != "APPROVED"
        || corpus.cases.is_empty()
        || !corpus.quality_threshold.is_finite()
        || !corpus.quality_observed_value.is_finite()
        || corpus.quality_observed_value < corpus.quality_threshold
    {
        return Err(pending("corpus manifest is not approved and complete"));
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
            return Err(pending("corpus case provenance or ordering is invalid"));
        }
        let path = root.join("fixtures/corpus").join(&case.frames_path);
        if !path.starts_with(root.join("fixtures/corpus")) {
            return Err(pending("corpus path escapes its root"));
        }
        previous = Some(&case.case_id);
    }
    let digest = sha256(&serde_json::to_vec(&corpus.cases).map_err(golden_block)?);
    if digest != corpus.corpus_sha256 {
        return Err(pending("corpus manifest digest mismatch"));
    }
    Ok(())
}

pub fn validate_input_case(
    case: &CorpusCase,
    frames: &[Vec<f32>],
) -> Result<(), (&'static str, String)> {
    if frames.len() != case.frame_count
        || frames.iter().any(|frame| {
            frame.len() != realtime_noise_contracts::HOP_SAMPLES
                || frame.iter().any(|sample| !sample.is_finite())
        })
        || frames_sha256(frames) != case.input_sha256
    {
        Err(pending("corpus frames do not match their approved binding"))
    } else {
        Ok(())
    }
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", sha2::Sha256::digest(bytes))
}

pub fn pending(reason: &str) -> (&'static str, String) {
    ("BLOCKED_PENDING_GOLDEN", reason.to_owned())
}

pub fn golden_block(error: impl std::fmt::Display) -> (&'static str, String) {
    pending(&error.to_string())
}
