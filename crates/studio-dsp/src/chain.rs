//! The studio chain: high-pass, fixed EQ, de-esser, compressor, AGC and limiter, with a click-free
//! preset switch.
//!
//! Numeric safety: whenever the chain is active (or fading to/from `Off`) the input is sanitised
//! first: non-finite samples and denormals become `0.0`. In steady `Off` the chain returns before
//! touching a single sample, so `Off` is bit-exact even for NaN or infinity; rejecting those is the
//! caller's job (`ProcessedFrame::checked`).
//!
//! Preset switch: two [`Path`]s run side by side for exactly one hop. The old path keeps its state;
//! the new path starts from a copy of that state with the new parameters (or from scratch when
//! coming from `Off`). The first [`LATENCY_SAMPLES`] samples of the hop stay on the old signal
//! (a fresh path's delay line is still empty there), then the output ramps linearly to the new
//! signal, reaching it on the last sample. Both signals are continuous and the weight is
//! continuous, so there is no click.

use crate::HOP_SAMPLES;
use crate::agc::Agc;
use crate::biquad::Biquad;
use crate::compressor::Compressor;
use crate::deesser::DeEsser;
use crate::limiter::{LATENCY_SAMPLES, LOOKAHEAD_SAMPLES, Limiter};
use crate::params::{EqBand, PresetParams, for_preset};
use crate::preset::Preset;
use std::f64::consts::FRAC_1_SQRT_2;

/// High-pass corner in hertz (second-order Butterworth).
const HIGH_PASS_HZ: f64 = 80.0;

/// Length of the part of the crossfade that actually ramps, in samples.
#[allow(clippy::cast_precision_loss)]
const RAMP_SAMPLES: f32 = (HOP_SAMPLES - LOOKAHEAD_SAMPLES) as f32;

fn bell(band: &EqBand) -> Biquad {
    Biquad::peaking(band.freq_hz, band.q, band.gain_db)
}

/// Replaces non-finite and denormal samples with zero.
fn sanitize(hop: &mut [f32; HOP_SAMPLES]) {
    for sample in hop.iter_mut() {
        if !(sample.is_finite() && sample.abs() >= f32::MIN_POSITIVE) {
            *sample = 0.0;
        }
    }
}

/// One complete signal path for one preset.
#[derive(Clone, Copy, Debug)]
struct Path {
    high_pass: Biquad,
    low_mid_cut: Biquad,
    presence: Biquad,
    deesser: DeEsser,
    compressor: Compressor,
    agc: Agc,
    limiter: Limiter,
}

impl Path {
    fn new(params: &PresetParams) -> Self {
        Self {
            high_pass: Biquad::high_pass(HIGH_PASS_HZ, FRAC_1_SQRT_2),
            low_mid_cut: bell(&params.low_mid_cut),
            presence: bell(&params.presence),
            deesser: DeEsser::new(params.deesser),
            compressor: Compressor::new(params.compressor),
            agc: Agc::new(),
            limiter: Limiter::new(),
        }
    }

    /// A copy of this path (same filter, envelope, AGC and limiter state) with new parameters.
    fn with_params(&self, params: &PresetParams) -> Self {
        let mut path = *self;
        path.low_mid_cut
            .set_coefficients_from(&bell(&params.low_mid_cut));
        path.presence.set_coefficients_from(&bell(&params.presence));
        path.deesser.set_params(params.deesser);
        path.compressor.set_params(params.compressor);
        path
    }

    fn set_leveler_intensity(&mut self, intensity: u8) {
        self.agc.set_intensity(intensity);
    }

    const fn reset(&mut self) {
        self.high_pass.reset();
        self.low_mid_cut.reset();
        self.presence.reset();
        self.deesser.reset();
        self.compressor.reset();
        self.agc.reset();
        self.limiter.reset();
    }

    /// Processes one sanitised hop in place.
    fn process(&mut self, hop: &mut [f32; HOP_SAMPLES]) {
        let mut scratch = [0.0_f64; HOP_SAMPLES];
        for (slot, &sample) in scratch.iter_mut().zip(hop.iter()) {
            let mut value = self.high_pass.process(f64::from(sample));
            value = self.low_mid_cut.process(value);
            value = self.presence.process(value);
            value = self.deesser.process(value);
            *slot = self.compressor.process(value);
        }
        self.agc.process_hop(&mut scratch);
        for (out, &value) in hop.iter_mut().zip(scratch.iter()) {
            *out = self.limiter.process(value);
        }
    }
}

