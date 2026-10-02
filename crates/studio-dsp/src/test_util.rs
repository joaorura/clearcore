//! Synthetic-signal helpers for unit tests (compiled only under `cfg(test)`).

use crate::units::sample_rate;
use std::f64::consts::TAU;

/// Generates `len` samples of a sine with the given peak amplitude.
#[allow(clippy::cast_precision_loss)]
pub fn sine(freq_hz: f64, amplitude: f64, len: usize) -> Vec<f64> {
    let step = TAU * freq_hz / sample_rate();
    (0..len)
        .map(|index| amplitude * (step * index as f64).sin())
        .collect()
}

/// Root-mean-square of a slice (0 for an empty slice).
#[allow(clippy::cast_precision_loss)]
pub fn rms(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let energy: f64 = samples.iter().map(|sample| sample * sample).sum();
    (energy / samples.len() as f64).sqrt()
}

/// Converts a linear amplitude to dB.
pub fn db(linear: f64) -> f64 {
    crate::units::linear_to_db(linear)
}

#[cfg(test)]
mod tests {
    use super::{db, rms, sine};

    #[test]
    fn sine_rms_is_amplitude_over_root_two() {
        let signal = sine(1_000.0, 0.5, 48_000);
        assert!((rms(&signal) - 0.5 / 2.0_f64.sqrt()).abs() < 1.0e-6);
    }

    #[test]
    fn db_of_one_is_zero() {
        assert!(db(1.0).abs() < 1.0e-12);
    }
}
