//! Typed enrollment failures; each maps to one fixed IPC code and carries no payload.

use realtime_noise_ipc::enrollment_codes as codes;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrollError {
    Clipping,
    TooQuiet,
    TooLittleSpeech,
    ModelNotConfigured,
    BudgetExceeded,
    InvalidAudio,
    PayloadTooLarge,
    JobNotFound,
    Failed,
}

impl EnrollError {
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Clipping => codes::ENROLL_CLIPPING,
            Self::TooQuiet => codes::ENROLL_TOO_QUIET,
            Self::TooLittleSpeech => codes::ENROLL_TOO_LITTLE_SPEECH,
            Self::ModelNotConfigured => codes::ENROLL_MODEL_NOT_CONFIGURED,
            Self::BudgetExceeded => codes::ENROLL_BUDGET_EXCEEDED,
            Self::InvalidAudio => codes::ENROLL_INVALID_AUDIO,
            Self::PayloadTooLarge => codes::ENROLL_PAYLOAD_TOO_LARGE,
            Self::JobNotFound => codes::ENROLL_JOB_NOT_FOUND,
            Self::Failed => codes::ENROLL_FAILED,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_error_maps_to_a_distinct_fixed_code() {
        use EnrollError::*;
        let all = [
            Clipping,
            TooQuiet,
            TooLittleSpeech,
            ModelNotConfigured,
            BudgetExceeded,
            InvalidAudio,
            PayloadTooLarge,
            JobNotFound,
            Failed,
        ];
        let mut codes: Vec<&str> = all.iter().map(EnrollError::code).collect();
        assert!(codes.iter().all(|c| c.starts_with("ENROLL_")));
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), all.len());
    }
}
