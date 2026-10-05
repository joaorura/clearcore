//! Dependency-free 16-bit mono PCM WAV codec.

/// Errors returned when decoding a WAV buffer.
#[derive(Debug, PartialEq, Eq)]
pub enum WavError {
    TooShort,
    NotRiffWave,
    UnsupportedFormat,
    BadDataChunk,
}

/// Encodes mono `f32` samples as a 16-bit PCM WAV (44-byte header).
///
/// Samples are clamped to `[-1, 1]`. Sizes that do not fit in `u32` saturate
/// (a WAV cannot describe more than 4 GiB anyway).
pub fn encode_wav_pcm16_mono(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let data_len = u32::try_from(samples.len().saturating_mul(2)).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(HEADER_LEN.saturating_add(samples.len().saturating_mul(2)));
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&data_len.saturating_add(36).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&sample_rate.saturating_mul(2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for &s in samples {
        // Clamped to [-32767, 32767] before the cast, so it cannot truncate.
        #[allow(clippy::cast_possible_truncation)]
        let v = (s.clamp(-1.0, 1.0) * 32767.0).round() as i16;
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

const HEADER_LEN: usize = 44;

fn le_u16(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn le_u32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([
        *s.first()?,
        *s.get(1)?,
        *s.get(2)?,
        *s.get(3)?,
    ]))
}

/// Decodes a 16-bit mono PCM WAV into samples and sample rate.
///
/// Walks the chunk list, ignoring unknown chunks before `data`.
pub fn decode_wav_pcm16_mono(bytes: &[u8]) -> Result<(Vec<f32>, u32), WavError> {
    if bytes.len() < 12 {
        return Err(WavError::TooShort);
    }
    if bytes.get(0..4) != Some(b"RIFF".as_slice()) || bytes.get(8..12) != Some(b"WAVE".as_slice()) {
        return Err(WavError::NotRiffWave);
    }
    let mut pos: usize = 12;
    let mut sample_rate: Option<u32> = None;
    loop {
        let id = bytes
            .get(pos..pos.checked_add(4).ok_or(WavError::BadDataChunk)?)
            .ok_or(WavError::BadDataChunk)?;
        let size = le_u32(bytes, pos.checked_add(4).ok_or(WavError::BadDataChunk)?)
            .ok_or(WavError::BadDataChunk)?;
        let size = usize::try_from(size).map_err(|_| WavError::BadDataChunk)?;
        let body = pos.checked_add(8).ok_or(WavError::BadDataChunk)?;
        let end = body.checked_add(size).ok_or(WavError::BadDataChunk)?;
        if id == b"data" {
            let rate = sample_rate.ok_or(WavError::UnsupportedFormat)?;
            if size % 2 != 0 {
                return Err(WavError::BadDataChunk);
            }
            let raw = bytes.get(body..end).ok_or(WavError::BadDataChunk)?;
            let samples = raw
                .chunks_exact(2)
                .map(|c| {
                    let lo = c.first().copied().unwrap_or(0);
                    let hi = c.get(1).copied().unwrap_or(0);
                    f32::from(i16::from_le_bytes([lo, hi])) / 32768.0
                })
                .collect();
            return Ok((samples, rate));
        }
        if id == b"fmt " {
            if size < 16 || bytes.len() < end {
                return Err(WavError::UnsupportedFormat);
            }
            let tag = le_u16(bytes, body);
            let channels = le_u16(bytes, body.saturating_add(2));
            let rate = le_u32(bytes, body.saturating_add(4));
            let bits = le_u16(bytes, body.saturating_add(14));
            if tag != Some(1) || channels != Some(1) || bits != Some(16) {
                return Err(WavError::UnsupportedFormat);
            }
            sample_rate = Some(rate.ok_or(WavError::UnsupportedFormat)?);
        }
        // Chunks are word-aligned: odd sizes carry one pad byte.
        pos = end.checked_add(size & 1).ok_or(WavError::BadDataChunk)?;
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]
mod tests {
    use super::*;

    #[test]
    fn header_is_44_bytes_and_riff() {
        let w = encode_wav_pcm16_mono(&[0.0; 10], 48_000);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..12], b"WAVE");
        assert_eq!(w.len(), 44 + 20);
    }

    #[test]
    fn round_trip_is_within_two_lsb() {
        let x: Vec<f32> = (0..1000)
            .map(|i| (i as f32 / 1000.0 * 2.0 - 1.0) * 0.9)
            .collect();
        let (y, sr) = decode_wav_pcm16_mono(&encode_wav_pcm16_mono(&x, 48_000)).unwrap();
        assert_eq!(sr, 48_000);
        assert_eq!(y.len(), x.len());
        assert!(
            x.iter()
                .zip(&y)
                .all(|(a, b)| (a - b).abs() <= 2.0 / 32768.0)
        );
    }

    #[test]
    fn out_of_range_samples_are_clamped_not_wrapped() {
        let (y, _) = decode_wav_pcm16_mono(&encode_wav_pcm16_mono(&[2.0, -2.0], 48_000)).unwrap();
        assert!(y[0] > 0.99 && y[1] < -0.99);
    }

    #[test]
    fn rejects_garbage_and_non_pcm16() {
        assert_eq!(decode_wav_pcm16_mono(b"nope"), Err(WavError::TooShort));
        let mut w = encode_wav_pcm16_mono(&[0.0; 4], 48_000);
        w[20] = 3; // format tag 3 = float
        assert_eq!(decode_wav_pcm16_mono(&w), Err(WavError::UnsupportedFormat));
    }

    #[test]
    fn data_chunk_larger_than_buffer_is_bad_data_chunk() {
        let mut w = encode_wav_pcm16_mono(&[0.0; 4], 48_000);
        w[40..44].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(decode_wav_pcm16_mono(&w), Err(WavError::BadDataChunk));
        let mut w = encode_wav_pcm16_mono(&[0.0; 4], 48_000);
        w[40..44].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(decode_wav_pcm16_mono(&w), Err(WavError::BadDataChunk));
    }

    #[test]
    fn extra_list_chunk_before_data_is_skipped() {
        let w = encode_wav_pcm16_mono(&[0.5, -0.5], 16_000);
        let mut out = w[..36].to_vec();
        out.extend_from_slice(b"LIST");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&[1, 2, 3, 0]); // 3 bytes + pad byte
        out.extend_from_slice(&w[36..]);
        let (y, sr) = decode_wav_pcm16_mono(&out).unwrap();
        assert_eq!(sr, 16_000);
        assert_eq!(y.len(), 2);
        assert!((y[0] - 0.5).abs() < 1e-3 && (y[1] + 0.5).abs() < 1e-3);
    }

    #[test]
    fn zero_samples_round_trip() {
        let w = encode_wav_pcm16_mono(&[], 44_100);
        assert_eq!(w.len(), 44);
        assert_eq!(decode_wav_pcm16_mono(&w), Ok((Vec::new(), 44_100)));
    }

    #[test]
    fn stereo_is_unsupported() {
        let mut w = encode_wav_pcm16_mono(&[0.0; 4], 48_000);
        w[22] = 2;
        assert_eq!(decode_wav_pcm16_mono(&w), Err(WavError::UnsupportedFormat));
    }

    #[test]
    fn missing_data_chunk_and_non_riff_are_rejected() {
        let w = encode_wav_pcm16_mono(&[0.0; 4], 48_000);
        assert_eq!(decode_wav_pcm16_mono(&w[..36]), Err(WavError::BadDataChunk));
        let mut bad = w;
        bad[0] = b'X';
        assert_eq!(decode_wav_pcm16_mono(&bad), Err(WavError::NotRiffWave));
    }
}
