//! Ingestion of enrollment samples: validate the raw take, run it through the base denoiser,
//! measure the clean speech and encode the full clean take as the stored WAV.

// removed when Task S6 wires the module
#![allow(dead_code)]

use realtime_noise_model::speech_trim::trim_speech;
use realtime_noise_model::wav::encode_wav_pcm16_mono;

use crate::enrollment_error::EnrollError;

pub trait Denoiser: Send {
    /// 48 kHz mono in -> 48 kHz mono out, same length, latency already compensated.
    fn denoise(&mut self, pcm48: &[f32]) -> Result<Vec<f32>, EnrollError>;
}

pub const MIN_SPEECH_RMS_DBFS: f32 = -40.0;
pub const MAX_PEAK: f32 = 0.99;
const SAMPLE_RATE: u32 = 48_000;

#[derive(Debug, Clone)]
pub struct IngestResult {
    pub wav_bytes: Vec<u8>,
    pub speech_seconds: f32,
    pub peak: f32,
    pub rms_dbfs: f32,
    pub active_fraction: f32,
}

/// Zeroes the borrowed PCM when dropped, so every exit path (including unwinding) wipes it.
struct ZeroOnDrop<'a>(&'a mut [f32]);

impl Drop for ZeroOnDrop<'_> {
    fn drop(&mut self) {
        self.0.fill(0.0);
    }
}

/// Steps: reject non-finite input (`InvalidAudio`) and a RAW peak >= 0.99 (`Clipping`); denoise;
/// measure with `trim_speech`; reject active-speech RMS < -40 dBFS (`TooQuiet`) and zero trimmed
/// speech (`TooLittleSpeech`); encode the DENOISED, UNTRIMMED take. `pcm48` is always zeroed
/// before returning, on success and on error.
///
/// A pure-silence take is reported as `TooQuiet` (its active-speech RMS is the floor, far below
/// the threshold), checked before the zero-speech test.
pub fn ingest_sample(
    denoiser: &mut dyn Denoiser,
    pcm48: &mut [f32],
) -> Result<IngestResult, EnrollError> {
    let guard = ZeroOnDrop(pcm48);
    ingest_inner(denoiser, guard.0)
}

fn ingest_inner(denoiser: &mut dyn Denoiser, pcm48: &[f32]) -> Result<IngestResult, EnrollError> {
    if pcm48.is_empty() || pcm48.iter().any(|v| !v.is_finite()) {
        return Err(EnrollError::InvalidAudio);
    }
    if pcm48.iter().fold(0.0_f32, |m, v| m.max(v.abs())) >= MAX_PEAK {
        return Err(EnrollError::Clipping);
    }
    let clean = denoiser.denoise(pcm48)?;
    if clean.len() != pcm48.len() || clean.iter().any(|v| !v.is_finite()) {
        return Err(EnrollError::Failed);
    }
    let peak = clean.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    let trimmed = trim_speech(&clean, SAMPLE_RATE);
    let rms_dbfs = realtime_noise_model::speech_trim::active_rms_dbfs(&clean, SAMPLE_RATE);
    if rms_dbfs < MIN_SPEECH_RMS_DBFS {
        return Err(EnrollError::TooQuiet);
    }
    if trimmed.samples.is_empty() || trimmed.speech_seconds <= 0.0 {
        return Err(EnrollError::TooLittleSpeech);
    }
    Ok(IngestResult {
        wav_bytes: encode_wav_pcm16_mono(&clean, SAMPLE_RATE),
        speech_seconds: trimmed.speech_seconds,
        peak,
        rms_dbfs,
        active_fraction: trimmed.active_fraction,
    })
}

#[cfg(feature = "tract")]
pub struct TractDenoiser {
    repo_root: std::path::PathBuf,
}

#[cfg(feature = "tract")]
impl TractDenoiser {
    #[must_use]
    pub fn new(repo_root: std::path::PathBuf) -> Self {
        Self { repo_root }
    }
}

