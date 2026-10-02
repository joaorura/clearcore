#![forbid(unsafe_code)]
// Plain multiply/add keeps results bit-identical on every target; `mul_add` would depend on
// hardware FMA availability and fall back to a slow software routine without it.
#![allow(clippy::suboptimal_flops)]
//! Studio DSP chain: high-pass, fixed EQ, de-esser, compressor, loudness AGC and limiter.
//!
//! Pure Rust, no external runtime dependencies, no `unsafe`, and no allocation on the audio path.
//! One hop is [`HOP_SAMPLES`] samples of mono 48 kHz audio.

mod agc;
mod biquad;
mod chain;
mod compressor;
mod deesser;
mod limiter;
pub mod loudness;
mod params;
mod preset;
#[cfg(test)]
mod test_util;
mod units;

pub use chain::StudioChain;
pub use preset::{Preset, StudioControl};

/// Samples per processing hop (10 ms at 48 kHz). Matches `realtime_noise_contracts::HOP_SAMPLES`.
pub const HOP_SAMPLES: usize = 480;

/// Sample rate in hertz. Matches `realtime_noise_contracts::SAMPLE_RATE_HZ`.
pub const SAMPLE_RATE_HZ: u32 = 48_000;
