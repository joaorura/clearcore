use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::host_frequency::{
    SustainedFrequencyObservation, SustainedFrequencySummary, validate_sustained_frequency,
};
pub use crate::host_observation::{CpuTopologyEntry, LocalHostFacts, observe_local_host};

const MINIMUM_RAM_KIB: u64 = 8 * 1024 * 1024;
const MINIMUM_OBSERVATION_SECONDS: u64 = 300;
const MAXIMUM_EVIDENCE_AGE_SECONDS: u64 = 300;
const MAXIMUM_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_PROVENANCE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct RunBinding<'a> {
    pub run_id: &'a str,
    pub started_at_unix_seconds: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct HostEvidenceFiles<'a> {
    pub document: &'a [u8],
    pub provenance: &'a [u8],
}

#[derive(Debug, Clone, Serialize)]
pub struct BoundHostEvidence {
    pub evidence_sha256: String,
    pub run_id: String,
    pub observation_started_at_unix_seconds: u64,
    pub observation_finished_at_unix_seconds: u64,
    pub host: HostIdentity,
    pub operating_conditions: OperatingConditions,
    pub sustained_frequency: SustainedFrequencySummary,
    pub provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostEvidenceError {
    Malformed(String),
    Invalid(&'static str),
}

impl fmt::Display for HostEvidenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(reason) => write!(formatter, "malformed host evidence: {reason}"),
            Self::Invalid(reason) => write!(formatter, "invalid host evidence: {reason}"),
        }
    }
}

