#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_contracts::SAMPLE_RATE_HZ;

use crate::{AudioEndpointConfig, FormatError, SampleFormat};

/// Converts audio buffers between native endpoint representations and the canonical engine format.
///
/// Canonical engine format is single-channel (mono), 48 kHz, normalized 32-bit floating point `[-1.0, 1.0]`.
pub struct EndpointFormatConverter;

impl EndpointFormatConverter {
    /// Validates whether an endpoint audio configuration is supported.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::UnsupportedChannels`] if channel count is not 1 (mono) or 2 (stereo).
    /// Returns [`FormatError::UnsupportedSampleRate`] if sample rate is not `48_000` Hz.
    pub fn validate_config(config: &AudioEndpointConfig) -> Result<(), FormatError> {
        if config.channels == 0 || config.channels > 2 {
            return Err(FormatError::UnsupportedChannels(config.channels));
        }

        if config.sample_rate_hz != SAMPLE_RATE_HZ {
            return Err(FormatError::UnsupportedSampleRate(config.sample_rate_hz));
        }

        match config.sample_format {
            SampleFormat::Pcm16 | SampleFormat::Float32 => Ok(()),
        }
    }

    /// Converts PCM16 signed 16-bit integer samples into normalized `[-1.0, 1.0]` f32 samples.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::BufferLengthMismatch`] if source and destination slice lengths differ.
    pub fn pcm16_to_f32(i16_slice: &[i16], f32_out: &mut [f32]) -> Result<(), FormatError> {
        if i16_slice.len() != f32_out.len() {
            return Err(FormatError::BufferLengthMismatch {
                expected: i16_slice.len(),
                actual: f32_out.len(),
            });
        }

        for (src, dst) in i16_slice.iter().zip(f32_out.iter_mut()) {
            *dst = if *src >= 0 {
                f32::from(*src) / 32767.0
            } else {
                f32::from(*src) / 32768.0
            };
        }

        Ok(())
    }

    /// Converts normalized `[-1.0, 1.0]` f32 samples into PCM16 signed 16-bit integers with saturation clamping.
    ///
    /// Non-finite samples (NaN, Inf) fail closed and are converted to 0 (digital silence).
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::BufferLengthMismatch`] if source and destination slice lengths differ.
    #[allow(clippy::cast_possible_truncation)]
    pub fn f32_to_pcm16(f32_slice: &[f32], i16_out: &mut [i16]) -> Result<(), FormatError> {
        if f32_slice.len() != i16_out.len() {
            return Err(FormatError::BufferLengthMismatch {
                expected: f32_slice.len(),
                actual: i16_out.len(),
            });
        }

        for (src, dst) in f32_slice.iter().zip(i16_out.iter_mut()) {
            let sample = *src;
            if !sample.is_finite() {
                *dst = 0;
                continue;
            }

            if sample >= 0.0 {
                let scaled = (sample * 32767.0).round();
                *dst = scaled.clamp(0.0, 32767.0) as i16;
            } else {
                let scaled = (sample * 32768.0).round();
                *dst = scaled.clamp(-32768.0, 0.0) as i16;
            }
        }

        Ok(())
    }

    /// Downmixes interleaved stereo samples to mono by averaging left and right channels.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::BufferLengthMismatch`] if `stereo.len() != mono.len() * 2`.
    pub fn stereo_to_mono(stereo: &[f32], mono: &mut [f32]) -> Result<(), FormatError> {
        let expected_mono_len = stereo.len() / 2;
        if stereo.len() != mono.len() * 2 {
            return Err(FormatError::BufferLengthMismatch {
                expected: expected_mono_len,
                actual: mono.len(),
            });
        }

        for (i, item) in mono.iter_mut().enumerate() {
            *item = (stereo[2 * i] + stereo[2 * i + 1]) * 0.5;
        }

        Ok(())
    }

    /// Upmixes mono samples to interleaved stereo by duplicating each mono sample to both channels.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::BufferLengthMismatch`] if `stereo.len() != mono.len() * 2`.
    pub fn mono_to_stereo(mono: &[f32], stereo: &mut [f32]) -> Result<(), FormatError> {
        let expected_stereo_len = mono.len() * 2;
        if stereo.len() != expected_stereo_len {
            return Err(FormatError::BufferLengthMismatch {
                expected: expected_stereo_len,
                actual: stereo.len(),
            });
        }

        for (i, &sample) in mono.iter().enumerate() {
            stereo[2 * i] = sample;
            stereo[2 * i + 1] = sample;
        }

        Ok(())
    }
}
