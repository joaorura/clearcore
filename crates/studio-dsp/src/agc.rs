//! Slow loudness AGC steering the signal towards -16 LUFS.
//!
//! Once per hop the 400 ms momentary loudness (BS.1770 K-weighting) of the incoming signal is
//! measured, the desired gain `target - loudness` is clamped to `[MIN_GAIN_DB, MAX_GAIN_DB]`, and
//! the applied gain moves towards it with a slow one-pole (`TAU_UP_S` when raising, `TAU_DOWN_S`
//! when lowering). Below `FREEZE_BELOW_LUFS` the gain is frozen, so pauses and noise floor are not
//! amplified. Inside a hop the gain ramps linearly from the previous to the new value, so there
//! are no steps. All values are documented starting points to be tuned by ear.

use crate::HOP_SAMPLES;
use crate::loudness::LoudnessMeter;
use crate::units::{db_to_linear, sample_rate};

/// Loudness target for standard presets, in LUFS.
pub const TARGET_LUFS: f64 = -16.0;
/// Largest boost and largest cut, in dB.
pub const MAX_GAIN_DB: f64 = 12.0;
pub const MIN_GAIN_DB: f64 = -12.0;
/// Momentary loudness below which the gain is frozen, in LUFS.
pub const FREEZE_BELOW_LUFS: f64 = -50.0;
/// Time constants of the gain, in seconds.
pub const TAU_UP_S: f64 = 3.0;
pub const TAU_DOWN_S: f64 = 1.0;

/// Per-hop smoothing coefficient for a time constant in seconds.
#[allow(clippy::cast_precision_loss)]
fn hop_coefficient(tau_seconds: f64) -> f64 {
    (-(HOP_SAMPLES as f64) / (tau_seconds * sample_rate())).exp()
}

/// Loudness AGC. Fixed size, no allocation.
#[derive(Clone, Copy, Debug)]
pub struct Agc {
    meter: LoudnessMeter,
    gain_db: f64,
    intensity: u8,
    up: f64,
    down: f64,
}

impl Default for Agc {
    fn default() -> Self {
        Self::new()
    }
}

impl Agc {
    /// Target LUFS for a given intensity percentage (1..=100).
    /// At 50%: -16.0 LUFS. At 100%: -12.0 LUFS. At 0%: -20.0 LUFS.
    #[must_use]
    pub fn target_lufs_for_intensity(intensity: u8) -> f64 {
        let clamped = f64::from(intensity.min(100));
        TARGET_LUFS + (clamped - 50.0) * 0.08
    }

    /// Creates an AGC with unity gain, empty loudness window, and default intensity 50 (-16.0 LUFS).
    pub fn new() -> Self {
        Self::with_intensity(50)
    }

    /// Creates an AGC with specified intensity (0–100).
    pub fn with_intensity(intensity: u8) -> Self {
        Self {
            meter: LoudnessMeter::new(),
            gain_db: 0.0,
            intensity: intensity.min(100),
            up: hop_coefficient(TAU_UP_S),
            down: hop_coefficient(TAU_DOWN_S),
        }
    }

    /// Sets the AGC intensity (0–100). 0 is bypass, 50 is balanced (-16 LUFS), 100 is maximum (-12 LUFS).
    pub fn set_intensity(&mut self, intensity: u8) {
        self.intensity = intensity.min(100);
    }

    /// Returns the current AGC intensity (0–100).
    #[allow(dead_code)]
    #[must_use]
    pub const fn intensity(&self) -> u8 {
        self.intensity
    }

    /// Resets the gain to unity and clears the loudness window.
    pub const fn reset(&mut self) {
        self.meter.reset();
        self.gain_db = 0.0;
    }

    /// Current gain in dB.
    #[cfg(test)]
    pub const fn gain_db(&self) -> f64 {
        self.gain_db
    }