/// The studio DSP chain for one mono 48 kHz stream.
#[derive(Clone, Debug)]
pub struct StudioChain {
    target: Preset,
    leveler_intensity: u8,
    explicit_leveler: bool,
    main: Option<Path>,
    outgoing: Option<Path>,
    fading: bool,
    standalone_agc: Agc,
    standalone_limiter: Limiter,
}

impl StudioChain {
    /// Creates a chain running `preset` from the first hop.
    #[must_use]
    pub fn new(preset: Preset) -> Self {
        let leveler_intensity = match preset {
            Preset::Off => 0,
            _ => 50,
        };
        let mut chain = Self {
            target: preset,
            leveler_intensity,
            explicit_leveler: false,
            main: for_preset(preset).map(Path::new),
            outgoing: None,
            fading: false,
            standalone_agc: Agc::with_intensity(leveler_intensity),
            standalone_limiter: Limiter::new(),
        };
        if let Some(path) = chain.main.as_mut() {
            path.set_leveler_intensity(leveler_intensity);
        }
        chain
    }

    /// Sets the voice auto-leveler intensity (0–100).
    /// 0 is bypass, 50 is balanced (-16 LUFS), 100 is maximum (-12 LUFS).
    pub fn set_leveler_intensity(&mut self, intensity: u8) {
        let intensity = intensity.min(100);
        self.leveler_intensity = intensity;
        self.explicit_leveler = true;
        if let Some(path) = self.main.as_mut() {
            path.set_leveler_intensity(intensity);
        }
        self.standalone_agc.set_intensity(intensity);
    }

    /// Returns the current voice auto-leveler intensity (0–100).
    #[must_use]
    pub const fn leveler_intensity(&self) -> u8 {
        self.leveler_intensity
    }

    /// Switches preset. The change is rendered with a one-hop crossfade (about 10 ms) on the next
    /// [`StudioChain::process`] call. Selecting the current preset is a no-op.
    pub fn set_preset(&mut self, preset: Preset) {
        if !self.fading && preset == self.target {
            return;
        }
        if !self.fading {
            self.outgoing = self.main.take();
            self.fading = true;
        }
        self.target = preset;
        if !self.explicit_leveler {
            self.leveler_intensity = match preset {
                Preset::Off => 0,
                _ => 50,
            };
            self.standalone_agc.set_intensity(self.leveler_intensity);
        }
        self.main = for_preset(preset).map(|params| {
            let mut path = self
                .outgoing
                .as_ref()
                .map_or_else(|| Path::new(params), |old| old.with_params(params));
            path.set_leveler_intensity(self.leveler_intensity);
            path
        });
        if self.main.is_none() && self.outgoing.is_none() {
            // Settled on `Off` with nothing audible to fade out (for example Off -> X -> Off before
            // the next hop): drop the crossfade so `Off` stays bit-exact.
            self.fading = false;
        }
    }

    /// Processes one hop in place: no allocation, no panic.
    pub fn process(&mut self, hop: &mut [f32; HOP_SAMPLES]) {
        if !self.fading && self.main.is_none() && self.leveler_intensity == 0 {
            return;
        }
        sanitize(hop);
        if !self.fading {
            if let Some(path) = self.main.as_mut() {
                path.process(hop);
            } else if self.leveler_intensity > 0 {
                let mut scratch = [0.0_f64; HOP_SAMPLES];
                for (slot, &sample) in scratch.iter_mut().zip(hop.iter()) {
                    *slot = f64::from(sample);
                }
                self.standalone_agc.process_hop(&mut scratch);
                for (out, &value) in hop.iter_mut().zip(scratch.iter()) {
                    *out = self.standalone_limiter.process(value);
                }
            }
            return;
        }

        let dry = *hop;
        let mut old = dry;
        if let Some(path) = self.outgoing.as_mut() {
            path.process(&mut old);
        } else if self.leveler_intensity > 0 {
            let mut scratch = [0.0_f64; HOP_SAMPLES];
            for (slot, &sample) in scratch.iter_mut().zip(old.iter()) {
                *slot = f64::from(sample);
            }
            self.standalone_agc.process_hop(&mut scratch);
            for (out, &value) in old.iter_mut().zip(scratch.iter()) {
                *out = self.standalone_limiter.process(value);
            }
        }
        let mut new = dry;
        if let Some(path) = self.main.as_mut() {
            path.process(&mut new);
        } else if self.leveler_intensity > 0 {
            let mut scratch = [0.0_f64; HOP_SAMPLES];
            for (slot, &sample) in scratch.iter_mut().zip(new.iter()) {
                *slot = f64::from(sample);
            }
            self.standalone_agc.process_hop(&mut scratch);
            for (out, &value) in new.iter_mut().zip(scratch.iter()) {
                *out = self.standalone_limiter.process(value);
            }
        }
        self.outgoing = None;
        self.fading = false;

        for (index, ((out, &old_sample), &new_sample)) in
            hop.iter_mut().zip(old.iter()).zip(new.iter()).enumerate()
        {
            let ramp = index.saturating_sub(LOOKAHEAD_SAMPLES - 1);
            #[allow(clippy::cast_precision_loss)]
            let weight = (ramp as f32 / RAMP_SAMPLES).min(1.0);
            *out = old_sample * (1.0 - weight) + new_sample * weight;
        }
    }

