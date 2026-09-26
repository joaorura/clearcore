mod audio;
mod wire;

pub use audio::{AudioFrame, CHANNELS, Discontinuity, FrameEnvelope, HOP_SAMPLES, SAMPLE_RATE_HZ};
pub use wire::{WireDecodeError, WireFrameEnvelopeV1};
