#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod export;
pub mod privacy;

pub use export::{
    BudgetBreakdown, DIAGNOSTICS_SCHEMA_VERSION, DiagnosticExportError, DiagnosticExporter,
    DiagnosticPayload, LatencyPercentiles, RawDiagnosticInput,
};
pub use privacy::{hash_device_id, sanitize_causes, sanitize_text, sha256_digest, sha256_hex};
