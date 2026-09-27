use std::{fs, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{BackendDescriptor, InferenceError, json::parse_unique};

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
pub struct NumericalTolerance {
    pub absolute: f32,
    pub relative: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GoldenProvenance {
    pub schema_version: u32,
    pub asset_sha256: String,
    pub backend: String,
    pub backend_version: String,
    pub runtime_version: String,
    pub cpu_profile: String,
    pub corpus_sha256: String,
    pub quality_metric: String,
    pub quality_metric_version: String,
    pub quality_threshold: f64,
    pub tolerance: NumericalTolerance,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GoldenCase {
    pub case_id: String,
    pub input_sha256: String,
    pub frames: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
        if self.provenance.schema_version != 1 {
            return Err(invalid("unsupported golden schema"));
        }
        if !self.provenance.tolerance.absolute.is_finite()
            || !self.provenance.tolerance.relative.is_finite()
            || self.provenance.tolerance.absolute < 0.0
            || self.provenance.tolerance.relative < 0.0
        {
            return Err(invalid("invalid numerical tolerance"));
        }
        let encoded =
            serde_json::to_vec(&self.provenance).map_err(|error| invalid(&error.to_string()))?;
        if format!("{:x}", Sha256::digest(encoded)) != self.provenance_sha256 {
            return Err(invalid("golden provenance digest mismatch"));
        }
        if self.cases.is_empty() {
            return Err(invalid("golden has no corpus cases"));
        }
        for case in &self.cases {
            if case.case_id.is_empty() || case.frames.is_empty() {
                return Err(invalid("golden case is incomplete"));
            }
            if case.frames.iter().any(|frame| {
                frame.len() != realtime_noise_contracts::HOP_SAMPLES
                    || frame.iter().any(|sample| !sample.is_finite())
            }) {
                return Err(invalid("golden frame violates the audio contract"));
            }
        }
        Ok(())
    }

    pub fn matches_descriptor(&self, descriptor: &BackendDescriptor) -> bool {
        self.provenance.asset_sha256 == descriptor.asset_sha256
            && self.provenance.backend == descriptor.backend
            && self.provenance.backend_version == descriptor.backend_version
            && self.provenance.runtime_version == descriptor.runtime_version
            && self.provenance.cpu_profile == descriptor.cpu_profile
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
}

fn invalid(message: &str) -> InferenceError {
    InferenceError::GoldenValidation(message.to_owned())
}
