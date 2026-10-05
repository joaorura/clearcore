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
/// 90 s * 48 kHz * 4 B = 17.28 MB; base64 inflates by 4/3 (23.04 MB) plus envelope.
pub const MAX_REQUEST_LINE_BYTES: usize = 32 * 1024 * 1024;
