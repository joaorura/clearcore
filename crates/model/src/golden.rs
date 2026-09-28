use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BackendDescriptor, InferenceError, json::parse_unique};

const SHA256_LENGTH: usize = 64;

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct NumericalTolerance {
    pub absolute: f32,
    pub relative: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenProvenance {
    pub schema_version: u32,
    pub asset_id: String,
    pub asset_sha256: String,
    pub candidate_record_sha256: String,
    pub legal_review_record_sha256: String,
    pub key_id: String,
    pub backend: String,
    pub backend_version: String,
    pub runtime: String,
    pub runtime_version: String,
    pub cpu_profile: String,
    pub corpus_sha256: String,
    pub generator_revision: String,
    pub generator_command: String,
    pub generated_at_utc: String,
    pub host_os: String,
    pub host_cpu: String,
    pub isolated_offline: bool,
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub hop_samples: usize,
    pub algorithmic_latency_samples: u32,
    pub input_normalization: String,
    pub quality_metric: String,
    pub quality_metric_version: String,
    pub quality_threshold: f64,
    pub quality_observed_value: f64,
    pub tolerance: NumericalTolerance,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenCase {
    pub case_id: String,
    pub input_sha256: String,
    pub output_sha256: String,
    pub frame_count: usize,
    pub input_frames: Vec<Vec<f32>>,
    pub output_frames: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoldenFixture {
    pub provenance: GoldenProvenance,
    pub provenance_sha256: String,
    pub cases: Vec<GoldenCase>,
}

impl GoldenFixture {
    pub fn read(path: &Path) -> Result<Self, InferenceError> {
        let bytes = fs::read(path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                InferenceError::GoldenPending(format!("{} is absent", path.display()))
            } else {
                InferenceError::Io(error)
            }
        })?;
        let value = parse_unique(&bytes)
            .map_err(|error| InferenceError::GoldenValidation(error.to_string()))?;
        let fixture: Self = serde_json::from_value(value)
            .map_err(|error| InferenceError::GoldenValidation(error.to_string()))?;
        fixture.validate()?;
        Ok(fixture)
    }

    pub fn validate(&self) -> Result<(), InferenceError> {
        self.validate_provenance()?;
        let encoded = serde_json::to_vec(&self.provenance).map_err(|error| json_error(&error))?;
        if sha256_bytes(&encoded) != self.provenance_sha256 {
            return Err(invalid("golden provenance digest mismatch"));
        }
        if self.cases.is_empty() {
            return Err(invalid("golden has no corpus cases"));
        }
        let mut previous: Option<&str> = None;
        for case in &self.cases {
            if case.case_id.trim().is_empty()
                || previous.is_some_and(|value| value >= case.case_id.as_str())
                || !is_sha256(&case.input_sha256)
                || !is_sha256(&case.output_sha256)
                || case.frame_count == 0
                || case.input_frames.len() != case.frame_count
                || case.output_frames.len() != case.frame_count
            {
                return Err(invalid("golden case identity or frame count is invalid"));
            }
            validate_frames(&case.input_frames)?;
            validate_frames(&case.output_frames)?;
            if frames_sha256(&case.input_frames) != case.input_sha256
                || frames_sha256(&case.output_frames) != case.output_sha256
            {
                return Err(invalid("golden case checksum mismatch"));
            }
            previous = Some(&case.case_id);
        }
        Ok(())
    }

    pub fn validate_for(&self, descriptor: &BackendDescriptor) -> Result<(), InferenceError> {
        self.validate()?;
        if self.provenance.asset_id == descriptor.asset_id
            && self.provenance.asset_sha256 == descriptor.asset_sha256
            && self.provenance.backend == descriptor.backend
            && self.provenance.backend_version == descriptor.backend_version
            && self.provenance.runtime == descriptor.runtime
            && self.provenance.runtime_version == descriptor.runtime_version
            && self.provenance.cpu_profile == descriptor.cpu_profile
        {
            Ok(())
        } else {
            Err(invalid("golden backend descriptor mismatch"))
        }
    }

    pub fn compare_case(&self, case_id: &str, actual: &[Vec<f32>]) -> Result<(), InferenceError> {
        let case = self
            .cases
            .iter()
            .find(|case| case.case_id == case_id)
            .ok_or_else(|| invalid("golden case is missing"))?;
        if actual.len() != case.output_frames.len() {
            return Err(invalid("golden output frame count mismatch"));
        }
        for (expected, actual) in case.output_frames.iter().zip(actual) {
            if actual.len() != expected.len() {
                return Err(invalid("golden output frame shape mismatch"));
            }
            for (&expected, &actual) in expected.iter().zip(actual) {
                self.compare_sample(expected, actual)?;
            }
        }
        Ok(())
    }

    pub fn compare_sample(&self, expected: f32, actual: f32) -> Result<(), InferenceError> {
        if !expected.is_finite() || !actual.is_finite() {
            return Err(invalid("comparison contains a non-finite sample"));
        }
        let difference = (expected - actual).abs();
        let limit = self
            .provenance
            .tolerance
            .relative
            .mul_add(expected.abs(), self.provenance.tolerance.absolute);
        if difference <= limit {
            Ok(())
        } else {
            Err(invalid("sample drift exceeds frozen tolerance"))
        }
    }

    fn validate_provenance(&self) -> Result<(), InferenceError> {
        let provenance = &self.provenance;
        if provenance.schema_version != 1
            || !provenance.isolated_offline
            || provenance.sample_rate_hz != realtime_noise_contracts::SAMPLE_RATE_HZ
            || provenance.channels != realtime_noise_contracts::CHANNELS
            || provenance.hop_samples != realtime_noise_contracts::HOP_SAMPLES
            || provenance.algorithmic_latency_samples != crate::ALGORITHM_LATENCY_SAMPLES
            || !provenance.quality_threshold.is_finite()
            || !provenance.quality_observed_value.is_finite()
            || provenance.quality_observed_value < provenance.quality_threshold
            || !provenance.tolerance.absolute.is_finite()
            || !provenance.tolerance.relative.is_finite()
            || provenance.tolerance.absolute < 0.0
            || provenance.tolerance.relative < 0.0
            || provenance.tolerance.absolute > 0.001
            || provenance.tolerance.relative > 0.001
        {
            return Err(invalid("invalid or unfrozen golden provenance"));
        }
        for digest in [
            &provenance.asset_sha256,
            &provenance.candidate_record_sha256,
            &provenance.legal_review_record_sha256,
            &provenance.corpus_sha256,
            &self.provenance_sha256,
        ] {
            if !is_sha256(digest) {
                return Err(invalid("invalid provenance digest"));
            }
        }
        for text in [
            &provenance.asset_id,
            &provenance.key_id,
            &provenance.backend,
            &provenance.backend_version,
            &provenance.runtime,
            &provenance.runtime_version,
            &provenance.cpu_profile,
            &provenance.generator_revision,
            &provenance.generator_command,
            &provenance.generated_at_utc,
            &provenance.host_os,
            &provenance.host_cpu,
            &provenance.input_normalization,
            &provenance.quality_metric,
            &provenance.quality_metric_version,
        ] {
            if text.trim().is_empty() {
                return Err(invalid("golden provenance field is empty"));
            }
        }
        Ok(())
    }
}

pub fn frames_sha256(frames: &[Vec<f32>]) -> String {
    let mut digest = Sha256::new();
    for frame in frames {
        for sample in frame {
            digest.update(sample.to_le_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn validate_frames(frames: &[Vec<f32>]) -> Result<(), InferenceError> {
    if frames.iter().any(|frame| {
        frame.len() != realtime_noise_contracts::HOP_SAMPLES
            || frame.iter().any(|sample| !sample.is_finite())
    }) {
        Err(invalid("golden frame violates the audio contract"))
    } else {
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == SHA256_LENGTH
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn invalid(message: &str) -> InferenceError {
    InferenceError::GoldenValidation(message.to_owned())
}

fn json_error(error: &serde_json::Error) -> InferenceError {
    invalid(&error.to_string())
}
