#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

pub mod endpoint_converter;
pub mod format_worker;
pub mod input_accumulator;
pub mod output_deframer;
pub mod resampler;

pub use endpoint_converter::EndpointFormatConverter;
pub use format_worker::FormatAdapterWorker;
pub use input_accumulator::{ACCUMULATOR_CAPACITY_SAMPLES, InputAccumulator};
pub use output_deframer::{DEFRAMER_CAPACITY_SAMPLES, OutputDeframer};
pub use resampler::{IdentityResampler, LinearResampler, Resampler};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    Pcm16,
    Float32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioEndpointConfig {
    pub sample_rate_hz: u32,
    pub channels: usize,
    pub sample_format: SampleFormat,
}

impl AudioEndpointConfig {
    #[must_use]
    pub const fn new(sample_rate_hz: u32, channels: usize, sample_format: SampleFormat) -> Self {
        Self {
            sample_rate_hz,
            channels,
            sample_format,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    UnsupportedChannels(usize),
    UnsupportedSampleRate(u32),
    UnsupportedSampleFormat,
    BufferLengthMismatch { expected: usize, actual: usize },
    CapacityExceeded,
}

impl core::fmt::Display for FormatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedChannels(c) => write!(f, "unsupported channel count: {c}"),
            Self::UnsupportedSampleRate(r) => write!(f, "unsupported sample rate: {r} Hz"),
            Self::UnsupportedSampleFormat => write!(f, "unsupported sample format"),
            Self::BufferLengthMismatch { expected, actual } => {
                write!(
                    f,
                    "buffer length mismatch: expected {expected}, got {actual}"
                )
            }
            Self::CapacityExceeded => write!(f, "adapter buffer capacity exceeded"),
        }
    }
}

impl std::error::Error for FormatError {}