impl Error for HostEvidenceError {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct HostEvidenceDocument {
    schema_version: u32,
    run_id: String,
    observation_started_at_unix_seconds: u64,
    observation_finished_at_unix_seconds: u64,
    host: HostIdentity,
    operating_conditions: OperatingConditions,
    sustained_frequency_observation: SustainedFrequencyObservation,
    provenance: EvidenceProvenance,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct HostIdentity {
    pub os: String,
    pub architecture: String,
    pub cpu_model: String,
    pub physical_cores: usize,
    pub avx2: bool,
    pub ram_kib: u64,
    pub virtualized: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OperatingConditions {
    pub ac_power: bool,
    pub affinity_cpus: Vec<usize>,
    pub governor: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceProvenance {
    pub collector: String,
    pub command: Vec<String>,
    pub source_sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationProvenanceRecord {
    schema_version: u32,
    run_id: String,
    observation_started_at_unix_seconds: u64,
    observation_finished_at_unix_seconds: u64,
    collector: String,
    command: Vec<String>,
    observation_sha256: String,
}

/// Parses and binds a host-evidence document to the local host and benchmark run.
///
/// # Errors
///
/// Returns [`HostEvidenceError`] when the JSON is malformed or when any host,
/// operating-condition, provenance, freshness, or run-binding check fails.
pub fn bind_host_evidence(
    files: HostEvidenceFiles<'_>,
    local: &LocalHostFacts,
    binding: &RunBinding<'_>,
) -> Result<BoundHostEvidence, HostEvidenceError> {
    if files.document.len() > MAXIMUM_DOCUMENT_BYTES
        || files.provenance.len() > MAXIMUM_PROVENANCE_BYTES
    {
        return Err(HostEvidenceError::Invalid(
            "host evidence exceeds size limit",
        ));
    }
    let document: HostEvidenceDocument = serde_json::from_slice(files.document)
        .map_err(|error| HostEvidenceError::Malformed(error.to_string()))?;
    let sustained_frequency = validate_document(&document, files.provenance, local, binding)?;
    Ok(BoundHostEvidence {
        evidence_sha256: sha256(files.document),
        run_id: document.run_id,
        observation_started_at_unix_seconds: document.observation_started_at_unix_seconds,
        observation_finished_at_unix_seconds: document.observation_finished_at_unix_seconds,
        host: document.host,
        operating_conditions: document.operating_conditions,
        sustained_frequency,
        provenance: document.provenance,
    })
}

fn validate_document(
    document: &HostEvidenceDocument,
    provenance: &[u8],
    local: &LocalHostFacts,
    binding: &RunBinding<'_>,
) -> Result<SustainedFrequencySummary, HostEvidenceError> {
    let observed_seconds = document
        .observation_finished_at_unix_seconds
        .checked_sub(document.observation_started_at_unix_seconds)
        .ok_or(HostEvidenceError::Invalid(
            "observation interval is reversed",
        ))?;
    let evidence_age = binding
        .started_at_unix_seconds
        .checked_sub(document.observation_finished_at_unix_seconds)
        .ok_or(HostEvidenceError::Invalid(
            "observation finishes after benchmark start",
        ))?;
    if document.schema_version != 2
        || document.run_id.is_empty()
        || document.run_id != binding.run_id
        || observed_seconds != MINIMUM_OBSERVATION_SECONDS
        || evidence_age > MAXIMUM_EVIDENCE_AGE_SECONDS
    {
        return Err(HostEvidenceError::Invalid(
            "stale or mismatched run binding",
        ));
    }
    validate_host(&document.host, local)?;
    validate_operating_conditions(&document.operating_conditions, local)?;
    let sustained_frequency =
        validate_sustained_frequency(&document.sustained_frequency_observation, local)
            .map_err(HostEvidenceError::Invalid)?;
    validate_provenance(&document.provenance, provenance, document)?;
    Ok(sustained_frequency)
}

fn validate_host(host: &HostIdentity, local: &LocalHostFacts) -> Result<(), HostEvidenceError> {
    let local_matches = local.os == host.os
        && local.architecture == host.architecture
        && local.cpu_model == host.cpu_model
        && local.physical_cores == Some(host.physical_cores)
        && local.avx2 == host.avx2
        && local.ram_kib == Some(host.ram_kib)
        && local.virtualized == Some(host.virtualized);
    let reference_matches = host.architecture == "x86_64"
        && host.cpu_model.contains("i5-10210U")
        && host.physical_cores == 4
        && host.avx2
        && host.ram_kib >= MINIMUM_RAM_KIB
        && !host.virtualized;
    if !local_matches || !reference_matches {
        return Err(HostEvidenceError::Invalid(
            "host identity or reference class mismatch",
        ));
    }
    Ok(())
}

fn validate_operating_conditions(
    conditions: &OperatingConditions,
    local: &LocalHostFacts,
) -> Result<(), HostEvidenceError> {
    let affinity_is_canonical = !conditions.affinity_cpus.is_empty()
        && conditions
            .affinity_cpus
            .windows(2)
            .all(|pair| pair[0] < pair[1]);
    if !conditions.ac_power
        || local.ac_power != Some(conditions.ac_power)
        || !affinity_is_canonical
        || local.affinity_cpus != conditions.affinity_cpus
        || conditions.governor.is_empty()
        || local.governor.as_deref() != Some(conditions.governor.as_str())
    {
        return Err(HostEvidenceError::Invalid(
            "operating conditions are unverified",
        ));
    }
    Ok(())
}

fn validate_provenance(
    provenance: &EvidenceProvenance,
    source: &[u8],
    document: &HostEvidenceDocument,
) -> Result<(), HostEvidenceError> {
    let record: ObservationProvenanceRecord = serde_json::from_slice(source)
        .map_err(|error| HostEvidenceError::Malformed(error.to_string()))?;
    let observation = serde_json::to_value(&document.sustained_frequency_observation)
        .and_then(|value| serde_json::to_vec(&value))
        .map_err(|error| HostEvidenceError::Malformed(error.to_string()))?;
    if provenance.collector.is_empty()
        || provenance.command.is_empty()
        || provenance.command.iter().any(String::is_empty)
        || !is_sha256(&provenance.source_sha256)
        || provenance.source_sha256 != sha256(source)
        || record.schema_version != 1
        || record.run_id != document.run_id
        || record.observation_started_at_unix_seconds
            != document.observation_started_at_unix_seconds
        || record.observation_finished_at_unix_seconds
            != document.observation_finished_at_unix_seconds
        || record.collector != provenance.collector
        || record.command != provenance.command
        || !is_sha256(&record.observation_sha256)
        || record.observation_sha256 != sha256(&observation)
    {
        return Err(HostEvidenceError::Invalid(
            "observation provenance is incomplete",
        ));
    }
    Ok(())
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
