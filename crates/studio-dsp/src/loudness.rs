//! ITU-R BS.1770 loudness: K-weighting, a 400 ms momentary meter and an offline gated measurement.
//!
//! The K-weighting coefficients are the ones published in BS.1770 for a 48 kHz sample rate, which
//! is the only rate this crate supports.

use crate::{HOP_SAMPLES, biquad::Biquad};

/// Loudness reported for silence (and for an empty meter), in LUFS.
pub const SILENCE_LUFS: f64 = -120.0;

/// Number of 10 ms hops in the 400 ms momentary window.
pub const WINDOW_HOPS: usize = 40;

/// Constant of the BS.1770 loudness formula: `LUFS = -0.691 + 10 * log10(mean square)`.
const LOUDNESS_OFFSET_DB: f64 = -0.691;

/// Gating block length (400 ms) and step (100 ms, 75 % overlap) of the offline measurement.
const BLOCK_SAMPLES: usize = WINDOW_HOPS * HOP_SAMPLES;
const STEP_SAMPLES: usize = BLOCK_SAMPLES / 4;

/// Absolute gate of BS.1770, in LUFS, and the relative gate offset, in LU.
const ABSOLUTE_GATE_LUFS: f64 = -70.0;
const RELATIVE_GATE_LU: f64 = -10.0;

/// Converts a mean square of K-weighted samples to LUFS.
fn mean_square_to_lufs(mean_square: f64) -> f64 {
    if mean_square <= 0.0 || !mean_square.is_finite() {
        return SILENCE_LUFS;
    }
    (LOUDNESS_OFFSET_DB + 10.0 * mean_square.log10()).max(SILENCE_LUFS)
}

/// Converts LUFS back to the mean square that produces it.
fn lufs_to_mean_square(lufs: f64) -> f64 {
    10.0_f64.powf((lufs - LOUDNESS_OFFSET_DB) / 10.0)
}

/// BS.1770 K-weighting: a high-shelf pre-filter followed by the RLB high-pass (48 kHz).
#[derive(Clone, Copy, Debug)]
pub struct KWeighting {
    shelf: Biquad,
    high_pass: Biquad,
}

impl KWeighting {
    /// Creates a K-weighting filter with cleared state.
    pub const fn new() -> Self {
        Self {
            shelf: Biquad::from_coefficients(
                1.535_124_859_586_97,
                -2.691_696_189_406_38,
                1.198_392_810_852_85,
                -1.690_659_293_182_41,
                0.732_480_774_215_85,
            ),
            high_pass: Biquad::from_coefficients(
                1.0,
                -2.0,
                1.0,
                -1.990_047_454_833_98,
                0.990_072_250_366_21,
            ),
        }
    }

    /// Weights one sample.
    pub fn process(&mut self, input: f64) -> f64 {
        self.high_pass.process(self.shelf.process(input))
    }

    /// Clears the filter state.
    pub const fn reset(&mut self) {
        self.shelf.reset();
        self.high_pass.reset();
    }
}

impl Default for KWeighting {
    fn default() -> Self {
        Self::new()
    }
}

/// Streaming momentary loudness over the last 400 ms, updated once per hop. Fixed size, no
/// allocation.
#[derive(Clone, Copy, Debug)]
pub struct LoudnessMeter {
    filter: KWeighting,
    hop_energy: [f64; WINDOW_HOPS],
    next: usize,
    filled: usize,
}

impl LoudnessMeter {
    /// Creates an empty meter.
    pub const fn new() -> Self {
        Self {
            filter: KWeighting::new(),
            hop_energy: [0.0; WINDOW_HOPS],
            next: 0,
            filled: 0,
        }
    }

    /// Feeds one hop (any non-zero length) and slides the 400 ms window.
    #[allow(clippy::cast_precision_loss)]
    pub fn push_hop(&mut self, hop: &[f64]) {
        if hop.is_empty() {
            return;
        }
        let mut energy = 0.0;
        for &sample in hop {
            let weighted = self.filter.process(sample);
            energy += weighted * weighted;
        }
        self.hop_energy[self.next] = energy / hop.len() as f64;
        self.next = (self.next + 1) % WINDOW_HOPS;
        self.filled = (self.filled + 1).min(WINDOW_HOPS);
    }

    /// Momentary loudness in LUFS over the hops seen so far (at most 400 ms);
    /// [`SILENCE_LUFS`] when empty or silent.
    #[allow(clippy::cast_precision_loss)]
    pub fn momentary_lufs(&self) -> f64 {
        if self.filled == 0 {
            return SILENCE_LUFS;
        }
        let total: f64 = self.hop_energy.iter().take(self.filled).sum();
        mean_square_to_lufs(total / self.filled as f64)
    }

    /// Clears the window and the filter state.
    pub const fn reset(&mut self) {
        self.filter.reset();
        self.hop_energy = [0.0; WINDOW_HOPS];
        self.next = 0;
        self.filled = 0;
    }
}

impl Default for LoudnessMeter {
    fn default() -> Self {
        Self::new()
    }
}

