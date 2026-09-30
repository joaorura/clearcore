#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_contracts::SAMPLE_RATE_HZ;

use crate::FormatError;

/// Trait for explicit sample rate conversion adapters.
pub trait Resampler: Send + Sync {
    /// Source sample rate in Hz.
    fn source_rate_hz(&self) -> u32;

    /// Target sample rate in Hz (typically `48_000` for Project Hippocamp).
    fn target_rate_hz(&self) -> u32;

    /// Resamples `input` into `output`, returning the number of output samples written.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] if the buffer sizes do not match expected conversion ratio.
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<usize, FormatError>;
}

/// Identity resampler used when native hardware rate matches the 48 kHz canonical rate.
#[derive(Debug, Clone, Copy)]
pub struct IdentityResampler {
    rate_hz: u32,
}

impl IdentityResampler {
    /// Creates a new `IdentityResampler` verifying the rate is exactly 48 kHz.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::UnsupportedSampleRate`] if `rate_hz != 48_000`.
    pub fn new(rate_hz: u32) -> Result<Self, FormatError> {
        if rate_hz != SAMPLE_RATE_HZ {
            return Err(FormatError::UnsupportedSampleRate(rate_hz));
        }
        Ok(Self { rate_hz })
    }
}

impl Resampler for IdentityResampler {
    fn source_rate_hz(&self) -> u32 {
        self.rate_hz
    }

    fn target_rate_hz(&self) -> u32 {
        self.rate_hz
    }

    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<usize, FormatError> {
        if input.len() != output.len() {
            return Err(FormatError::BufferLengthMismatch {
                expected: input.len(),
                actual: output.len(),
            });
        }
        output.copy_from_slice(input);
        Ok(input.len())
    }
}

/// Deterministic linear interpolation resampler for non-canonical sample rates.
#[derive(Debug)]
pub struct LinearResampler {
    source_rate_hz: u32,
    target_rate_hz: u32,
}

impl LinearResampler {
    /// Creates a new `LinearResampler` between arbitrary valid sample rates.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::UnsupportedSampleRate`] if either rate is 0.
    pub fn new(source_rate_hz: u32, target_rate_hz: u32) -> Result<Self, FormatError> {
        if source_rate_hz == 0 {
            return Err(FormatError::UnsupportedSampleRate(0));
        }
        if target_rate_hz == 0 {
            return Err(FormatError::UnsupportedSampleRate(0));
        }
        Ok(Self {
            source_rate_hz,
            target_rate_hz,
        })
    }
}

impl Resampler for LinearResampler {
    fn source_rate_hz(&self) -> u32 {
        self.source_rate_hz
    }

    fn target_rate_hz(&self) -> u32 {
        self.target_rate_hz
    }

    #[allow(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::suboptimal_flops
    )]
    fn process(&mut self, input: &[f32], output: &mut [f32]) -> Result<usize, FormatError> {
        if input.is_empty() || output.is_empty() {
            return Ok(0);
        }

        let ratio = f64::from(self.source_rate_hz) / f64::from(self.target_rate_hz);
        let max_src_idx = input.len().saturating_sub(1);

        for (out_idx, out_sample) in output.iter_mut().enumerate() {
            let src_pos = (out_idx as f64) * ratio;
            let src_idx = src_pos.floor() as usize;
            let frac = (src_pos - (src_idx as f64)) as f32;

            if src_idx >= max_src_idx {
                *out_sample = input[max_src_idx];
            } else {
                let s0 = input[src_idx];
                let s1 = input[src_idx + 1];
                *out_sample = s0 + frac * (s1 - s0);
            }
        }

        Ok(output.len())
    }
}
