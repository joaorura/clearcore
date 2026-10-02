//! Lookahead peak limiter with a -1 dBFS ceiling.
//!
//! Let `D = LOOKAHEAD_SAMPLES`. For every input sample `x[k]` the limiter computes the gain
//! `g[k] = min(1, ceiling / |x[k]|)` needed to keep that sample under the ceiling, takes the
//! running minimum `m[j]` of `g` over the last `D + 1` samples, and averages `m` over the last
//! `D + 1` values. The output at time `n` is `x[n - D]` times that average (`s[n - D]`). Because
//! every `m[j]` with `n - D <= j <= n` is at most `g[n - D]`, the average is at most `g[n - D]`,
//! so `|y| <= ceiling` holds by construction, while the gain reaches its minimum linearly over the
//! `D` samples before the peak (no distortion-inducing step). A one-pole release then slows the
//! gain recovery; it only ever lowers the gain below that average, so the guarantee holds.

use crate::units::{db_to_linear, flush, time_constant_coefficient};

/// Lookahead, and therefore latency, in samples (2 ms at 48 kHz).
pub const LOOKAHEAD_SAMPLES: usize = 96;

/// Same value as [`LOOKAHEAD_SAMPLES`], typed `u32` for `StudioChain::latency_samples`.
pub const LATENCY_SAMPLES: u32 = 96;

const _: () = assert!(LOOKAHEAD_SAMPLES == LATENCY_SAMPLES as usize);

/// Output ceiling in dBFS.
pub const CEILING_DBFS: f64 = -1.0;

/// Ceiling as a linear amplitude in `f32`, rounded down so that `20 * log10(ceiling) <= -1.0`.
pub const CEILING_LINEAR_F32: f32 = 0.891_250_9;

/// Gain recovery time constant after a peak, in milliseconds.
const RELEASE_MS: f64 = 50.0;

const WINDOW: usize = LOOKAHEAD_SAMPLES + 1;

/// Mono lookahead limiter. Fixed size, no allocation.
#[derive(Clone, Copy, Debug)]
pub struct Limiter {
    ceiling: f64,
    release: f64,
    audio: [f64; WINDOW],
    required: [f64; WINDOW],
    minimum: [f64; WINDOW],
    position: usize,
    gain: f64,
}

impl Limiter {
    /// Creates a limiter with an empty (silent) delay line.
    pub fn new() -> Self {
        Self {
            ceiling: db_to_linear(CEILING_DBFS).min(f64::from(CEILING_LINEAR_F32)),
            release: time_constant_coefficient(RELEASE_MS),
            audio: [0.0; WINDOW],
            required: [1.0; WINDOW],
            minimum: [1.0; WINDOW],
            position: 0,
            gain: 1.0,
        }
    }

    /// Clears the delay line and the gain.
    pub const fn reset(&mut self) {
        self.audio = [0.0; WINDOW];
        self.required = [1.0; WINDOW];
        self.minimum = [1.0; WINDOW];
        self.position = 0;
        self.gain = 1.0;
    }

    /// Pushes one sample and returns the limited sample from `LOOKAHEAD_SAMPLES` ago. Non-finite
    /// input is treated as silence.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    pub fn process(&mut self, input: f64) -> f32 {
        let input = if input.is_finite() { input } else { 0.0 };
        let magnitude = input.abs();
        let required = if magnitude > self.ceiling {
            self.ceiling / magnitude
        } else {
            1.0
        };
        self.audio[self.position] = input;
        self.required[self.position] = required;
        let minimum = self.required.iter().copied().fold(1.0, f64::min);
        self.minimum[self.position] = minimum;
        let average = self.minimum.iter().sum::<f64>() / WINDOW as f64;

        self.gain = if average < self.gain {
            average
        } else {
            flush(self.gain + (average - self.gain) * (1.0 - self.release))
        };

        let oldest = (self.position + 1) % WINDOW;
        self.position = oldest;
        let delayed = self.audio[oldest];
        ((delayed * self.gain) as f32).clamp(-CEILING_LINEAR_F32, CEILING_LINEAR_F32)
    }
}

#[cfg(test)]
mod tests {
    use super::{CEILING_DBFS, CEILING_LINEAR_F32, LATENCY_SAMPLES, LOOKAHEAD_SAMPLES, Limiter};
    use crate::test_util::sine;
    use crate::units::{db_to_linear, linear_to_db};

