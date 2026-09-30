#![forbid(unsafe_code)]

use crate::privacy::{hash_device_id, sanitize_causes};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const DIAGNOSTICS_SCHEMA_VERSION: &str = "realtime-noise.diagnostics.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LatencyPercentiles {
    pub p50_us: u64,
    pub p95_us: u64,
    pub p99_us: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetBreakdown {
    pub measured_us: u64,
    pub configured_us: u64,
    pub derived_us: u64,
    pub unobservable_us: u64,
}

/// Raw, unsanitized diagnostic inputs collected from the audio runtime and supervisor.
/// Contains raw identifiers and potential audio/contextual fragments that MUST be stripped
/// before export.
#[derive(Debug, Clone, PartialEq)]
pub struct RawDiagnosticInput {
    pub device_id: String,
    pub generation: u64,
    pub causes: Vec<String>,
    pub last_attempt: Option<usize>,
    pub pcm_samples: Vec<f32>,
    pub embeddings: Vec<f32>,
    pub meeting_title: Option<String>,
    pub transcript_snippet: Option<String>,
    pub latency_p50_us: u64,
    pub latency_p95_us: u64,
    pub latency_p99_us: u64,
    pub budget_measured_us: u64,
    pub budget_configured_us: u64,
    pub budget_derived_us: u64,
    pub budget_unobservable_us: u64,
}

/// Fully sanitized, privacy-first diagnostic payload.
/// Guaranteed to exclude all raw PCM audio, ML embedding vectors, meeting titles,
/// and speech transcripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticPayload {
    pub schema_version: String,
    pub device_id_hash: String,
    pub generation: u64,
    pub causes: Vec<String>,
    pub last_attempt: Option<usize>,
    pub latencies: LatencyPercentiles,
    pub budget: BudgetBreakdown,
    pub timestamp_utc: String,
}

#[derive(Debug)]
pub enum DiagnosticExportError {
    Serialization(serde_json::Error),
    PrivacyViolation(String),
}

impl fmt::Display for DiagnosticExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Serialization(e) => write!(f, "JSON serialization error: {e}"),
            Self::PrivacyViolation(v) => write!(f, "Diagnostic privacy violation: {v}"),
        }
    }
}

impl std::error::Error for DiagnosticExportError {}

impl From<serde_json::Error> for DiagnosticExportError {
    fn from(err: serde_json::Error) -> Self {
        Self::Serialization(err)
    }
}

#[derive(Debug, Clone)]
pub struct DiagnosticExporter {
    install_salt: String,
}

impl DiagnosticExporter {
    #[must_use]
    pub fn new(install_salt: impl Into<String>) -> Self {
        Self {
            install_salt: install_salt.into(),
        }
    }

    /// Transforms raw diagnostic input into a strictly sanitized, privacy-preserving `DiagnosticPayload`.
    /// Strips all audio samples, embeddings, meeting names, and transcripts.
    #[must_use]
    pub fn export_payload(&self, input: &RawDiagnosticInput) -> DiagnosticPayload {
        let hashed_id = hash_device_id(&input.device_id, &self.install_salt);
        let sanitized_causes = sanitize_causes(&input.causes);

        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        DiagnosticPayload {
            schema_version: DIAGNOSTICS_SCHEMA_VERSION.to_string(),
            device_id_hash: hashed_id,
            generation: input.generation,
            causes: sanitized_causes,
            last_attempt: input.last_attempt,
            latencies: LatencyPercentiles {
                p50_us: input.latency_p50_us,
                p95_us: input.latency_p95_us,
                p99_us: input.latency_p99_us,
            },
            budget: BudgetBreakdown {
                measured_us: input.budget_measured_us,
                configured_us: input.budget_configured_us,
                derived_us: input.budget_derived_us,
                unobservable_us: input.budget_unobservable_us,
            },
            timestamp_utc: format!("{ts}s-epoch"),
        }
    }

    /// Exports sanitized diagnostics as a JSON string with an additional fail-closed verification check.
    pub fn export_json(&self, input: &RawDiagnosticInput) -> Result<String, DiagnosticExportError> {
        let payload = self.export_payload(input);
        let json_str = serde_json::to_string_pretty(&payload)?;

        // Fail-closed privacy inspection of outgoing serialized bytes
        Self::verify_privacy_envelope(&json_str)?;

        Ok(json_str)
    }

    /// Exports sanitized diagnostics as raw archive bytes.
    pub fn export_archive(
        &self,
        input: &RawDiagnosticInput,
    ) -> Result<Vec<u8>, DiagnosticExportError> {
        let json_str = self.export_json(input)?;
        Ok(json_str.into_bytes())
    }

    fn verify_privacy_envelope(json: &str) -> Result<(), DiagnosticExportError> {
        let forbidden_keys = [
            "\"pcm_samples\"",
            "\"pcm\"",
            "\"embeddings\"",
            "\"embedding\"",
            "\"meeting_title\"",
            "\"transcript_snippet\"",
            "\"raw_samples\"",
            "\"audio_data\"",
        ];

        for key in &forbidden_keys {
            if json.contains(key) {
                return Err(DiagnosticExportError::PrivacyViolation(format!(
                    "Forbidden key found in diagnostic export: {key}"
                )));
            }
        }

        Ok(())
    }
}