#[cfg(feature = "tract")]
impl Denoiser for TractDenoiser {
    fn denoise(&mut self, pcm48: &[f32]) -> Result<Vec<f32>, EnrollError> {
        use realtime_noise_contracts::HOP_SAMPLES;
        use realtime_noise_model::{
            ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, CpuProfile, InferenceBackend,
            TractBackend,
        };

        let manifest =
            ApprovedAssetManifest::verify(&self.repo_root).map_err(|_| EnrollError::Failed)?;
        // No reset exists: a fresh backend (fresh state) per sample is the contract.
        let mut backend = TractBackend::new(&manifest, CpuProfile::Avx2Minimum)
            .map_err(|_| EnrollError::Failed)?;
        let delay = usize::try_from(ALGORITHM_LATENCY_SAMPLES).map_err(|_| EnrollError::Failed)?;
        let pad = delay + (HOP_SAMPLES - pcm48.len() % HOP_SAMPLES) % HOP_SAMPLES;
        let mut padded = Vec::with_capacity(pcm48.len() + pad);
        padded.extend_from_slice(pcm48);
        padded.resize(pcm48.len() + pad, 0.0);
        let mut out = Vec::with_capacity(padded.len());
        for chunk in padded.chunks_exact(HOP_SAMPLES) {
            let mut frame = [0.0_f32; HOP_SAMPLES];
            frame.copy_from_slice(chunk);
            let processed = backend.process(&frame).map_err(|_| EnrollError::Failed)?;
            out.extend_from_slice(&processed.samples);
        }
        let mut result: Vec<f32> = out.into_iter().skip(delay).collect();
        result.truncate(pcm48.len());
        Ok(result)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;
    use realtime_noise_model::wav::decode_wav_pcm16_mono;

    struct Gain(f32);
    impl Denoiser for Gain {
        fn denoise(&mut self, pcm48: &[f32]) -> Result<Vec<f32>, EnrollError> {
            Ok(pcm48.iter().map(|v| v * self.0).collect())
        }
    }

    fn bursts(secs: f32, amp: f32) -> Vec<f32> {
        let n = (secs * 48_000.0) as usize;
        (0..n)
            .map(|i| {
                let t = i as f32 / 48_000.0;
                let env = if (t * 4.0).fract() < 0.7 { 1.0 } else { 0.0 };
                amp * env * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect()
    }

    #[test]
    fn raw_pcm_is_zeroed_even_on_error() {
        let mut pcm = bursts(1.0, 0.3);
        pcm[100] = 1.0;
        let r = ingest_sample(&mut Gain(1.0), &mut pcm);
        assert_eq!(r.unwrap_err(), EnrollError::Clipping);
        assert!(pcm.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn raw_pcm_is_zeroed_on_success() {
        let mut pcm = bursts(3.0, 0.3);
        assert!(ingest_sample(&mut Gain(1.0), &mut pcm).is_ok());
        assert!(pcm.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn clipping_is_judged_on_the_raw_peak() {
        let mut pcm = bursts(1.0, 0.3);
        pcm[100] = 0.995;
        let r = ingest_sample(&mut Gain(0.5), &mut pcm);
        assert_eq!(r.unwrap_err(), EnrollError::Clipping);
    }

    #[test]
    fn quiet_take_is_rejected() {
        let mut pcm = bursts(3.0, 0.001);
        let r = ingest_sample(&mut Gain(1.0), &mut pcm);
        assert_eq!(r.unwrap_err(), EnrollError::TooQuiet);
    }

    #[test]
    fn silence_is_too_quiet_or_too_little_speech() {
        let mut pcm = vec![0.0_f32; 48_000 * 2];
        let r = ingest_sample(&mut Gain(1.0), &mut pcm).unwrap_err();
        assert!(matches!(
            r,
            EnrollError::TooQuiet | EnrollError::TooLittleSpeech
        ));
        // Documented behaviour: silence is reported as TooQuiet.
        assert_eq!(r, EnrollError::TooQuiet);
    }

    #[test]
    fn good_take_returns_a_decodable_wav_and_speech_seconds() {
        let mut pcm = bursts(3.0, 0.3);
        let len = pcm.len();
        let r = ingest_sample(&mut Gain(1.0), &mut pcm).unwrap();
        let (decoded, rate) = decode_wav_pcm16_mono(&r.wav_bytes).unwrap();
        assert_eq!(rate, 48_000);
        assert_eq!(decoded.len(), len);
        assert!(r.speech_seconds > 0.0 && r.speech_seconds <= 3.0);
        assert!(r.active_fraction > 0.0 && r.rms_dbfs >= MIN_SPEECH_RMS_DBFS);
    }

    #[test]
    fn non_finite_input_is_invalid_audio() {
        let mut pcm = bursts(1.0, 0.3);
        pcm[10] = f32::NAN;
        let r = ingest_sample(&mut Gain(1.0), &mut pcm);
        assert_eq!(r.unwrap_err(), EnrollError::InvalidAudio);
        assert!(pcm.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn stored_wav_reflects_the_denoised_signal() {
        let raw = bursts(3.0, 0.4);
        let mut pcm = raw;
        let r = ingest_sample(&mut Gain(0.5), &mut pcm).unwrap();
        let (decoded, _) = decode_wav_pcm16_mono(&r.wav_bytes).unwrap();
        let peak = decoded.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.2).abs() < 0.01, "peak {peak}");
        assert!((r.peak - 0.2).abs() < 0.01);
    }

    #[cfg(feature = "tract")]
    #[test]
    #[ignore = "uses the approved DFNet3 asset; run locally"]
    fn tract_denoiser_keeps_length_and_is_finite() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut seed = 12345_u32;
        let noise: Vec<f32> = (0..96_000)
            .map(|_| {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((seed >> 8) as f32 / 16_777_216.0 - 0.5) * 0.2
            })
            .collect();
        let start = std::time::Instant::now();
        let out = TractDenoiser::new(root).denoise(&noise).unwrap();
        eprintln!("DFNET3_COST: {:?} for 2.0 s audio", start.elapsed());
        assert_eq!(out.len(), noise.len());
        assert!(out.iter().all(|v| v.is_finite()));
    }
}
