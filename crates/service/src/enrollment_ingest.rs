//! Ingestion of enrollment samples. T0 declares only the `Denoiser` trait; task S4 fills the rest.

use crate::enrollment_error::EnrollError;

pub trait Denoiser: Send {
    /// 48 kHz mono in -> 48 kHz mono out, same length, latency already compensated.
    fn denoise(&mut self, pcm48: &[f32]) -> Result<Vec<f32>, EnrollError>;
}