    /// Latency in samples: the limiter lookahead (96 = 2 ms). It is a constant of the contract and
    /// is reported even while the preset is `Off`.
    #[must_use]
    pub const fn latency_samples(&self) -> u32 {
        LATENCY_SAMPLES
    }

    /// Clears all filter, envelope, AGC and limiter state; the selected preset is kept and any
    /// pending crossfade is dropped.
    pub fn reset(&mut self) {
        self.outgoing = None;
        self.fading = false;
        if let Some(path) = self.main.as_mut() {
            path.reset();
        }
        self.standalone_agc.reset();
        self.standalone_limiter.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::StudioChain;
    use crate::HOP_SAMPLES;
    use crate::preset::Preset;

    fn tone_hop(start: usize, amplitude: f32) -> [f32; HOP_SAMPLES] {
        let mut hop = [0.0_f32; HOP_SAMPLES];
        for (index, sample) in hop.iter_mut().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let time = (start + index) as f32 / 48_000.0;
            *sample = amplitude * (std::f32::consts::TAU * 1_000.0 * time).sin();
        }
        hop
    }

    #[test]
    fn off_leaves_every_bit_untouched_even_for_non_finite_input() {
        let mut chain = StudioChain::new(Preset::Off);
        let mut hop = tone_hop(0, 0.5);
        hop[3] = f32::NAN;
        hop[4] = f32::INFINITY;
        let before = hop.map(f32::to_bits);
        chain.process(&mut hop);
        assert_eq!(hop.map(f32::to_bits), before);
    }

    #[test]
    fn active_preset_changes_the_signal_and_keeps_it_finite() {
        let mut chain = StudioChain::new(Preset::Podcast);
        let mut changed = false;
        for hop_index in 0..20 {
            let original = tone_hop(hop_index * HOP_SAMPLES, 0.5);
            let mut hop = original;
            chain.process(&mut hop);
            assert!(hop.iter().all(|sample| sample.is_finite()));
            changed |= hop.map(f32::to_bits) != original.map(f32::to_bits);
        }
        assert!(changed);
    }

    #[test]
    fn latency_is_the_limiter_lookahead() {
        assert_eq!(StudioChain::new(Preset::Off).latency_samples(), 96);
        assert_eq!(StudioChain::new(Preset::Broadcast).latency_samples(), 96);
    }

    #[test]
    fn crossfade_to_off_completes_in_one_hop_and_then_passes_bits_through() {
        let mut chain = StudioChain::new(Preset::Podcast);
        for hop_index in 0..10 {
            chain.process(&mut tone_hop(hop_index * HOP_SAMPLES, 0.3));
        }
        chain.set_preset(Preset::Off);
        chain.process(&mut tone_hop(10 * HOP_SAMPLES, 0.3));
        let original = tone_hop(11 * HOP_SAMPLES, 0.3);
        let mut hop = original;
        chain.process(&mut hop);
        assert_eq!(hop.map(f32::to_bits), original.map(f32::to_bits));
    }

    #[test]
    fn selecting_the_current_preset_is_a_no_op() {
        let mut chain = StudioChain::new(Preset::Off);
        chain.set_preset(Preset::Off);
        let original = tone_hop(0, 0.3);
        let mut hop = original;
        chain.process(&mut hop);
        assert_eq!(hop.map(f32::to_bits), original.map(f32::to_bits));
    }

    #[test]
    fn reset_restores_the_cold_state() {
        let mut fresh = StudioChain::new(Preset::Podcast);
        let mut reused = StudioChain::new(Preset::Podcast);
        for hop_index in 0..30 {
            reused.process(&mut tone_hop(hop_index * HOP_SAMPLES, 0.7));
        }
        reused.reset();
        let mut a = tone_hop(0, 0.2);
        let mut b = a;
        fresh.process(&mut a);
        reused.process(&mut b);
        assert_eq!(a.map(f32::to_bits), b.map(f32::to_bits));
    }
}
