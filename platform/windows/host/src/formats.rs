#![forbid(unsafe_code)]

use realtime_noise_contracts::{AudioFrame, EndpointError, HOP_SAMPLES, SAMPLE_RATE_HZ};

/// Canonical format specifications for Windows audio endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioEndpointFormat {
    /// 32-bit IEEE-754 floating point, normalized `[-1.0, 1.0]`.
    Float32,
    /// 16-bit signed integer linear PCM.
    Pcm16,
}

/// Validates whether the given format matches the Windows `WaveRT` / WASAPI canonical constraints.
///
/// Canonical constraints:
/// - Sample rate must be exactly `48_000` Hz.
/// - Channel count must be exactly `1` (mono).
/// - Bit depth must be either `16` (PCM16) or `32` (Float32).
///
/// # Errors
///
/// Returns [`EndpointError::UnsupportedFormat`] if any constraint is violated.
pub fn validate_format(
    sample_rate_hz: u32,
    channels: u16,
    bit_depth: u16,
) -> Result<AudioEndpointFormat, EndpointError> {
    if sample_rate_hz != SAMPLE_RATE_HZ {
        return Err(EndpointError::UnsupportedFormat(format!(
            "unsupported sample rate {sample_rate_hz} Hz, canonical format requires {SAMPLE_RATE_HZ} Hz"
        )));
    }

    if channels != 1 {
        return Err(EndpointError::UnsupportedFormat(format!(
            "unsupported channels {channels}, canonical format requires mono (1 channel)"
        )));
    }

    match bit_depth {
        32 => Ok(AudioEndpointFormat::Float32),
        16 => Ok(AudioEndpointFormat::Pcm16),
        _ => Err(EndpointError::UnsupportedFormat(format!(
            "unsupported bit depth {bit_depth} bits, canonical format requires 16 or 32 bits"
        ))),
    }
}

/// Converts PCM16 signed 16-bit integer samples to normalized `[-1.0, 1.0]` Float32 samples.
///
/// # Errors
///
/// Returns [`EndpointError::UnsupportedFormat`] if source and destination slice lengths differ.
#[allow(clippy::cast_precision_loss)]
pub fn pcm16_to_canonical_f32(pcm16: &[i16], f32_out: &mut [f32]) -> Result<(), EndpointError> {
    if pcm16.len() != f32_out.len() {
        return Err(EndpointError::UnsupportedFormat(format!(
            "PCM16 to Float32 buffer length mismatch: expected {}, got {}",
            f32_out.len(),
            pcm16.len()
        )));
    }

    for (src, dst) in pcm16.iter().zip(f32_out.iter_mut()) {
        *dst = if *src >= 0 {
            f32::from(*src) / 32767.0
        } else {
            f32::from(*src) / 32768.0
        };
    }

    Ok(())
}

/// Converts normalized `[-1.0, 1.0]` Float32 samples into PCM16 signed 16-bit integers with clamping.
///
/// Non-finite samples (NaN, Inf) fail closed and are sanitized to 0 (digital silence).
///
/// # Errors
///
/// Returns [`EndpointError::UnsupportedFormat`] if source and destination slice lengths differ.
#[allow(clippy::cast_possible_truncation)]
pub fn canonical_f32_to_pcm16(f32_in: &[f32], pcm16_out: &mut [i16]) -> Result<(), EndpointError> {
    if f32_in.len() != pcm16_out.len() {
        return Err(EndpointError::UnsupportedFormat(format!(
            "Float32 to PCM16 buffer length mismatch: expected {}, got {}",
            pcm16_out.len(),
            f32_in.len()
        )));
    }

    for (src, dst) in f32_in.iter().zip(pcm16_out.iter_mut()) {
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

/// Serializes an [`AudioFrame`] into a 1920-byte little-endian IEEE-754 Float32 byte buffer.
pub fn audio_frame_to_f32_bytes(frame: &AudioFrame) -> [u8; HOP_SAMPLES * 4] {
    let mut bytes = [0_u8; HOP_SAMPLES * 4];
    for (chunk, &sample) in bytes.chunks_exact_mut(4).zip(frame.iter()) {
        chunk.copy_from_slice(&sample.to_le_bytes());
    }
    bytes
}

/// Deserializes a 1920-byte little-endian IEEE-754 Float32 buffer into an [`AudioFrame`].
///
/// # Errors
///
/// Returns [`EndpointError::UnsupportedFormat`] if the byte buffer is not exactly 1920 bytes.
pub fn f32_bytes_to_audio_frame(bytes: &[u8]) -> Result<AudioFrame, EndpointError> {
    const EXPECTED_LEN: usize = HOP_SAMPLES * 4;
    if bytes.len() != EXPECTED_LEN {
        return Err(EndpointError::UnsupportedFormat(format!(
            "f32 byte buffer length must be exactly {EXPECTED_LEN} bytes, got {}",
            bytes.len()
        )));
    }

    let mut frame = [0.0_f32; HOP_SAMPLES];
    for (chunk, sample) in bytes.chunks_exact(4).zip(frame.iter_mut()) {
        let raw = [chunk[0], chunk[1], chunk[2], chunk[3]];
        *sample = f32::from_le_bytes(raw);
    }

    Ok(frame)
}

/// Serializes an [`AudioFrame`] into a 960-byte little-endian PCM16 byte buffer.
#[allow(clippy::cast_possible_truncation)]
pub fn audio_frame_to_pcm16_bytes(frame: &AudioFrame) -> [u8; HOP_SAMPLES * 2] {
    let mut bytes = [0_u8; HOP_SAMPLES * 2];
    for (chunk, &sample) in bytes.chunks_exact_mut(2).zip(frame.iter()) {
        let pcm = if !sample.is_finite() {
            0_i16
        } else if sample >= 0.0 {
            let scaled = (sample * 32767.0).round();
            scaled.clamp(0.0, 32767.0) as i16
        } else {
            let scaled = (sample * 32768.0).round();
            scaled.clamp(-32768.0, 0.0) as i16
        };
        chunk.copy_from_slice(&pcm.to_le_bytes());
    }
    bytes
}

/// Deserializes a 960-byte little-endian PCM16 buffer into a canonical [`AudioFrame`].
///
/// # Errors
///
/// Returns [`EndpointError::UnsupportedFormat`] if the byte buffer is not exactly 960 bytes.
#[allow(clippy::cast_precision_loss)]
pub fn pcm16_bytes_to_audio_frame(bytes: &[u8]) -> Result<AudioFrame, EndpointError> {
    const EXPECTED_LEN: usize = HOP_SAMPLES * 2;
    if bytes.len() != EXPECTED_LEN {
        return Err(EndpointError::UnsupportedFormat(format!(
            "PCM16 byte buffer length must be exactly {EXPECTED_LEN} bytes, got {}",
            bytes.len()
        )));
    }

    let mut frame = [0.0_f32; HOP_SAMPLES];
    for (chunk, sample) in bytes.chunks_exact(2).zip(frame.iter_mut()) {
        let raw = i16::from_le_bytes([chunk[0], chunk[1]]);
        *sample = if raw >= 0 {
            f32::from(raw) / 32767.0
        } else {
            f32::from(raw) / 32768.0
        };
    }

    Ok(frame)
}