/// Integrated loudness of a whole signal with the BS.1770 two-stage gating (absolute -70 LUFS,
/// relative -10 LU). **Offline only**: it allocates, so it must never run on the audio thread.
#[allow(clippy::cast_precision_loss)]
pub fn integrated_lufs(signal: &[f32]) -> f64 {
    let mut filter = KWeighting::new();
    let energy: Vec<f64> = signal
        .iter()
        .map(|&sample| {
            let weighted = filter.process(f64::from(sample));
            weighted * weighted
        })
        .collect();

    let mut blocks = Vec::new();
    let mut start = 0;
    while start + BLOCK_SAMPLES <= energy.len() {
        let block = &energy[start..start + BLOCK_SAMPLES];
        blocks.push(block.iter().sum::<f64>() / BLOCK_SAMPLES as f64);
        start += STEP_SAMPLES;
    }

    let mean_above = |threshold: f64| -> Option<f64> {
        let kept: Vec<f64> = blocks
            .iter()
            .copied()
            .filter(|&block| block > threshold)
            .collect();
        if kept.is_empty() {
            None
        } else {
            Some(kept.iter().sum::<f64>() / kept.len() as f64)
        }
    };

    let Some(after_absolute) = mean_above(lufs_to_mean_square(ABSOLUTE_GATE_LUFS)) else {
        return SILENCE_LUFS;
    };
    let relative_gate = mean_square_to_lufs(after_absolute) + RELATIVE_GATE_LU;
    mean_above(lufs_to_mean_square(relative_gate)).map_or(SILENCE_LUFS, mean_square_to_lufs)
}

#[cfg(test)]
mod tests {
    use super::{LoudnessMeter, SILENCE_LUFS, integrated_lufs};
    use crate::HOP_SAMPLES;
    use crate::test_util::sine;
    use crate::units::db_to_linear;

    /// Peak level in dBFS of a sine whose BS.1770 loudness at 1 kHz is `lufs`
    /// (a sine's mean square is half the squared peak: -3.0103 dB).
    fn tone_peak_db_for(lufs: f64) -> f64 {
        lufs + 3.0103
    }

    fn meter_reading(signal: &[f64]) -> f64 {
        let mut meter = LoudnessMeter::new();
        for hop in signal.chunks_exact(HOP_SAMPLES) {
            meter.push_hop(hop);
        }
        meter.momentary_lufs()
    }

    fn to_f32(signal: &[f64]) -> Vec<f32> {
        #[allow(clippy::cast_possible_truncation)]
        signal.iter().map(|&sample| sample as f32).collect()
    }

    #[test]
    fn calibrated_tone_at_minus_23_lufs_measures_minus_23_on_the_meter() {
        let amplitude = db_to_linear(tone_peak_db_for(-23.0));
        let signal = sine(1_000.0, amplitude, 48_000 * 2);
        assert!((meter_reading(&signal) + 23.0).abs() < 0.1);
    }

    #[test]
    fn calibrated_tone_at_minus_23_lufs_measures_minus_23_integrated() {
        let amplitude = db_to_linear(tone_peak_db_for(-23.0));
        let signal = to_f32(&sine(1_000.0, amplitude, 48_000 * 5));
        assert!((integrated_lufs(&signal) + 23.0).abs() < 0.1);
    }

    #[test]
    fn full_scale_997_hz_sine_reads_minus_3_01_lufs() {
        let signal = sine(997.0, 1.0, 48_000 * 2);
        assert!((meter_reading(&signal) + 3.01).abs() < 0.05);
    }

    #[test]
    fn k_weighting_attenuates_deep_bass_and_lifts_the_top_octave() {
        let amplitude = 0.1;
        let reference = meter_reading(&sine(1_000.0, amplitude, 48_000 * 2));
        let bass = meter_reading(&sine(20.0, amplitude, 48_000 * 4));
        let treble = meter_reading(&sine(10_000.0, amplitude, 48_000 * 2));
        assert!(bass < reference - 10.0, "bass {bass} reference {reference}");
        assert!(
            treble > reference + 3.0,
            "treble {treble} reference {reference}"
        );
    }

    #[test]
    fn empty_and_silent_meters_report_the_silence_floor() {
        let meter = LoudnessMeter::new();
        assert!((meter.momentary_lufs() - SILENCE_LUFS).abs() < f64::EPSILON);
        let silence = vec![0.0; HOP_SAMPLES * 10];
        assert!((meter_reading(&silence) - SILENCE_LUFS).abs() < f64::EPSILON);
    }

    #[test]
    fn window_forgets_audio_older_than_400_ms() {
        let mut meter = LoudnessMeter::new();
        let loud = sine(1_000.0, 0.5, HOP_SAMPLES * 40);
        for hop in loud.chunks_exact(HOP_SAMPLES) {
            meter.push_hop(hop);
        }
        assert!(meter.momentary_lufs() > -20.0);
        let silence = [0.0; HOP_SAMPLES];
        for _ in 0..40 {
            meter.push_hop(&silence);
        }
        // The K-weighting tail of the first silent hop is the only energy left in the window.
        assert!(meter.momentary_lufs() < -45.0);
    }

    #[test]
    fn reset_returns_the_meter_to_its_initial_state() {
        let mut meter = LoudnessMeter::new();
        meter.push_hop(&[0.5; HOP_SAMPLES]);
        meter.reset();
        assert!((meter.momentary_lufs() - SILENCE_LUFS).abs() < f64::EPSILON);
    }

    #[test]
    fn integrated_measurement_ignores_blocks_below_the_gates() {
        let loud = sine(1_000.0, db_to_linear(tone_peak_db_for(-23.0)), 48_000 * 10);
        let quiet = sine(1_000.0, db_to_linear(-100.0), 48_000 * 10);
        let mut signal = to_f32(&loud);
        signal.extend(to_f32(&quiet));
        assert!((integrated_lufs(&signal) + 23.0).abs() < 0.1);
        assert!((integrated_lufs(&vec![0.0; 48_000]) - SILENCE_LUFS).abs() < f64::EPSILON);
    }
}
