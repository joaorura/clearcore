//! Feed-forward compressor.
//!
//! The detector is a mean-square follower with a fixed 5 ms time constant, reported as the
//! peak-equivalent level of a sine (`10 log10(ms) + 3.0103`), so a sine of amplitude A reads A.
//! The gain computer is the textbook static curve `L -> T + (L - T) / R` above the threshold `T`,
//! and attack/release smooth the resulting gain (in dB). Makeup is added after smoothing.

use crate::units::{db_to_linear, flush, time_constant_coefficient};

/// Detector time constant, in milliseconds.
const DETECTOR_MS: f64 = 5.0;

/// Peak-equivalent offset of a sine: `10 * log10(2)`.
const SINE_PEAK_OFFSET_DB: f64 = 3.010_299_956_639_812;

/// Compressor settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressorParams {
    /// Level above which compression starts, in dB (peak-equivalent).
    pub threshold_db: f64,
    /// Compression ratio R in `R:1` (>= 1).
    pub ratio: f64,
    /// Time to bite into rising level, in milliseconds.
    pub attack_ms: f64,
    /// Time to let go after the level falls, in milliseconds.
    pub release_ms: f64,
    /// Gain added after compression, in dB.
    pub makeup_db: f64,
}

/// Static gain, in dB, applied to a signal at `level_db`: `T + (L - T) / R - L` above the
/// threshold, `0` at or below it.
pub fn static_gain_db(level_db: f64, threshold_db: f64, ratio: f64) -> f64 {
    if level_db > threshold_db {
        threshold_db + (level_db - threshold_db) / ratio.max(1.0) - level_db
    } else {
        0.0
    }
}

/// Mono compressor.
#[derive(Clone, Copy, Debug)]
pub struct Compressor {
    params: CompressorParams,
    detector: f64,
    attack: f64,
    release: f64,
    mean_square: f64,
    gain_db: f64,
}

impl Compressor {
    /// Creates a compressor with cleared state.
    pub fn new(params: CompressorParams) -> Self {
        Self {
            params,
            detector: time_constant_coefficient(DETECTOR_MS),
            attack: time_constant_coefficient(params.attack_ms),
            release: time_constant_coefficient(params.release_ms),
            mean_square: 0.0,
            gain_db: 0.0,
        }
    }

    /// Replaces the settings, keeping detector and gain state (used for preset changes).
    pub fn set_params(&mut self, params: CompressorParams) {
        self.params = params;
        self.attack = time_constant_coefficient(params.attack_ms);
        self.release = time_constant_coefficient(params.release_ms);
    }

    /// Clears the detector and gain state.
    pub const fn reset(&mut self) {
        self.mean_square = 0.0;
        self.gain_db = 0.0;
    }

    /// Current smoothed gain reduction in dB, without makeup (<= 0).
    #[cfg(test)]
    pub const fn gain_db(&self) -> f64 {
        self.gain_db
    }

    /// Processes one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        self.mean_square =
            flush(self.detector * self.mean_square + (1.0 - self.detector) * input * input);
        let level_db = 10.0 * self.mean_square.max(1.0e-20).log10() + SINE_PEAK_OFFSET_DB;
        let target_db = static_gain_db(level_db, self.params.threshold_db, self.params.ratio);
        let coefficient = if target_db < self.gain_db {
            self.attack
        } else {
            self.release
        };
        self.gain_db = flush(coefficient * self.gain_db + (1.0 - coefficient) * target_db);
        input * db_to_linear(self.gain_db + self.params.makeup_db)
    }
}

#[cfg(test)]
mod tests {
    use super::{Compressor, CompressorParams, static_gain_db};
    use crate::test_util::{db, rms, sine};
    use crate::units::db_to_linear;

    const PARAMS: CompressorParams = CompressorParams {
        threshold_db: -20.0,
        ratio: 4.0,
        attack_ms: 10.0,
        release_ms: 100.0,
        makeup_db: 0.0,
    };

    fn run(params: CompressorParams, input: &[f64]) -> Vec<f64> {
        let mut compressor = Compressor::new(params);
        input.iter().map(|&x| compressor.process(x)).collect()
    }

    fn settled_gain_db(input: &[f64], output: &[f64]) -> f64 {
        let tail = input.len() * 3 / 4;
        db(rms(&output[tail..])) - db(rms(&input[tail..]))
    }

    #[test]
    fn static_curve_matches_the_formula() {
        // L = -10, T = -20, R = 4: T + (L - T) / R = -17.5 -> gain -7.5 dB.
        assert!((static_gain_db(-10.0, -20.0, 4.0) + 7.5).abs() < 1.0e-12);
        assert!(static_gain_db(-30.0, -20.0, 4.0).abs() < 1.0e-12);
        assert!(static_gain_db(-20.0, -20.0, 4.0).abs() < 1.0e-12);
        assert!(static_gain_db(-5.0, -20.0, 1.0).abs() < 1.0e-12);
    }

    #[test]
    fn steady_tone_gain_matches_the_static_curve() {
        let input = sine(1_000.0, db_to_linear(-10.0), 48_000);
        let output = run(PARAMS, &input);
        let gain = settled_gain_db(&input, &output);
        assert!((gain + 7.5).abs() < 0.25, "gain {gain}");
    }

    #[test]
    fn tone_below_threshold_is_not_compressed() {
        let input = sine(1_000.0, db_to_linear(-40.0), 48_000);
        let gain = settled_gain_db(&input, &run(PARAMS, &input));
        assert!(gain.abs() < 0.01, "gain {gain}");
    }

    #[test]
    fn makeup_gain_is_added_after_compression() {
        let params = CompressorParams {
            makeup_db: 3.0,
            ..PARAMS
        };
        let input = sine(1_000.0, db_to_linear(-10.0), 48_000);
        let gain = settled_gain_db(&input, &run(params, &input));
        assert!((gain + 4.5).abs() < 0.25, "gain {gain}");
    }

    #[test]
    fn attack_is_gradual_and_release_returns_to_unity() {
        let mut compressor = Compressor::new(PARAMS);
        let loud = sine(1_000.0, db_to_linear(-10.0), 48_000);
        let mut at_1ms = 0.0;
        let mut at_100ms = 0.0;
        for (index, &sample) in loud.iter().enumerate() {
            compressor.process(sample);
            if index == 47 {
                at_1ms = compressor.gain_db();
            }
            if index == 4_799 {
                at_100ms = compressor.gain_db();
            }
        }
        let settled = compressor.gain_db();
        assert!(at_1ms > at_100ms && at_100ms > settled - 0.5);
        assert!(at_1ms > -1.0, "1 ms gain {at_1ms}");
        for _ in 0..48_000 {
            compressor.process(0.0);
        }
        assert!(compressor.gain_db() > -0.1);
    }

    #[test]
    fn changing_parameters_keeps_the_smoothed_gain() {
        let mut compressor = Compressor::new(PARAMS);
        for &sample in &sine(1_000.0, db_to_linear(-10.0), 48_000) {
            compressor.process(sample);
        }
        let before = compressor.gain_db();
        compressor.set_params(CompressorParams {
            ratio: 3.0,
            ..PARAMS
        });
        assert!((compressor.gain_db() - before).abs() < f64::EPSILON);
    }
}
