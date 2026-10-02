//! Small numeric helpers shared by the DSP blocks.

use crate::SAMPLE_RATE_HZ;

/// Values below this magnitude are flushed to zero to avoid denormal slowdowns.
pub const DENORMAL_FLOOR: f64 = 1.0e-30;

/// Sample rate as `f64`.
pub fn sample_rate() -> f64 {
    f64::from(SAMPLE_RATE_HZ)
}

/// Converts decibels to a linear amplitude factor.
pub fn db_to_linear(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Converts a linear amplitude to decibels, flooring at -400 dB so the result is always finite.
pub fn linear_to_db(linear: f64) -> f64 {
    20.0 * linear.max(1.0e-20).log10()
}

/// One-pole smoothing coefficient `exp(-1 / (tau * fs))` for a time constant in milliseconds.
pub fn time_constant_coefficient(milliseconds: f64) -> f64 {
    (-1.0 / (milliseconds * 1.0e-3 * sample_rate())).exp()
}

/// Flushes denormal and non-finite values to zero.
pub fn flush(value: f64) -> f64 {
    if value.is_finite() && value.abs() >= DENORMAL_FLOOR {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{db_to_linear, flush, linear_to_db, time_constant_coefficient};

    #[test]
    fn decibel_conversions_round_trip() {
        for db in [-60.0, -20.0, -1.0, 0.0, 6.0] {
            assert!((linear_to_db(db_to_linear(db)) - db).abs() < 1.0e-9);
        }
        assert!((db_to_linear(-6.0206) - 0.5).abs() < 1.0e-4);
    }

    #[test]
    fn linear_to_db_is_finite_for_zero() {
        assert!(linear_to_db(0.0).is_finite());
    }

    #[test]
    fn time_constant_reaches_one_over_e_after_tau() {
        let coefficient = time_constant_coefficient(10.0);
        let after_tau = coefficient.powi(480);
        assert!((after_tau - (-1.0_f64).exp()).abs() < 1.0e-9);
    }

    #[test]
    fn flush_removes_denormals_and_non_finite_values() {
        assert!(flush(f64::NAN).abs() < f64::MIN_POSITIVE);
        assert!(flush(f64::INFINITY).abs() < f64::MIN_POSITIVE);
        assert!(flush(1.0e-310).abs() < f64::MIN_POSITIVE);
        assert!((flush(0.5) - 0.5).abs() < f64::EPSILON);
    }
}
