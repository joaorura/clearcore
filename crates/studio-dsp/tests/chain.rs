//! Integration tests of the public `studio_dsp` API with synthetic signals only.

#![forbid(unsafe_code)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::suboptimal_flops
)]

use std::f32::consts::TAU;

use studio_dsp::loudness::integrated_lufs;
use studio_dsp::{HOP_SAMPLES, Preset, StudioChain, StudioControl};

const ALL_PRESETS: [Preset; 4] = [
    Preset::Off,
    Preset::Natural,
    Preset::Podcast,
    Preset::Broadcast,
];
const ACTIVE_PRESETS: [Preset; 3] = [Preset::Natural, Preset::Podcast, Preset::Broadcast];

/// `10^(-1/20)` rounded down: the -1 dBFS ceiling.
const CEILING: f32 = 0.891_250_9;

fn tone(freq_hz: f32, amplitude: f32, start: usize, len: usize) -> Vec<f32> {
    (start..start + len)
        .map(|index| amplitude * (TAU * freq_hz * index as f32 / 48_000.0).sin())
        .collect()
}

/// Deterministic pseudo-random samples in `[-amplitude, amplitude]` (xorshift64).
fn noise(len: usize, amplitude: f32) -> Vec<f32> {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            amplitude * ((state >> 40) as f32 / (1_u64 << 24) as f32 * 2.0 - 1.0)
        })
        .collect()
}

/// Runs whole hops through the chain in place.
fn process(chain: &mut StudioChain, signal: &mut [f32]) {
    for chunk in signal.chunks_exact_mut(HOP_SAMPLES) {
        let mut hop = [0.0_f32; HOP_SAMPLES];
        hop.copy_from_slice(chunk);
        chain.process(&mut hop);
        chunk.copy_from_slice(&hop);
    }
}

fn bits(signal: &[f32]) -> Vec<u32> {
    signal.iter().map(|sample| sample.to_bits()).collect()
}

fn max_step(signal: &[f32]) -> f32 {
    signal
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs())
        .fold(0.0, f32::max)
}

fn peak(signal: &[f32]) -> f32 {
    signal
        .iter()
        .fold(0.0, |acc, &sample| acc.max(sample.abs()))
}

#[test]
fn off_is_bit_exact_for_every_kind_of_input() {
    let mut signal = noise(HOP_SAMPLES * 50, 1.5);
    signal[7] = f32::NAN;
    signal[8] = f32::INFINITY;
    signal[9] = f32::NEG_INFINITY;
    signal[10] = 1.0e-40;
    signal[11] = -0.0;
    let expected = bits(&signal);
    let mut chain = StudioChain::new(Preset::Off);
    process(&mut chain, &mut signal);
    assert_eq!(bits(&signal), expected);
}

#[test]
fn output_never_exceeds_the_ceiling_with_plus_6_db_input() {
    let loud = 2.0;
    for preset in ACTIVE_PRESETS {
        for mut signal in [
            tone(1_000.0, loud, 0, 48_000),
            tone(60.0, loud, 0, 48_000),
            noise(48_000, loud),
        ] {
            let mut chain = StudioChain::new(preset);
            process(&mut chain, &mut signal);
            assert!(
                peak(&signal) <= CEILING,
                "{preset:?} peak {}",
                peak(&signal)
            );
        }
    }
}

#[test]
fn ceiling_holds_across_preset_switches_between_active_presets() {
    let mut chain = StudioChain::new(Preset::Natural);
    let mut signal = tone(800.0, 2.0, 0, HOP_SAMPLES * 40);
    for (index, chunk) in signal.chunks_exact_mut(HOP_SAMPLES).enumerate() {
        chain.set_preset(ACTIVE_PRESETS[index % ACTIVE_PRESETS.len()]);
        process(&mut chain, chunk);
    }
    assert!(peak(&signal) <= CEILING, "peak {}", peak(&signal));
}

#[test]
fn agc_steers_a_quiet_tone_to_minus_16_lufs_through_the_whole_chain() {
    // A 1 kHz sine of peak P dBFS reads P - 3.01 LUFS: -26 LUFS is a peak of about -22.99 dBFS.
    let amplitude = 10.0_f32.powf((-26.0 + 3.0103) / 20.0);
    let mut signal = tone(1_000.0, amplitude, 0, 48_000 * 60);
    let mut chain = StudioChain::new(Preset::Podcast);
    process(&mut chain, &mut signal);
    let measured = integrated_lufs(&signal[signal.len() - 48_000 * 5..]);
    assert!((measured + 16.0).abs() < 0.3, "measured {measured}");
}

