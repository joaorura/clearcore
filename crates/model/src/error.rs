use std::{error::Error, fmt};

#[derive(Debug)]
pub enum InferenceError {
    AssetNotApproved(String),
    ArchiveValidation(String),
    UnsupportedCpuProfile(String),
    ModelCorruption(String),
    InputContract(String),
    NonFiniteOutput,
    InferenceExecution(String),
    DeadlineMeasurement(String),
    GoldenPending(String),
    GoldenValidation(String),
    Io(std::io::Error),
}

impl fmt::Display for InferenceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetNotApproved(message) => write!(formatter, "asset not approved: {message}"),
            Self::ArchiveValidation(message) => {
                write!(formatter, "archive validation failed: {message}")
            }
            Self::UnsupportedCpuProfile(message) => {
                write!(formatter, "unsupported CPU profile: {message}")
            }
            Self::ModelCorruption(message) => write!(formatter, "model is corrupt: {message}"),
            Self::InputContract(message) => write!(formatter, "input contract mismatch: {message}"),
            Self::NonFiniteOutput => formatter.write_str("backend produced non-finite output"),
            Self::InferenceExecution(message) => write!(formatter, "inference failed: {message}"),
            Self::DeadlineMeasurement(message) => {
                write!(formatter, "measurement failed: {message}")
            }
            Self::GoldenPending(message) => write!(formatter, "BLOCKED_PENDING_GOLDEN: {message}"),
            Self::GoldenValidation(message) => {
                write!(formatter, "golden validation failed: {message}")
            }
            Self::Io(error) => write!(formatter, "I/O failed: {error}"),
        }
    }
}

impl Error for InferenceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for InferenceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
