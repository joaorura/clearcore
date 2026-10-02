//! De-esser: attenuates only the 5-9 kHz band, and only when that band dominates the signal.
//!
//! A band-pass (5-9 kHz) feeds a peak envelope follower; a second follower tracks the broadband
//! signal. When `band_envelope / broadband_envelope` exceeds a relative threshold, the band
//! component (not the whole signal) is attenuated with a compressor-style law, up to
//! `max_reduction_db`. The output is `input - (1 - gain) * band`.

use crate::biquad::Biquad;
use crate::units::{db_to_linear, flush, linear_to_db, time_constant_coefficient};

/// Lower and upper edge of the sibilance band, in hertz.
pub const BAND_LOW_HZ: f64 = 5_000.0;
pub const BAND_HIGH_HZ: f64 = 9_000.0;

/// Envelope attack and release, in milliseconds (shared by the band and broadband followers so a
/// pure in-band tone reads a ratio of 1).
const ATTACK_MS: f64 = 1.0;
const RELEASE_MS: f64 = 20.0;

/// Below this broadband envelope (about -80 dBFS) the de-esser does nothing.
const ACTIVITY_FLOOR: f64 = 1.0e-4;

/// De-esser settings. `max_reduction_db == 0.0` disables the effect (exact bypass).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeEsserParams {
    /// Band-to-broadband envelope ratio above which attenuation starts (0..1).
    pub threshold_ratio: f64,
    /// Compression ratio applied to the excess (1 = none).
    pub ratio: f64,
    /// Largest attenuation of the band, in dB (>= 0).
    pub max_reduction_db: f64,
}

/// Band-limited de-esser.
#[derive(Clone, Copy, Debug)]
pub struct DeEsser {
    band: Biquad,
    params: DeEsserParams,
    band_envelope: f64,
    broadband_envelope: f64,
    attack: f64,
    release: f64,
}

/// One-pole peak follower with separate attack and release coefficients.
fn follow(envelope: f64, level: f64, attack: f64, release: f64) -> f64 {
    let coefficient = if level > envelope { attack } else { release };
    flush(level + coefficient * (envelope - level))
}

impl DeEsser {
    /// Creates a de-esser with cleared state.
    pub fn new(params: DeEsserParams) -> Self {
        let centre = (BAND_LOW_HZ * BAND_HIGH_HZ).sqrt();
        Self {
            band: Biquad::band_pass(centre, centre / (BAND_HIGH_HZ - BAND_LOW_HZ)),
            params,
            band_envelope: 0.0,
            broadband_envelope: 0.0,
            attack: time_constant_coefficient(ATTACK_MS),
            release: time_constant_coefficient(RELEASE_MS),
        }
    }

    /// Replaces the settings, keeping filter and envelope state (used for preset changes).
    pub const fn set_params(&mut self, params: DeEsserParams) {
        self.params = params;
    }

    /// Clears the filter and envelope state.
    pub const fn reset(&mut self) {
        self.band.reset();
        self.band_envelope = 0.0;
        self.broadband_envelope = 0.0;
    }

    /// Processes one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        let band = self.band.process(input);
        self.band_envelope = follow(self.band_envelope, band.abs(), self.attack, self.release);
        self.broadband_envelope = follow(
            self.broadband_envelope,
            input.abs(),
            self.attack,
            self.release,
        );
        input - (1.0 - self.band_gain()) * band
    }

    /// Linear gain to apply to the band component for the current envelopes.
    fn band_gain(&self) -> f64 {
        let params = &self.params;
        if params.max_reduction_db <= 0.0 || self.broadband_envelope < ACTIVITY_FLOOR {
            return 1.0;
        }
        let ratio = self.band_envelope / self.broadband_envelope;
        if ratio <= params.threshold_ratio {
            return 1.0;
        }
        let over_db = linear_to_db(ratio / params.threshold_ratio);
        let reduction_db =
            (over_db * (1.0 - 1.0 / params.ratio.max(1.0))).min(params.max_reduction_db);
        db_to_linear(-reduction_db)
    }
}

#[cfg(test)]
mod tests {
    use super::{DeEsser, DeEsserParams};
    use crate::test_util::{db, rms, sine};

    const STRONG: DeEsserParams = DeEsserParams {
        threshold_ratio: 0.5,
        ratio: 4.0,
        max_reduction_db: 9.0,
    };

    fn run(params: DeEsserParams, input: &[f64]) -> Vec<f64> {
        let mut deesser = DeEsser::new(params);
        input.iter().map(|&x| deesser.process(x)).collect()
    }

    fn gain_db_between(input: &[f64], output: &[f64]) -> f64 {
        let tail = input.len() / 2;
        db(rms(&output[tail..])) - db(rms(&input[tail..]))
    }

    #[test]
    fn attenuates_a_tone_inside_the_sibilance_band() {
        let input = sine(7_000.0, 0.1, 48_000);
        let output = run(STRONG, &input);
        // ratio ~ 1 against a 0.5 threshold: 6.02 dB over, 4:1 -> about -4.5 dB.
        let gain = gain_db_between(&input, &output);
        assert!((-5.5..=-3.5).contains(&gain), "gain {gain}");
    }

    #[test]
    fn leaves_a_low_frequency_tone_untouched() {
        let input = sine(1_000.0, 0.3, 48_000);
        let output = run(STRONG, &input);
        assert!(gain_db_between(&input, &output).abs() < 0.1);
    }

    #[test]
    fn leaves_weak_sibilance_under_a_loud_vowel_untouched() {
        let vowel = sine(200.0, 0.3, 48_000);
        let hiss = sine(7_000.0, 0.05, 48_000);
        let input: Vec<f64> = vowel.iter().zip(&hiss).map(|(a, b)| a + b).collect();
        let output = run(STRONG, &input);
        assert!(gain_db_between(&input, &output).abs() < 0.3);
    }

    #[test]
    fn reduction_never_exceeds_the_configured_maximum() {
        let params = DeEsserParams {
            max_reduction_db: 2.0,
            ..STRONG
        };
        let input = sine(7_000.0, 0.1, 48_000);
        let gain = gain_db_between(&input, &run(params, &input));
        assert!(gain >= -2.6, "gain {gain}");
    }

    #[test]
    fn zero_max_reduction_is_an_exact_bypass() {
        let params = DeEsserParams {
            max_reduction_db: 0.0,
            ..STRONG
        };
        let input = sine(7_000.0, 0.1, 4_800);
        let output = run(params, &input);
        assert!(
            input
                .iter()
                .zip(&output)
                .all(|(a, b)| a.to_bits() == b.to_bits())
        );
    }

    #[test]
    fn silence_stays_silent_and_finite() {
        let output = run(STRONG, &vec![0.0; 4_800]);
        assert!(output.iter().all(|sample| sample.abs() < f64::MIN_POSITIVE));
    }
}