/// The tone is 1300 Hz on purpose: it is not a whole number of cycles per 48-sample unit, so the
/// limiter delay (96 samples) shows up as a phase offset between the old and the new signal. A
/// 1 kHz tone has exactly two cycles in 96 samples and hid a crossfade ramp 48x too short.
///
/// Measured over every preset pair, the largest step across the switch is at most 1.04x the
/// steady-state step (usually 1.00x); the limit is 1.5x. Mutation check: a ramp of 8 samples
/// instead of 384, or a dry switch, exceeds that limit.
#[test]
fn preset_switches_never_create_a_discontinuity() {
    for from in ALL_PRESETS {
        for to in ALL_PRESETS {
            let mut chain = StudioChain::new(from);
            let mut before = tone(1_300.0, 0.3, 0, HOP_SAMPLES * 30);
            process(&mut chain, &mut before);
            chain.set_preset(to);
            let mut during = tone(1_300.0, 0.3, HOP_SAMPLES * 30, HOP_SAMPLES * 4);
            process(&mut chain, &mut during);

            let steady_before = max_step(&before[HOP_SAMPLES * 20..]);
            let steady_after = max_step(&during[HOP_SAMPLES * 2..]);
            let across_switch = max_step(&during);
            let limit = 1.5 * steady_before.max(steady_after);
            assert!(
                across_switch <= limit,
                "{from:?} -> {to:?}: step {across_switch} limit {limit}"
            );
            let join = [before[before.len() - 1], during[0]];
            assert!(max_step(&join) <= limit, "{from:?} -> {to:?} at the join");
        }
    }
}

#[test]
fn non_finite_and_denormal_input_never_reaches_the_output() {
    let poison = [
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1.0e-40,
        -1.0e-40,
        f32::MAX,
        f32::MIN,
        f32::MIN_POSITIVE / 4.0,
    ];
    for preset in ACTIVE_PRESETS {
        let mut chain = StudioChain::new(preset);
        let mut signal = tone(500.0, 0.3, 0, HOP_SAMPLES * 20);
        for (index, sample) in signal.iter_mut().enumerate() {
            if index % 7 == 0 {
                *sample = poison[(index / 7) % poison.len()];
            }
        }
        process(&mut chain, &mut signal);
        assert!(signal.iter().all(|sample| sample.is_finite()), "{preset:?}");
        assert!(peak(&signal) <= CEILING, "{preset:?}");
    }
}

#[test]
fn non_finite_input_is_sanitised_during_a_crossfade_to_and_from_off() {
    let mut chain = StudioChain::new(Preset::Off);
    chain.set_preset(Preset::Podcast);
    let mut hop = [0.2_f32; HOP_SAMPLES];
    hop[5] = f32::NAN;
    hop[300] = f32::INFINITY;
    chain.process(&mut hop);
    assert!(hop.iter().all(|sample| sample.is_finite()));

    chain.set_preset(Preset::Off);
    let mut hop = [0.2_f32; HOP_SAMPLES];
    hop[17] = f32::NEG_INFINITY;
    chain.process(&mut hop);
    assert!(hop.iter().all(|sample| sample.is_finite()));
}

fn scripted_run() -> Vec<f32> {
    let mut chain = StudioChain::new(Preset::Natural);
    let mut signal = noise(HOP_SAMPLES * 60, 0.8);
    for (index, chunk) in signal.chunks_exact_mut(HOP_SAMPLES).enumerate() {
        if index % 7 == 3 {
            chain.set_preset(ALL_PRESETS[(index / 7) % ALL_PRESETS.len()]);
        }
        process(&mut chain, chunk);
    }
    signal
}

#[test]
fn two_runs_produce_identical_bits() {
    assert_eq!(bits(&scripted_run()), bits(&scripted_run()));
}

#[test]
fn switching_twice_before_a_hop_still_crossfades_from_the_audible_preset() {
    let mut chain = StudioChain::new(Preset::Podcast);
    let mut warm = tone(1_000.0, 0.3, 0, HOP_SAMPLES * 20);
    process(&mut chain, &mut warm);
    chain.set_preset(Preset::Off);
    chain.set_preset(Preset::Broadcast);
    let mut hop = tone(1_000.0, 0.3, HOP_SAMPLES * 20, HOP_SAMPLES);
    process(&mut chain, &mut hop);
    assert!(hop.iter().all(|sample| sample.is_finite()));
    let join = [warm[warm.len() - 1], hop[0]];
    assert!(max_step(&join) < 0.05);
    assert!(max_step(&hop) < 0.05);
}

#[test]
fn off_stays_bit_exact_after_a_switch_that_is_undone_before_the_next_hop() {
    // Off -> Podcast -> Off with no `process` in between: there is nothing to fade (no audible
    // outgoing path), so the hop must pass through untouched, NaN, infinity and denormals included.
    let mut chain = StudioChain::new(Preset::Off);
    chain.set_preset(Preset::Podcast);
    chain.set_preset(Preset::Off);
    let mut signal = noise(HOP_SAMPLES, 1.5);
    signal[3] = f32::NAN;
    signal[4] = f32::INFINITY;
    signal[5] = f32::NEG_INFINITY;
    signal[6] = 1.0e-40;
    signal[7] = -1.0e-40;
    signal[8] = -0.0;
    let expected = bits(&signal);
    process(&mut chain, &mut signal);
    let altered = bits(&signal)
        .iter()
        .zip(&expected)
        .filter(|(got, want)| got != want)
        .count();
    assert_eq!(
        altered, 0,
        "{altered} of {HOP_SAMPLES} samples were altered"
    );
}

#[test]
fn latency_is_96_samples_for_every_preset() {
    for preset in ALL_PRESETS {
        assert_eq!(StudioChain::new(preset).latency_samples(), 96);
    }
}

#[test]
fn control_is_shared_across_threads() {
    let control = StudioControl::new(Preset::Off);
    std::thread::scope(|scope| {
        scope.spawn(|| control.set_preset(Preset::Broadcast));
    });
    assert_eq!(control.preset(), Preset::Broadcast);
}
