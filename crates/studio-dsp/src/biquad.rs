//! Second-order IIR sections from the RBJ Audio EQ Cookbook, Direct Form II Transposed.
//!
//! Coefficients are computed once at construction (never inside the sample loop). State is `f64`
//! and denormals are flushed after every update.

use crate::units::{DENORMAL_FLOOR, db_to_linear, sample_rate};
use std::f64::consts::TAU;

/// One biquad section with normalised coefficients (`a0 == 1`) and its two state variables.
#[derive(Clone, Copy, Debug)]
pub struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl Biquad {
    /// Builds a section from already-normalised coefficients (`a0 == 1`).
    pub const fn from_coefficients(b0: f64, b1: f64, b2: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            z1: 0.0,
            z2: 0.0,
        }
    }

    /// A section that passes the signal unchanged.
    #[cfg(test)]
    pub const fn identity() -> Self {
        Self::from_coefficients(1.0, 0.0, 0.0, 0.0, 0.0)
    }

    fn normalised(b: [f64; 3], a: [f64; 3]) -> Self {
        Self::from_coefficients(
            b[0] / a[0],
            b[1] / a[0],
            b[2] / a[0],
            a[1] / a[0],
            a[2] / a[0],
        )
    }

    /// Returns `(cos w0, alpha)` for a centre frequency and Q.
    fn trig(freq_hz: f64, q: f64) -> (f64, f64) {
        let w0 = TAU * freq_hz / sample_rate();
        (w0.cos(), w0.sin() / (2.0 * q))
    }

    /// Second-order high-pass.
    pub fn high_pass(freq_hz: f64, q: f64) -> Self {
        let (cos_w0, alpha) = Self::trig(freq_hz, q);
        let half = f64::midpoint(1.0, cos_w0);
        Self::normalised(
            [half, -2.0 * half, half],
            [1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha],
        )
    }

    /// Band-pass with 0 dB gain at the centre frequency (constant peak gain form).
    pub fn band_pass(freq_hz: f64, q: f64) -> Self {
        let (cos_w0, alpha) = Self::trig(freq_hz, q);
        Self::normalised(
            [alpha, 0.0, -alpha],
            [1.0 + alpha, -2.0 * cos_w0, 1.0 - alpha],
        )
    }

    /// Peaking (bell) EQ with `gain_db` at the centre frequency.
    pub fn peaking(freq_hz: f64, q: f64, gain_db: f64) -> Self {
        let amplitude = db_to_linear(gain_db / 2.0);
        let (cos_w0, alpha) = Self::trig(freq_hz, q);
        Self::normalised(
            [
                1.0 + alpha * amplitude,
                -2.0 * cos_w0,
                1.0 - alpha * amplitude,
            ],
            [
                1.0 + alpha / amplitude,
                -2.0 * cos_w0,
                1.0 - alpha / amplitude,
            ],
        )
    }

    /// Filters one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        let output = self.b0 * input + self.z1;
        self.z1 = self.b1 * input - self.a1 * output + self.z2;
        self.z2 = self.b2 * input - self.a2 * output;
        if self.z1.abs() < DENORMAL_FLOOR {
            self.z1 = 0.0;
        }
        if self.z2.abs() < DENORMAL_FLOOR {
            self.z2 = 0.0;
        }
        output
    }

    /// Clears the filter state, keeping the coefficients.
    pub const fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Replaces the coefficients with those of `other`, keeping this section's state.
    pub const fn set_coefficients_from(&mut self, other: &Self) {
        self.b0 = other.b0;
        self.b1 = other.b1;
        self.b2 = other.b2;
        self.a1 = other.a1;
        self.a2 = other.a2;
    }

    /// Analytic magnitude response in dB at `freq_hz`.
    #[cfg(test)]
    pub fn magnitude_db(&self, freq_hz: f64) -> f64 {
        let w = TAU * freq_hz / sample_rate();
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let num_re = self.b0 + self.b1 * c1 + self.b2 * c2;
        let num_im = self.b1 * s1 + self.b2 * s2;
        let den_re = 1.0 + self.a1 * c1 + self.a2 * c2;
        let den_im = self.a1 * s1 + self.a2 * s2;
        10.0 * ((num_re * num_re + num_im * num_im) / (den_re * den_re + den_im * den_im)).log10()
    }
}

#[cfg(test)]
mod tests {
    use super::Biquad;
    use crate::test_util::{db, rms, sine};

    const Q_BUTTERWORTH: f64 = std::f64::consts::FRAC_1_SQRT_2;

    #[test]
    fn high_pass_80_hz_is_minus_3_db_at_cutoff_and_flat_above() {
        let filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        assert!((filter.magnitude_db(80.0) + 3.0103).abs() < 0.05);
        assert!(filter.magnitude_db(8_000.0).abs() < 0.01);
        assert!(filter.magnitude_db(20.0) < -22.0);
    }

    #[test]
    fn peaking_hits_the_requested_gain_at_centre_and_is_flat_far_away() {
        let cut = Biquad::peaking(250.0, 1.0, -3.0);
        assert!((cut.magnitude_db(250.0) + 3.0).abs() < 0.01);
        assert!(cut.magnitude_db(8_000.0).abs() < 0.1);
        let boost = Biquad::peaking(4_000.0, 0.9, 2.0);
        assert!((boost.magnitude_db(4_000.0) - 2.0).abs() < 0.01);
        assert!(boost.magnitude_db(100.0).abs() < 0.1);
    }

    #[test]
    fn band_pass_has_unity_gain_at_centre_and_rejects_low_frequencies() {
        let filter = Biquad::band_pass(6_708.0, 1.68);
        assert!(filter.magnitude_db(6_708.0).abs() < 0.01);
        assert!(filter.magnitude_db(1_000.0) < -15.0);
    }

    #[test]
    fn time_domain_output_matches_the_analytic_response() {
        let mut filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        let expected_db = filter.magnitude_db(100.0);
        let input = sine(100.0, 0.5, 48_000);
        let output: Vec<f64> = input.iter().map(|&x| filter.process(x)).collect();
        let measured_db = db(rms(&output[24_000..])) - db(rms(&input[24_000..]));
        assert!((measured_db - expected_db).abs() < 0.01);
    }

    #[test]
    fn identity_passes_samples_unchanged_and_reset_clears_state() {
        let mut identity = Biquad::identity();
        assert!((identity.process(0.25) - 0.25).abs() < f64::EPSILON);
        let mut filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        let first = filter.process(1.0);
        filter.process(0.5);
        filter.reset();
        assert!((filter.process(1.0) - first).abs() < f64::EPSILON);
    }

    #[test]
    fn state_decays_to_exact_zero_instead_of_denormals() {
        let mut filter = Biquad::high_pass(80.0, Q_BUTTERWORTH);
        filter.process(1.0);
        let mut last = 1.0;
        for _ in 0..2_000_000 {
            last = filter.process(0.0);
        }
        assert!(last == 0.0 || last.abs() >= f64::MIN_POSITIVE);
    }
}