    /// Measures `hop`, updates the gain and applies it in place with a linear ramp.
    #[allow(clippy::cast_precision_loss)]
    pub fn process_hop(&mut self, hop: &mut [f64; HOP_SAMPLES]) {
        if self.intensity == 0 && self.gain_db.abs() < 1e-4 {
            self.gain_db = 0.0;
            return;
        }

        self.meter.push_hop(hop);
        let loudness = self.meter.momentary_lufs();
        let previous_gain = db_to_linear(self.gain_db);

        if self.intensity == 0 {
            let desired = 0.0;
            let coefficient = if desired < self.gain_db {
                self.down
            } else {
                self.up
            };
            self.gain_db = coefficient * self.gain_db + (1.0 - coefficient) * desired;
            if self.gain_db.abs() < 1e-4 {
                self.gain_db = 0.0;
            }
        } else if loudness >= FREEZE_BELOW_LUFS {
            let target = Self::target_lufs_for_intensity(self.intensity);
            let desired = (target - loudness).clamp(MIN_GAIN_DB, MAX_GAIN_DB);
            let coefficient = if desired < self.gain_db {
                self.down
            } else {
                self.up
            };
            self.gain_db = coefficient * self.gain_db + (1.0 - coefficient) * desired;
        }

        let next_gain = db_to_linear(self.gain_db);
        for (index, sample) in hop.iter_mut().enumerate() {
            let fraction = (index + 1) as f64 / HOP_SAMPLES as f64;
            *sample *= previous_gain + (next_gain - previous_gain) * fraction;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Agc, MAX_GAIN_DB, MIN_GAIN_DB, TARGET_LUFS};
    use crate::HOP_SAMPLES;
    use crate::loudness::LoudnessMeter;
    use crate::test_util::sine;
    use crate::units::db_to_linear;

    /// Sine amplitude whose BS.1770 loudness at 1 kHz is `lufs` (peak = rms + 3.0103 dB).
    fn amplitude_for(lufs: f64) -> f64 {
        db_to_linear(lufs + 3.0103)
    }

    /// Runs `seconds` of a 1 kHz tone at `lufs` through the AGC and returns the output.
    fn run_tone(agc: &mut Agc, lufs: f64, seconds: usize) -> Vec<f64> {
        let input = sine(1_000.0, amplitude_for(lufs), 48_000 * seconds);
        run(agc, &input)
    }

    fn run(agc: &mut Agc, input: &[f64]) -> Vec<f64> {
        let mut output = Vec::with_capacity(input.len());
        for chunk in input.chunks_exact(HOP_SAMPLES) {
            let mut hop = [0.0; HOP_SAMPLES];
            hop.copy_from_slice(chunk);
            agc.process_hop(&mut hop);
            output.extend_from_slice(&hop);
        }
        output
    }

    fn lufs_of_tail(signal: &[f64]) -> f64 {
        let mut meter = LoudnessMeter::new();
        for hop in signal[signal.len() - 48_000..].chunks_exact(HOP_SAMPLES) {
            meter.push_hop(hop);
        }
        meter.momentary_lufs()
    }

    #[test]
    fn quiet_tone_is_raised_to_the_target() {
        let mut agc = Agc::new();
        let output = run_tone(&mut agc, -26.0, 40);
        assert!((lufs_of_tail(&output) - TARGET_LUFS).abs() < 0.5);
        assert!((agc.gain_db() - 10.0).abs() < 0.5);
    }

    #[test]
    fn loud_tone_is_lowered_to_the_target() {
        let mut agc = Agc::new();
        let output = run_tone(&mut agc, -8.0, 30);
        assert!((lufs_of_tail(&output) - TARGET_LUFS).abs() < 0.5);
    }

    #[test]
    fn boost_is_capped_at_the_maximum_gain() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -40.0, 60);
        assert!(
            (agc.gain_db() - MAX_GAIN_DB).abs() < 0.05,
            "{}",
            agc.gain_db()
        );
    }

    #[test]
    fn cut_is_capped_at_the_minimum_gain() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -2.0, 40);
        assert!(
            (agc.gain_db() - MIN_GAIN_DB).abs() < 0.05,
            "{}",
            agc.gain_db()
        );
    }

    #[test]
    fn gain_moves_slowly() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -26.0, 1);
        // One second into a 10 dB correction with a 3 s time constant: well under 4 dB.
        assert!(
            agc.gain_db() > 0.0 && agc.gain_db() < 4.0,
            "{}",
            agc.gain_db()
        );
    }

    #[test]
    fn gain_is_frozen_during_silence() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -26.0, 20);
        let silence = vec![0.0; 48_000];
        run(&mut agc, &silence);
        let after_one_second = agc.gain_db();
        for _ in 0..4 {
            run(&mut agc, &silence);
        }
        assert!((agc.gain_db() - after_one_second).abs() < 1.0e-12);
        assert!((agc.gain_db() - 10.0).abs() < 0.5, "{}", agc.gain_db());
    }

    #[test]
    fn first_hop_does_not_jump_in_level() {
        let mut fresh = Agc::new();
        let input = sine(1_000.0, amplitude_for(-26.0), HOP_SAMPLES);
        let output = run(&mut fresh, &input);
        let in_peak = input.iter().fold(0.0_f64, |acc, x| acc.max(x.abs()));
        let out_peak = output.iter().fold(0.0_f64, |acc, x| acc.max(x.abs()));
        assert!(out_peak / in_peak < db_to_linear(0.1));
    }

    #[test]
    fn reset_restores_unity_gain() {
        let mut agc = Agc::new();
        run_tone(&mut agc, -26.0, 5);
        agc.reset();
        assert!(agc.gain_db().abs() < f64::EPSILON);
    }

    #[test]
    fn intensity_zero_acts_as_bypass() {
        let mut agc = Agc::with_intensity(0);
        let input = sine(1_000.0, amplitude_for(-26.0), HOP_SAMPLES * 5);
        let output = run(&mut agc, &input);
        assert_eq!(agc.gain_db(), 0.0);
        assert_eq!(input, output);
    }

    #[test]
    fn intensity_hundred_steers_to_minus_twelve_lufs() {
        let mut agc = Agc::with_intensity(100);
        assert_eq!(Agc::target_lufs_for_intensity(100), -12.0);
        let output = run_tone(&mut agc, -20.0, 40);
        assert!((lufs_of_tail(&output) - (-12.0)).abs() < 0.5);
    }
}