    fn run(input: &[f64]) -> Vec<f32> {
        let mut limiter = Limiter::new();
        input.iter().map(|&x| limiter.process(x)).collect()
    }

    fn peak_db(output: &[f32]) -> f64 {
        let peak = output.iter().fold(0.0_f32, |acc, &x| acc.max(x.abs()));
        linear_to_db(f64::from(peak))
    }

    /// Deterministic pseudo-random samples in `[-amplitude, amplitude]` (xorshift64).
    #[allow(clippy::cast_precision_loss)]
    fn noise(len: usize, amplitude: f64) -> Vec<f64> {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        (0..len)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                amplitude * ((state >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn ceiling_constant_is_at_or_below_minus_one_dbfs() {
        assert!(linear_to_db(f64::from(CEILING_LINEAR_F32)) <= CEILING_DBFS);
    }

    #[test]
    fn latency_is_the_lookahead() {
        assert_eq!(LATENCY_SAMPLES, 96);
        assert_eq!(LOOKAHEAD_SAMPLES, 96);
    }

    #[test]
    fn plus_6_db_sine_never_exceeds_the_ceiling() {
        let output = run(&sine(1_000.0, db_to_linear(6.0), 48_000));
        assert!(
            peak_db(&output) <= CEILING_DBFS,
            "peak {}",
            peak_db(&output)
        );
        assert!(peak_db(&output) > -1.5, "limiter over-attenuates");
    }

    #[test]
    fn square_wave_noise_and_impulses_never_exceed_the_ceiling() {
        let square: Vec<f64> = (0..48_000)
            .map(|i| if (i / 24) % 2 == 0 { 2.0 } else { -2.0 })
            .collect();
        let impulses: Vec<f64> = (0..48_000)
            .map(|i| if i % 1_000 == 0 { 4.0 } else { 0.0 })
            .collect();
        for signal in [square, impulses, noise(96_000, 4.0)] {
            let output = run(&signal);
            assert!(
                peak_db(&output) <= CEILING_DBFS,
                "peak {}",
                peak_db(&output)
            );
        }
    }

    #[test]
    fn signal_below_the_ceiling_is_only_delayed() {
        let input = sine(1_000.0, 0.5, 9_600);
        let output = run(&input);
        for index in LOOKAHEAD_SAMPLES..input.len() {
            #[allow(clippy::cast_possible_truncation)]
            let expected = input[index - LOOKAHEAD_SAMPLES] as f32;
            assert!((output[index] - expected).abs() <= f32::EPSILON);
        }
    }

    #[test]
    fn an_impulse_emerges_exactly_one_lookahead_later() {
        let mut input = vec![0.0; 1_000];
        input[10] = 0.5;
        let output = run(&input);
        assert!((output[10 + LOOKAHEAD_SAMPLES] - 0.5).abs() <= f32::EPSILON);
        assert!(output[10 + LOOKAHEAD_SAMPLES - 1].abs() <= f32::EPSILON);
    }

    #[test]
    fn gain_recovers_after_a_loud_burst() {
        let mut input = sine(1_000.0, 2.0, 4_800);
        input.extend(sine(1_000.0, 0.1, 24_000));
        let output = run(&input);
        let tail_peak = output[output.len() - 2_000..]
            .iter()
            .fold(0.0_f32, |acc, &x| acc.max(x.abs()));
        assert!((tail_peak - 0.1).abs() < 0.002, "tail peak {tail_peak}");
    }

    #[test]
    fn gain_reduction_has_no_audible_steps() {
        let output = run(&sine(1_000.0, db_to_linear(6.0), 48_000));
        let largest_step = output
            .windows(2)
            .map(|pair| (pair[1] - pair[0]).abs())
            .fold(0.0_f32, f32::max);
        // A full-scale 1 kHz sine at the ceiling moves at most 0.89 * 2 * pi * 1000 / 48000.
        assert!(largest_step < 0.13, "largest step {largest_step}");
    }

    #[test]
    fn non_finite_input_yields_finite_output_under_the_ceiling() {
        let input = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.3, 1.0e300];
        let output = run(&input.repeat(100));
        assert!(output.iter().all(|x| x.is_finite()));
        assert!(peak_db(&output) <= CEILING_DBFS);
    }
}
