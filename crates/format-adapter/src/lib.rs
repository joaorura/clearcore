#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ};

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
                write!(f, "buffer length mismatch: expected {expected}, got {actual}")
            }
            Self::CapacityExceeded => write!(f, "adapter buffer capacity exceeded"),
        }
    }
}

impl std::error::Error for FormatError {}

pub const ACCUMULATOR_CAPACITY_SAMPLES: usize = 1920;

#[derive(Debug)]
pub struct InputAccumulator {
    buffer: [f32; ACCUMULATOR_CAPACITY_SAMPLES],
    head: usize,
    len: usize,
}

impl Default for InputAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl InputAccumulator {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: [0.0; ACCUMULATOR_CAPACITY_SAMPLES],
            head: 0,
            len: 0,
        }
    }

    pub fn push_samples(&mut self, _samples: &[f32]) {
        // Skeleton: red phase no-op
    }

    pub fn pop_frame(&mut self) -> Option<AudioFrame> {
        // Skeleton: red phase returns None
        None
    }

    #[must_use]
    pub const fn available_samples(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}

pub struct EndpointFormatConverter;

impl EndpointFormatConverter {
    pub fn validate_config(_config: &AudioEndpointConfig) -> Result<(), FormatError> {
        // Skeleton: red phase returns Ok(()) so unsupported format test fails
        Ok(())
    }

    pub fn pcm16_to_f32(_i16_slice: &[i16], _f32_out: &mut [f32]) -> Result<(), FormatError> {
        // Skeleton: red phase no-op so roundtrip test fails
        Ok(())
    }

    pub fn f32_to_pcm16(_f32_slice: &[f32], _i16_out: &mut [i16]) -> Result<(), FormatError> {
        // Skeleton: red phase no-op so roundtrip test fails
        Ok(())
    }

    pub fn stereo_to_mono(_stereo: &[f32], _mono: &mut [f32]) -> Result<(), FormatError> {
        Ok(())
    }

    pub fn mono_to_stereo(_mono: &[f32], _stereo: &mut [f32]) -> Result<(), FormatError> {
        Ok(())
    }
}
