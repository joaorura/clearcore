//! Fixed error codes of the voice enrollment pipeline (spec section 5). Responses carry these codes only.

pub const ENROLL_CLIPPING: &str = "ENROLL_CLIPPING";
pub const ENROLL_TOO_QUIET: &str = "ENROLL_TOO_QUIET";
pub const ENROLL_TOO_LITTLE_SPEECH: &str = "ENROLL_TOO_LITTLE_SPEECH";
pub const ENROLL_MODEL_NOT_CONFIGURED: &str = "ENROLL_MODEL_NOT_CONFIGURED";
pub const ENROLL_BUDGET_EXCEEDED: &str = "ENROLL_BUDGET_EXCEEDED";
pub const ENROLL_INVALID_AUDIO: &str = "ENROLL_INVALID_AUDIO";
pub const ENROLL_PAYLOAD_TOO_LARGE: &str = "ENROLL_PAYLOAD_TOO_LARGE";
pub const ENROLL_JOB_NOT_FOUND: &str = "ENROLL_JOB_NOT_FOUND";
pub const ENROLL_FAILED: &str = "ENROLL_FAILED";
pub const ENROLL_BUSY: &str = "ENROLL_BUSY";
/// The active isolation model cannot apply a voice profile (e.g. no `FiLM` inputs, NPU backend).
pub const ENROLL_BACKEND_UNSUPPORTED: &str = "ENROLL_BACKEND_UNSUPPORTED";
/// 90 s * 48 kHz * 4 B = 17.28 MB; base64 inflates by 4/3 (23.04 MB) plus envelope.
pub const MAX_REQUEST_LINE_BYTES: usize = 32 * 1024 * 1024;

/// The closed list of enrollment codes; clients reject (or collapse) anything else.
pub const ALL_ENROLL_CODES: [&str; 11] = [
    ENROLL_CLIPPING,
    ENROLL_TOO_QUIET,
    ENROLL_TOO_LITTLE_SPEECH,
    ENROLL_MODEL_NOT_CONFIGURED,
    ENROLL_BUDGET_EXCEEDED,
    ENROLL_INVALID_AUDIO,
    ENROLL_PAYLOAD_TOO_LARGE,
    ENROLL_JOB_NOT_FOUND,
    ENROLL_FAILED,
    ENROLL_BUSY,
    ENROLL_BACKEND_UNSUPPORTED,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_list_is_distinct_prefixed_and_has_backend_unsupported() {
        let mut codes = ALL_ENROLL_CODES.to_vec();
        assert!(codes.iter().all(|c| c.starts_with("ENROLL_")));
        assert!(codes.contains(&"ENROLL_BACKEND_UNSUPPORTED"));
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), ALL_ENROLL_CODES.len());
    }
}
