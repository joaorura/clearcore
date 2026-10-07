//! Speaker enrollment: from a short 16 kHz mono recording to a [`VoiceProfile`].
//!
//! The pipeline runs outside the real-time path:
//!
//! 1. [`validate_enrollment_audio`] checks the recording (rate, duration, clipping, level and the
//!    fraction of active speech).
//! 2. A [`SpeakerEmbeddingModel`] turns the audio into the four `FiLM` vectors
//!    (`gamma_enc`, `beta_enc`, `gamma_df`, `beta_df`, 256 values each). The product model is the
//!    signed `voice-enrollment-asset-v1` ONNX ([`OnnxEnrollmentModel`], feature `tract`).
//! 3. The vectors are validated (finite, in range, 256 values) and sealed in a [`VoiceProfile`].
//!
//! The raw recording is biometric data: [`SpeakerEnrollmentEngine::enroll`] overwrites it with
//! zeros before returning, on success and on every error path, and [`EnrollmentRecording`] wipes
//! itself when dropped.

use std::fmt;

use crate::voice_profile::{
    BandGains, FILM_HIDDEN_DIM, FiLMVectors, VoiceProfile, VoiceProfileError,
};

pub const ENROLLMENT_SAMPLE_RATE_HZ: u32 = 16_000;
pub const ENROLLMENT_MIN_DURATION_SECS: f32 = 6.0;
/// Engineering cap, not a training range.
///
/// The model was trained on 6-12 s recordings and durations up to 90 s were measured stable
/// (spec section 3). The service enforces the same 90 s through its speech budget.
pub const ENROLLMENT_MAX_DURATION_SECS: f32 = 90.0;
/// 6 s at 16 kHz.
pub const ENROLLMENT_MIN_SAMPLES: usize = 96_000;
/// 90 s at 16 kHz.
///
/// The validator accepts exactly this many samples, while the service's speech budget may leave a
/// one-sample slack at the 90 s boundary; the caller (the service) must map
/// [`EnrollmentError::TooLong`] to `ENROLL_BUDGET_EXCEEDED`.
pub const ENROLLMENT_MAX_SAMPLES: usize = 1_440_000;
/// Highest accepted absolute sample value; above it the recording is considered clipped.
pub const ENROLLMENT_MAX_PEAK: f32 = 0.99;
/// Minimum overall level (RMS, dBFS) so a silent recording is refused.
pub const ENROLLMENT_MIN_RMS_DBFS: f32 = -40.0;
/// Minimum fraction of 20 ms frames classified as speech by the energy VAD.
pub const ENROLLMENT_MIN_ACTIVE_FRACTION: f32 = 0.60;

/// 20 ms at 16 kHz.
const VAD_FRAME_SAMPLES: usize = 320;
/// A frame is speech when it is louder than this absolute floor...
const VAD_ABSOLUTE_FLOOR_DBFS: f32 = -50.0;
/// ...and within this many dB of the loudest frame.
const VAD_RELATIVE_RANGE_DB: f32 = 30.0;

#[derive(Debug)]
pub enum EnrollmentError {
    UnsupportedSampleRate { actual: u32 },
    TooShort { samples: usize },
    TooLong { samples: usize },
    NonFiniteSample { index: usize },
    Clipping { peak: f32 },
    TooQuiet { rms_dbfs: f32 },
    InsufficientSpeech { active_fraction: f32 },
    InvalidMetadata(&'static str),
    Model(String),
    InvalidConditioning(VoiceProfileError),
}

impl fmt::Display for EnrollmentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedSampleRate { actual } => write!(
                f,
                "enrollment audio must be {ENROLLMENT_SAMPLE_RATE_HZ} Hz, got {actual} Hz"
            ),
            Self::TooShort { samples } => write!(
                f,
                "enrollment audio has {samples} samples, minimum is {ENROLLMENT_MIN_SAMPLES} ({ENROLLMENT_MIN_DURATION_SECS} s)"
            ),
            Self::TooLong { samples } => write!(
                f,
                "enrollment audio has {samples} samples, maximum is {ENROLLMENT_MAX_SAMPLES} ({ENROLLMENT_MAX_DURATION_SECS} s)"
            ),
            Self::NonFiniteSample { index } => {
                write!(
                    f,
                    "enrollment audio has a non-finite sample at index {index}"
                )
            }
            Self::Clipping { peak } => write!(
                f,
                "enrollment audio is clipped (peak {peak:.3} > {ENROLLMENT_MAX_PEAK})"
            ),
            Self::TooQuiet { rms_dbfs } => write!(
                f,
                "enrollment audio is too quiet ({rms_dbfs:.1} dBFS < {ENROLLMENT_MIN_RMS_DBFS} dBFS)"
            ),
            Self::InsufficientSpeech { active_fraction } => write!(
                f,
                "enrollment audio has {:.0}% active speech, minimum is {:.0}%",
                active_fraction * 100.0,
                ENROLLMENT_MIN_ACTIVE_FRACTION * 100.0
            ),
            Self::InvalidMetadata(what) => write!(f, "invalid profile metadata: {what}"),
            Self::Model(message) => write!(f, "enrollment model failed: {message}"),
            Self::InvalidConditioning(error) => {
                write!(f, "enrollment model produced invalid conditioning: {error}")
            }
        }
    }
}

impl std::error::Error for EnrollmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidConditioning(error) => Some(error),
            _ => None,
        }
    }
}

/// Borrowed view of a recording, for validation.
#[derive(Debug, Clone, Copy)]
pub struct EnrollmentAudio<'a> {
    pub samples: &'a [f32],
    pub sample_rate: u32,
}

/// Measurements taken while validating a recording.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EnrollmentReport {
    pub duration_secs: f32,
    pub peak: f32,
    pub rms_dbfs: f32,
    pub active_speech_fraction: f32,
}

#[allow(clippy::cast_precision_loss)]
const fn count_to_f32(value: usize) -> f32 {
    value as f32
}

fn mean_square_dbfs(samples: &[f32]) -> f32 {
    let sum: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let mean = sum / samples.len().max(1) as f64;
    #[allow(clippy::cast_possible_truncation)]
    let db = (10.0 * (mean + 1e-12).log10()) as f32;
    db
}

/// Checks a recording against the enrollment contract. Pure: no I/O, no model.
///
/// The energy VAD cannot tell speech from stationary noise; judging SNR needs the model's local
/// SNR estimate and is outside this check.
pub fn validate_enrollment_audio(
    audio: &EnrollmentAudio<'_>,
) -> Result<EnrollmentReport, EnrollmentError> {
    if audio.sample_rate != ENROLLMENT_SAMPLE_RATE_HZ {
        return Err(EnrollmentError::UnsupportedSampleRate {
            actual: audio.sample_rate,
        });
    }
    let samples = audio.samples;
    if samples.len() < ENROLLMENT_MIN_SAMPLES {
        return Err(EnrollmentError::TooShort {
            samples: samples.len(),
        });
    }
    if samples.len() > ENROLLMENT_MAX_SAMPLES {
        return Err(EnrollmentError::TooLong {
            samples: samples.len(),
        });
    }
    if let Some(index) = samples.iter().position(|s| !s.is_finite()) {
        return Err(EnrollmentError::NonFiniteSample { index });
    }
    let peak = samples.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    if peak > ENROLLMENT_MAX_PEAK {
        return Err(EnrollmentError::Clipping { peak });
    }
    let rms_dbfs = mean_square_dbfs(samples);
    if rms_dbfs <= ENROLLMENT_MIN_RMS_DBFS {
        return Err(EnrollmentError::TooQuiet { rms_dbfs });
    }

    let frame_levels: Vec<f32> = samples
        .chunks_exact(VAD_FRAME_SAMPLES)
        .map(mean_square_dbfs)
        .collect();
    let loudest = frame_levels
        .iter()
        .fold(f32::MIN, |acc, level| acc.max(*level));
    let threshold = VAD_ABSOLUTE_FLOOR_DBFS.max(loudest - VAD_RELATIVE_RANGE_DB);
    let active = frame_levels
        .iter()
        .filter(|level| **level >= threshold)
        .count();
    let active_speech_fraction = count_to_f32(active) / count_to_f32(frame_levels.len());
    if active_speech_fraction < ENROLLMENT_MIN_ACTIVE_FRACTION {
        return Err(EnrollmentError::InsufficientSpeech {
            active_fraction: active_speech_fraction,
        });
    }

    Ok(EnrollmentReport {
        duration_secs: count_to_f32(samples.len()) / 16_000.0,
        peak,
        rms_dbfs,
        active_speech_fraction,
    })
}

/// Owned recording that overwrites its samples with zeros when dropped.
pub struct EnrollmentRecording {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl EnrollmentRecording {
    #[must_use]
    pub const fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    #[must_use]
    pub fn as_audio(&self) -> EnrollmentAudio<'_> {
        EnrollmentAudio {
            samples: &self.samples,
            sample_rate: self.sample_rate,
        }
    }

    /// Overwrites every sample with `0.0` (the length is kept).
    pub fn wipe(&mut self) {
        self.samples.fill(0.0);
        // Keeps the compiler from proving the buffer is never read again and dropping the stores.
        std::hint::black_box(&mut self.samples);
    }
}

impl Drop for EnrollmentRecording {
    fn drop(&mut self) {
        self.wipe();
    }
}

impl fmt::Debug for EnrollmentRecording {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Never prints samples.
        f.debug_struct("EnrollmentRecording")
            .field("samples", &self.samples.len())
            .field("sample_rate", &self.sample_rate)
            .finish()
    }
}

/// The four `FiLM` vectors exactly as the model produced them (not yet validated).
#[derive(Debug, Clone, PartialEq)]
pub struct RawFilmVectors {
    pub gamma_enc: Vec<f32>,
    pub beta_enc: Vec<f32>,
    pub gamma_df: Vec<f32>,
    pub beta_df: Vec<f32>,
}

/// Speaker embedding extraction: 16 kHz mono audio in, `FiLM` vectors out.
pub trait SpeakerEmbeddingModel {
    fn extract(&mut self, samples_16khz: &[f32]) -> Result<RawFilmVectors, EnrollmentError>;
}

/// Identity of the profile being created.
#[derive(Debug, Clone)]
pub struct ProfileMetadata {
    pub id: String,
    pub name: String,
    pub created_at_utc: String,
}

pub struct SpeakerEnrollmentEngine<M: SpeakerEmbeddingModel> {
    model: M,
}

impl<M: SpeakerEmbeddingModel> SpeakerEnrollmentEngine<M> {
    pub const fn new(model: M) -> Self {
        Self { model }
    }

    /// Validates the recording, extracts the `FiLM` vectors and returns the sealed profile.
    ///
    /// `recording` is overwritten with zeros before this returns, whatever the outcome. Vectors
    /// outside the physical limits are rejected, never clamped.
    pub fn enroll(
        &mut self,
        recording: &mut EnrollmentRecording,
        metadata: &ProfileMetadata,
        eq: Option<BandGains>,
    ) -> Result<VoiceProfile, EnrollmentError> {
        let outcome = self.extract(recording, metadata);
        recording.wipe();
        let raw = outcome?;
        let film = FiLMVectors::new(raw.gamma_enc, raw.beta_enc, raw.gamma_df, raw.beta_df)
            .map_err(EnrollmentError::InvalidConditioning)?;
        VoiceProfile::new(
            metadata.id.clone(),
            metadata.name.clone(),
            metadata.created_at_utc.clone(),
            film,
            eq,
        )
        .map_err(EnrollmentError::InvalidConditioning)
    }

    fn extract(
        &mut self,
        recording: &EnrollmentRecording,
        metadata: &ProfileMetadata,
    ) -> Result<RawFilmVectors, EnrollmentError> {
        if metadata.id.trim().is_empty() {
            return Err(EnrollmentError::InvalidMetadata("id is empty"));
        }
        if metadata.name.trim().is_empty() {
            return Err(EnrollmentError::InvalidMetadata("name is empty"));
        }
        validate_enrollment_audio(&recording.as_audio())?;
        let raw = self.model.extract(recording.samples())?;
        for (field, vector) in [
            ("gamma_enc", &raw.gamma_enc),
            ("beta_enc", &raw.beta_enc),
            ("gamma_df", &raw.gamma_df),
            ("beta_df", &raw.beta_df),
        ] {
            if vector.len() != FILM_HIDDEN_DIM {
                return Err(EnrollmentError::InvalidConditioning(
                    VoiceProfileError::DimensionMismatch {
                        field,
                        expected: FILM_HIDDEN_DIM,
                        actual: vector.len(),
                    },
                ));
            }
        }
        Ok(raw)
    }
}

#[cfg(feature = "tract")]
pub use onnx::OnnxEnrollmentModel;

#[cfg(feature = "tract")]
mod onnx {
    use std::{
        io::{Cursor, Read},
        path::Component,
    };

    use flate2::read::GzDecoder;
    use sha2::{Digest, Sha256};
    use tar::Archive;
    use tract_onnx::prelude::*;

    use super::{EnrollmentError, RawFilmVectors, SpeakerEmbeddingModel};
    use crate::{ModelRole, VerifiedAsset};

    const MEMBER: &str = "enrollment.onnx";
    /// Members a development archive may contain; anything else is rejected.
    const DEV_ARCHIVE_MEMBER_ALLOWLIST: &[&str] = &["enrollment.onnx"];
    /// The real member is about 85 MB; anything above this is treated as hostile.
    const MAX_ONNX_BYTES: u64 = 256 * 1024 * 1024;
    /// Ceiling on the total decompressed bytes the tar walk may consume.
    const MAX_ARCHIVE_DECOMPRESSED_BYTES: u64 = 300 * 1024 * 1024;
    const MAX_ARCHIVE_ENTRIES: usize = 16;
    const OUTPUTS: [&str; 4] = ["gamma_enc", "beta_enc", "gamma_df", "beta_df"];

    /// Runs the `voice-enrollment-asset-v1` ONNX through tract.
    ///
    /// Contract: one input `audio` (`f32`, `[1, N]`, 16 kHz mono) and four outputs named
    /// `gamma_enc`, `beta_enc`, `gamma_df`, `beta_df` (256 values each, any shape that flattens
    /// to 256).
    pub struct OnnxEnrollmentModel {
        onnx_bytes: Vec<u8>,
    }

    impl OnnxEnrollmentModel {
        /// Loads the model from an asset already verified by the registry (role
        /// `SpeakerEnrollment`, signature and digest checked).
        pub fn from_verified_asset(asset: &VerifiedAsset) -> Result<Self, EnrollmentError> {
            if asset.role() != ModelRole::SpeakerEnrollment {
                return Err(EnrollmentError::Model(format!(
                    "asset {} has role {}, expected speaker-enrollment",
                    asset.asset_id(),
                    asset.role().as_str()
                )));
            }
            let snapshot = asset.archive_snapshot();
            let mut archive = Archive::new(GzDecoder::new(&*snapshot));
            let entries = archive
                .entries()
                .map_err(|e| EnrollmentError::Model(e.to_string()))?;
            for entry in entries {
                let mut entry = entry.map_err(|e| EnrollmentError::Model(e.to_string()))?;
                let is_member = entry
                    .path()
                    .map_err(|e| EnrollmentError::Model(e.to_string()))?
                    .ends_with(MEMBER);
                if is_member {
                    let mut bytes = Vec::new();
                    entry
                        .read_to_end(&mut bytes)
                        .map_err(|e| EnrollmentError::Model(e.to_string()))?;
                    return Self::from_onnx_bytes(bytes);
                }
            }
            Err(EnrollmentError::Model(format!(
                "asset has no {MEMBER} member"
            )))
        }

        /// Development-only loader for the unsigned `voice-enrollment-asset-v1` archive.
        ///
        /// `expected_sha256_hex` is the lowercase hex SHA-256 of the WHOLE archive. Only the
        /// allowlisted members are accepted and read; any other member, absolute path or `..`
        /// component is rejected. Error messages are fixed strings and never echo a hash.
        pub fn from_dev_archive(
            archive_tar_gz: &[u8],
            expected_sha256_hex: &str,
        ) -> Result<Self, EnrollmentError> {
            let fail = |message: &str| EnrollmentError::Model(message.to_owned());
            let digest = Sha256::digest(archive_tar_gz);
            let mut actual = String::with_capacity(64);
            for byte in digest {
                actual.extend(
                    [byte >> 4, byte & 0x0f]
                        .into_iter()
                        .filter_map(|n| char::from_digit(u32::from(n), 16)),
                );
            }
            if expected_sha256_hex.len() != 64 || actual != expected_sha256_hex {
                return Err(fail("development archive hash mismatch"));
            }
            let mut archive =
                Archive::new(GzDecoder::new(archive_tar_gz).take(MAX_ARCHIVE_DECOMPRESSED_BYTES));
            let entries = archive
                .entries()
                .map_err(|_| fail("development archive is unreadable"))?;
            let mut onnx_bytes: Option<Vec<u8>> = None;
            for (index, entry) in entries.enumerate() {
                if index >= MAX_ARCHIVE_ENTRIES {
                    return Err(fail("development archive has too many entries"));
                }
                let mut entry = entry.map_err(|_| fail("development archive is unreadable"))?;
                let path = entry
                    .path()
                    .map_err(|_| fail("development archive has an invalid member path"))?
                    .into_owned();
                let safe = path
                    .components()
                    .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
                if !safe {
                    return Err(fail("development archive has an unsafe member path"));
                }
                let name = path.to_str().map(|n| n.trim_start_matches("./"));
                if entry.header().entry_type().is_dir()
                    && name.is_some_and(|n| n.is_empty() || n == ".")
                {
                    continue;
                }
                if !name.is_some_and(|n| DEV_ARCHIVE_MEMBER_ALLOWLIST.contains(&n)) {
                    return Err(fail("development archive has an unlisted member"));
                }
                if !entry.header().entry_type().is_file() {
                    return Err(fail("development archive member is not a regular file"));
                }
                if onnx_bytes.is_some() {
                    return Err(fail("development archive has a duplicate member"));
                }
                if entry.size() > MAX_ONNX_BYTES {
                    return Err(fail("development archive member is too large"));
                }
                let mut bytes = Vec::new();
                entry
                    .by_ref()
                    .take(MAX_ONNX_BYTES + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|_| fail("development archive member is unreadable"))?;
                if bytes.len() as u64 > MAX_ONNX_BYTES {
                    return Err(fail("development archive member is too large"));
                }
                onnx_bytes = Some(bytes);
            }
            let bytes =
                onnx_bytes.ok_or_else(|| fail("development archive has no model member"))?;
            Self::from_onnx_bytes(bytes)
        }

        /// Parses raw ONNX bytes into an enrollment model.
        pub fn from_onnx_bytes(onnx_bytes: Vec<u8>) -> Result<Self, EnrollmentError> {
            // Parse once now so a broken asset fails at load time, not at the first enrollment.
            onnx()
                .model_for_read(&mut Cursor::new(&onnx_bytes))
                .map_err(|e| EnrollmentError::Model(e.to_string()))?;
            Ok(Self { onnx_bytes })
        }

        /// Loads the enrollment model directly from a file path.
        pub fn from_file(path: &std::path::Path) -> Result<Self, EnrollmentError> {
            let bytes = std::fs::read(path).map_err(|e| {
                EnrollmentError::Model(format!("failed to read enrollment model file: {e}"))
            })?;
            Self::from_onnx_bytes(bytes)
        }
    }

    impl SpeakerEmbeddingModel for OnnxEnrollmentModel {
        fn extract(&mut self, samples_16khz: &[f32]) -> Result<RawFilmVectors, EnrollmentError> {
            let fail = |e: &dyn std::fmt::Display| EnrollmentError::Model(e.to_string());
            let mut model = onnx()
                .model_for_read(&mut Cursor::new(&self.onnx_bytes))
                .map_err(|e| fail(&e))?;
            let mut names = Vec::new();
            for outlet in model.output_outlets().map_err(|e| fail(&e))?.to_vec() {
                names.push(model.outlet_label(outlet).map(str::to_owned));
            }
            model
                .set_input_fact(
                    0,
                    InferenceFact::dt_shape(f32::datum_type(), tvec!(1, samples_16khz.len())),
                )
                .map_err(|e| fail(&e))?;
            let runnable = model
                .into_optimized()
                .map_err(|e| fail(&e))?
                .into_runnable()
                .map_err(|e| fail(&e))?;
            let input = Tensor::from_shape(&[1, samples_16khz.len()], samples_16khz)
                .map_err(|e| fail(&e))?;
            let outputs = runnable.run(tvec!(input.into())).map_err(|e| fail(&e))?;

            let take = |wanted: &str| -> Result<Vec<f32>, EnrollmentError> {
                let position = names
                    .iter()
                    .position(|name| name.as_deref() == Some(wanted))
                    .ok_or_else(|| {
                        EnrollmentError::Model(format!("model has no output {wanted}"))
                    })?;
                let view = outputs
                    .get(position)
                    .ok_or_else(|| {
                        EnrollmentError::Model(format!("model returned no value for {wanted}"))
                    })?
                    .as_slice::<f32>()
                    .map_err(|e| fail(&e))?;
                Ok(view.to_vec())
            };
            let [gamma_enc, beta_enc, gamma_df, beta_df] = OUTPUTS;
            Ok(RawFilmVectors {
                gamma_enc: take(gamma_enc)?,
                beta_enc: take(beta_enc)?,
                gamma_df: take(gamma_df)?,
                beta_df: take(beta_df)?,
            })
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    const RATE: u32 = ENROLLMENT_SAMPLE_RATE_HZ;

    /// 150 Hz "voiced" bursts: `on_ms` of tone then `off_ms` of near silence, repeated.
    fn burst_speech(seconds: f32, amplitude: f32, on_ms: usize, off_ms: usize) -> Vec<f32> {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let total = (seconds * 16_000.0) as usize;
        let period = (on_ms + off_ms) * 16;
        (0..total)
            .map(|n| {
                let t = count_to_f32(n) / 16_000.0;
                if n % period < on_ms * 16 {
                    amplitude * (2.0 * std::f32::consts::PI * 150.0 * t).sin()
                } else {
                    0.0
                }
            })
            .collect()
    }

    fn speech() -> Vec<f32> {
        burst_speech(8.0, 0.2, 500, 100)
    }

    fn check(samples: &[f32], rate: u32) -> Result<EnrollmentReport, EnrollmentError> {
        validate_enrollment_audio(&EnrollmentAudio {
            samples,
            sample_rate: rate,
        })
    }

    #[test]
    fn valid_recording_is_accepted_and_measured() {
        let report = check(&speech(), RATE).expect("valid");
        assert!((report.duration_secs - 8.0).abs() < 1e-3);
        assert!(report.peak <= 0.2 + 1e-4);
        assert!(report.rms_dbfs > ENROLLMENT_MIN_RMS_DBFS);
        assert!(report.active_speech_fraction > 0.8, "{report:?}");
    }

    #[test]
    fn only_16_khz_is_accepted() {
        assert!(matches!(
            check(&speech(), 48_000),
            Err(EnrollmentError::UnsupportedSampleRate { actual: 48_000 })
        ));
    }

    #[test]
    fn duration_bounds_are_inclusive() {
        let at_min = burst_speech(6.0, 0.2, 500, 100);
        assert_eq!(at_min.len(), ENROLLMENT_MIN_SAMPLES);
        assert!(check(&at_min, RATE).is_ok());
        assert!(matches!(
            check(&at_min[..ENROLLMENT_MIN_SAMPLES - 1], RATE),
            Err(EnrollmentError::TooShort { .. })
        ));
        let at_max = burst_speech(90.0, 0.2, 500, 100);
        assert_eq!(at_max.len(), ENROLLMENT_MAX_SAMPLES);
        assert!(check(&at_max, RATE).is_ok());
        let mut too_long = at_max;
        too_long.push(0.1);
        assert!(matches!(
            check(&too_long, RATE),
            Err(EnrollmentError::TooLong { .. })
        ));
        assert!(matches!(
            check(&[], RATE),
            Err(EnrollmentError::TooShort { samples: 0 })
        ));
    }

    #[test]
    fn sixty_seconds_are_accepted_and_ninety_one_are_not() {
        let ok = burst_speech(60.0, 0.2, 500, 100);
        assert!(check(&ok, RATE).is_ok());
        let long = burst_speech(91.0, 0.2, 500, 100);
        assert!(matches!(
            check(&long, RATE),
            Err(EnrollmentError::TooLong { .. })
        ));
    }

    #[test]
    fn non_finite_samples_are_rejected_with_their_index() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut samples = speech();
            samples[1234] = bad;
            assert!(matches!(
                check(&samples, RATE),
                Err(EnrollmentError::NonFiniteSample { index: 1234 })
            ));
        }
    }

    #[test]
    fn clipping_threshold_is_0_99() {
        let mut samples = speech();
        samples[500] = 0.99;
        assert!(
            check(&samples, RATE).is_ok(),
            "0.99 is the highest accepted peak"
        );
        samples[500] = 0.991;
        assert!(matches!(
            check(&samples, RATE),
            Err(EnrollmentError::Clipping { .. })
        ));
        samples[500] = -1.0;
        assert!(matches!(
            check(&samples, RATE),
            Err(EnrollmentError::Clipping { .. })
        ));
    }

    #[test]
    fn quiet_recording_is_rejected() {
        let samples = burst_speech(8.0, 0.002, 500, 100);
        assert!(matches!(
            check(&samples, RATE),
            Err(EnrollmentError::TooQuiet { .. })
        ));
        assert!(matches!(
            check(&vec![0.0; 96_000], RATE),
            Err(EnrollmentError::TooQuiet { .. })
        ));
    }

    #[test]
    fn mostly_silent_recording_is_rejected_for_lack_of_speech() {
        // 100 ms of tone every 600 ms: ~17% active.
        let samples = burst_speech(8.0, 0.4, 100, 500);
        let error = check(&samples, RATE).expect_err("not enough speech");
        assert!(
            matches!(error, EnrollmentError::InsufficientSpeech { active_fraction } if active_fraction < 0.3)
        );
    }

    #[test]
    fn speech_fraction_threshold_is_60_percent() {
        // 650 ms on / 350 ms off: 65% passes. 550 / 450: 55% fails.
        assert!(check(&burst_speech(8.0, 0.3, 650, 350), RATE).is_ok());
        assert!(matches!(
            check(&burst_speech(8.0, 0.3, 550, 450), RATE),
            Err(EnrollmentError::InsufficientSpeech { .. })
        ));
    }

    #[test]
    fn recording_wipes_itself_and_never_prints_samples() {
        let mut recording = EnrollmentRecording::new(vec![0.5; 64], RATE);
        assert!(!format!("{recording:?}").contains("0.5"));
        recording.wipe();
        assert_eq!(recording.samples().len(), 64);
        assert!(
            recording
                .samples()
                .iter()
                .all(|s| s.to_bits() == 0.0_f32.to_bits())
        );
    }

    // ---- engine, with a scripted model ----

    struct ScriptedModel {
        result: Option<Result<RawFilmVectors, EnrollmentError>>,
        calls: usize,
        saw_audio: Vec<f32>,
    }

    impl ScriptedModel {
        fn returning(result: Result<RawFilmVectors, EnrollmentError>) -> Self {
            Self {
                result: Some(result),
                calls: 0,
                saw_audio: Vec::new(),
            }
        }
    }

    impl SpeakerEmbeddingModel for ScriptedModel {
        fn extract(&mut self, samples: &[f32]) -> Result<RawFilmVectors, EnrollmentError> {
            self.calls += 1;
            self.saw_audio = samples.to_vec();
            self.result
                .take()
                .unwrap_or_else(|| Err(EnrollmentError::Model("exhausted".into())))
        }
    }

    fn vectors(gamma: f32, beta: f32) -> RawFilmVectors {
        RawFilmVectors {
            gamma_enc: vec![gamma; FILM_HIDDEN_DIM],
            beta_enc: vec![beta; FILM_HIDDEN_DIM],
            gamma_df: vec![gamma; FILM_HIDDEN_DIM],
            beta_df: vec![beta; FILM_HIDDEN_DIM],
        }
    }

    fn metadata() -> ProfileMetadata {
        ProfileMetadata {
            id: "spk-001".to_owned(),
            name: "Speaker".to_owned(),
            created_at_utc: "2026-10-02T12:00:00Z".to_owned(),
        }
    }

    fn all_zero(recording: &EnrollmentRecording) -> bool {
        recording
            .samples()
            .iter()
            .all(|s| s.to_bits() == 0.0_f32.to_bits())
    }

    #[test]
    fn enrollment_builds_a_sealed_profile_and_wipes_the_recording() {
        let audio = speech();
        let mut recording = EnrollmentRecording::new(audio.clone(), RATE);
        let mut engine =
            SpeakerEnrollmentEngine::new(ScriptedModel::returning(Ok(vectors(1.2, 0.03))));
        let profile = engine
            .enroll(&mut recording, &metadata(), Some(BandGains::neutral()))
            .expect("enrolled");
        assert_eq!(engine.model.calls, 1);
        assert_eq!(
            engine.model.saw_audio, audio,
            "the model sees the validated audio"
        );
        assert_eq!(profile.id, "spk-001");
        assert!(
            profile
                .film
                .gamma_enc
                .iter()
                .all(|g| (g - 1.2).abs() < f32::EPSILON)
        );
        assert!(profile.eq.is_some());
        profile.verify_integrity().expect("sealed");
        assert!(
            all_zero(&recording),
            "raw audio must be wiped after enrollment"
        );
    }

    #[test]
    fn invalid_audio_never_reaches_the_model_and_is_wiped() {
        let mut recording = EnrollmentRecording::new(burst_speech(8.0, 0.4, 100, 500), RATE);
        let mut engine =
            SpeakerEnrollmentEngine::new(ScriptedModel::returning(Ok(vectors(1.0, 0.0))));
        let error = engine
            .enroll(&mut recording, &metadata(), None)
            .expect_err("rejected");
        assert!(matches!(error, EnrollmentError::InsufficientSpeech { .. }));
        assert_eq!(engine.model.calls, 0);
        assert!(all_zero(&recording));
    }

    #[test]
    fn model_failure_is_reported_and_the_recording_is_wiped() {
        let mut recording = EnrollmentRecording::new(speech(), RATE);
        let mut engine = SpeakerEnrollmentEngine::new(ScriptedModel::returning(Err(
            EnrollmentError::Model("boom".to_owned()),
        )));
        assert!(matches!(
            engine.enroll(&mut recording, &metadata(), None),
            Err(EnrollmentError::Model(_))
        ));
        assert!(all_zero(&recording));
    }

    #[test]
    fn non_finite_or_out_of_range_vectors_are_rejected_not_clamped() {
        for bad in [
            {
                let mut raw = vectors(1.0, 0.0);
                raw.beta_enc[7] = f32::NAN;
                raw
            },
            vectors(1000.0, 0.0), // gamma far above the physical limit
            vectors(1.0, 99.0),   // beta far above the physical limit
            vectors(0.0, 0.0),    // gamma below its lower bound
        ] {
            let mut recording = EnrollmentRecording::new(speech(), RATE);
            let mut engine = SpeakerEnrollmentEngine::new(ScriptedModel::returning(Ok(bad)));
            let error = engine
                .enroll(&mut recording, &metadata(), None)
                .expect_err("rejected");
            assert!(
                matches!(error, EnrollmentError::InvalidConditioning(_)),
                "{error:?}"
            );
            assert!(all_zero(&recording));
        }
    }

    #[test]
    fn wrong_vector_dimensions_are_rejected() {
        let mut raw = vectors(1.0, 0.0);
        raw.gamma_df.pop();
        let mut recording = EnrollmentRecording::new(speech(), RATE);
        let mut engine = SpeakerEnrollmentEngine::new(ScriptedModel::returning(Ok(raw)));
        assert!(matches!(
            engine.enroll(&mut recording, &metadata(), None),
            Err(EnrollmentError::InvalidConditioning(
                VoiceProfileError::DimensionMismatch {
                    field: "gamma_df",
                    ..
                }
            ))
        ));
    }

    #[test]
    fn empty_id_or_name_is_rejected_before_inference() {
        for broken in [
            ProfileMetadata {
                id: "  ".to_owned(),
                ..metadata()
            },
            ProfileMetadata {
                name: String::new(),
                ..metadata()
            },
        ] {
            let mut recording = EnrollmentRecording::new(speech(), RATE);
            let mut engine =
                SpeakerEnrollmentEngine::new(ScriptedModel::returning(Ok(vectors(1.0, 0.0))));
            assert!(matches!(
                engine.enroll(&mut recording, &broken, None),
                Err(EnrollmentError::InvalidMetadata(_))
            ));
            assert_eq!(engine.model.calls, 0);
            assert!(all_zero(&recording));
        }
    }
}

#[cfg(all(test, feature = "tract"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod onnx_tests {
    use std::{fs, io::Write, path::PathBuf};

    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, Header};

    use super::*;
    use crate::{AssetDescriptor, ModelRole, VerifiedAsset};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn onnx_bytes() -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/enrollment-contract-test.onnx");
        fs::read(path).expect("fixture (tools/accelerators/gen_enrollment_test_onnx.py)")
    }

    fn tone(amplitude: f32, offset: f32) -> Vec<f32> {
        (0..ENROLLMENT_MIN_SAMPLES + 16_000)
            .map(|n| {
                let t = f32::from(u16::try_from(n % 65_000).unwrap_or(0)) / 16_000.0;
                amplitude.mul_add((2.0 * std::f32::consts::PI * 150.0 * t).sin(), offset)
            })
            .collect()
    }

    fn archive_with(member: &str, bytes: &[u8]) -> Vec<u8> {
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        let mut header = Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        builder
            .append_data(&mut header, member, bytes)
            .expect("append");
        builder.into_inner().expect("tar").finish().expect("gzip")
    }

    fn verified(role: ModelRole, archive: Vec<u8>) -> VerifiedAsset {
        let size_bytes = archive.len() as u64;
        VerifiedAsset::for_test(
            AssetDescriptor {
                asset_id: "voice-enrollment-asset-v1".to_owned(),
                role,
                sha256: "0".repeat(64),
                size_bytes,
                allowed_members: &["enrollment.onnx"],
            },
            archive,
        )
    }

    #[test]
    fn onnx_model_extracts_four_named_vectors_that_depend_on_the_audio() -> TestResult {
        let mut model = OnnxEnrollmentModel::from_onnx_bytes(onnx_bytes())?;
        let quiet = model.extract(&tone(0.1, 0.0))?;
        let loud = model.extract(&tone(0.4, 0.05))?;
        for vector in [
            &quiet.gamma_enc,
            &quiet.beta_enc,
            &quiet.gamma_df,
            &quiet.beta_df,
        ] {
            assert_eq!(vector.len(), FILM_HIDDEN_DIM);
            assert!(vector.iter().all(|v| v.is_finite()));
        }
        // Contract fixture: gamma_df = 1 + 0.5 * tanh(mean(|x|)), so it grows with the level and
        // is routed to the right output name.
        // Names are routed to the right output: gammas sit near 1, betas near 0.
        for gamma in [
            &quiet.gamma_enc,
            &quiet.gamma_df,
            &loud.gamma_enc,
            &loud.gamma_df,
        ] {
            assert!(
                (0.9..1.6).contains(&gamma[0]),
                "gamma should be near 1, got {}",
                gamma[0]
            );
        }
        for beta in [
            &quiet.beta_enc,
            &quiet.beta_df,
            &loud.beta_enc,
            &loud.beta_df,
        ] {
            assert!(
                beta[0].abs() < 0.06,
                "beta should be near 0, got {}",
                beta[0]
            );
        }
        assert!(loud.gamma_df[0] > quiet.gamma_df[0]);
        assert!(loud.beta_df[0] > quiet.beta_df[0]);
        assert!(
            (quiet.gamma_enc[0] - 1.0).abs() < 0.05,
            "mean of a zero-mean tone is ~0"
        );
        assert_ne!(
            loud.beta_enc[0].to_bits(),
            quiet.beta_enc[0].to_bits(),
            "DC offset moves beta_enc"
        );
        Ok(())
    }

    #[test]
    fn full_enrollment_through_the_onnx_model() -> TestResult {
        let mut engine =
            SpeakerEnrollmentEngine::new(OnnxEnrollmentModel::from_onnx_bytes(onnx_bytes())?);
        let samples: Vec<f32> = tone(0.3, 0.0);
        let mut recording = EnrollmentRecording::new(samples, ENROLLMENT_SAMPLE_RATE_HZ);
        let profile = engine.enroll(
            &mut recording,
            &ProfileMetadata {
                id: "spk-onnx".to_owned(),
                name: "Speaker".to_owned(),
                created_at_utc: "2026-10-02T12:00:00Z".to_owned(),
            },
            None,
        )?;
        profile.verify_integrity()?;
        assert!(!profile.film.is_identity());
        assert!(
            recording
                .samples()
                .iter()
                .all(|s| s.to_bits() == 0.0_f32.to_bits())
        );
        Ok(())
    }

    #[test]
    fn loading_from_a_verified_asset_requires_the_enrollment_role_and_member() {
        let archive = archive_with("enrollment.onnx", &onnx_bytes());
        assert!(
            OnnxEnrollmentModel::from_verified_asset(&verified(
                ModelRole::SpeakerEnrollment,
                archive.clone()
            ))
            .is_ok()
        );
        assert!(matches!(
            OnnxEnrollmentModel::from_verified_asset(&verified(ModelRole::NeuralEq, archive)),
            Err(EnrollmentError::Model(_))
        ));
        let wrong_member = archive_with("neural_eq.onnx", &onnx_bytes());
        assert!(matches!(
            OnnxEnrollmentModel::from_verified_asset(&verified(
                ModelRole::SpeakerEnrollment,
                wrong_member
            )),
            Err(EnrollmentError::Model(_))
        ));
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        let mut hex = String::new();
        for byte in Sha256::digest(bytes) {
            hex.extend(
                [byte >> 4, byte & 0x0f]
                    .into_iter()
                    .filter_map(|n| char::from_digit(u32::from(n), 16)),
            );
        }
        hex
    }

    fn archive_with_members(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (name, bytes) in members {
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            builder
                .append_data(&mut header, name, *bytes)
                .expect("append");
        }
        builder.into_inner().expect("tar").finish().expect("gzip")
    }

    #[test]
    fn dev_archive_rejects_wrong_hash_and_unlisted_members() {
        let onnx = onnx_bytes();
        let evil = archive_with_members(&[("enrollment.onnx", &onnx), ("evil.bin", b"x")]);
        let evil_hash = sha256_hex(&evil);
        match OnnxEnrollmentModel::from_dev_archive(&evil, &evil_hash) {
            Err(EnrollmentError::Model(message)) => assert!(!message.contains(&evil_hash)),
            other => panic!("unlisted member must be rejected, got ok={}", other.is_ok()),
        }
        let good = archive_with_members(&[("enrollment.onnx", &onnx)]);
        let wrong = "00".repeat(32);
        match OnnxEnrollmentModel::from_dev_archive(&good, &wrong) {
            Err(EnrollmentError::Model(message)) => {
                assert_eq!(message, "development archive hash mismatch");
                assert!(!message.contains(&sha256_hex(&good)));
            }
            other => panic!("wrong hash must be rejected, got ok={}", other.is_ok()),
        }
        // Uppercase hex is not the documented form either.
        assert!(
            OnnxEnrollmentModel::from_dev_archive(&good, &sha256_hex(&good).to_uppercase())
                .is_err()
        );
    }

    /// Gzipped tar built by hand so hostile names and types are not validated by the builder.
    fn raw_archive(entries: &[(&str, u8, &str, &[u8])]) -> Vec<u8> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        for (name, kind, link, data) in entries {
            raw_entry(&mut gz, name, *kind, link, data.len() as u64);
            gz.write_all(data).expect("data");
            gz.write_all(&vec![0; (512 - data.len() % 512) % 512])
                .expect("pad");
        }
        gz.write_all(&[0; 1024]).expect("end");
        gz.finish().expect("gzip")
    }

    fn raw_entry(out: &mut impl Write, name: &str, kind: u8, link: &str, size: u64) {
        let mut h = [0_u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[108..115].copy_from_slice(b"0000000");
        h[116..123].copy_from_slice(b"0000000");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[136..147].copy_from_slice(b"00000000000");
        h[156] = kind;
        h[157..157 + link.len()].copy_from_slice(link.as_bytes());
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h[155] = b' ';
        out.write_all(&h).expect("header");
    }

    fn assert_fixed_error(archive: &[u8], hash: &str) {
        match OnnxEnrollmentModel::from_dev_archive(archive, hash) {
            Err(EnrollmentError::Model(message)) => {
                assert!(!message.contains(hash));
                assert!(!message.contains("enrollment.onnx"));
                assert!(!message.contains("evil"));
            }
            other => panic!("expected a Model error, got ok={}", other.is_ok()),
        }
    }

    #[test]
    fn dev_archive_rejects_duplicate_and_unsafe_and_non_file_members() {
        let onnx = onnx_bytes();
        let cases = [
            raw_archive(&[
                ("enrollment.onnx", b'0', "", &onnx),
                ("enrollment.onnx", b'0', "", &onnx),
            ]),
            raw_archive(&[("../enrollment.onnx", b'0', "", &onnx)]),
            raw_archive(&[("/enrollment.onnx", b'0', "", &onnx)]),
            raw_archive(&[("a/../enrollment.onnx", b'0', "", &onnx)]),
            raw_archive(&[("enrollment.onnx", b'2', "/etc/passwd", b"")]),
            raw_archive(&[("enrollment.onnx", b'1', "other", b"")]),
            raw_archive(&[("enrollment.onnx", b'5', "", b"")]),
        ];
        for archive in &cases {
            assert_fixed_error(archive, &sha256_hex(archive));
        }
    }

    #[test]
    fn dev_archive_rejects_an_oversized_member_quickly() {
        let started = std::time::Instant::now();
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        let size = 300_u64 * 1024 * 1024;
        raw_entry(&mut gz, "enrollment.onnx", b'0', "", size);
        let chunk = vec![0_u8; 1024 * 1024];
        for _ in 0..300 {
            gz.write_all(&chunk).expect("zeros");
        }
        gz.write_all(&[0; 1024]).expect("end");
        let archive = gz.finish().expect("gzip");
        assert!(archive.len() < 10 * 1024 * 1024);
        assert_fixed_error(&archive, &sha256_hex(&archive));
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }

    #[test]
    fn dev_archive_rejects_too_many_entries() {
        let entries: Vec<(&str, u8, &str, &[u8])> =
            (0..17).map(|_| ("./", b'5', "", &b""[..])).collect();
        let archive = raw_archive(&entries);
        assert_fixed_error(&archive, &sha256_hex(&archive));
    }

    #[test]
    fn dev_archive_checks_the_hash_before_decompressing() {
        let junk = b"this is not gzip".to_vec();
        match OnnxEnrollmentModel::from_dev_archive(&junk, &"00".repeat(32)) {
            Err(EnrollmentError::Model(message)) => {
                assert_eq!(message, "development archive hash mismatch");
            }
            other => panic!("expected hash mismatch, got ok={}", other.is_ok()),
        }
    }

    #[test]
    fn dev_archive_tolerates_a_root_directory_entry() -> TestResult {
        let archive = raw_archive(&[
            ("./", b'5', "", b""),
            ("./enrollment.onnx", b'0', "", &onnx_bytes()),
        ]);
        OnnxEnrollmentModel::from_dev_archive(&archive, &sha256_hex(&archive))?;
        Ok(())
    }

    #[test]
    fn dev_archive_loads_the_fixture_model() -> TestResult {
        let good = archive_with_members(&[("enrollment.onnx", &onnx_bytes())]);
        let model = OnnxEnrollmentModel::from_dev_archive(&good, &sha256_hex(&good))?;
        let mut engine = SpeakerEnrollmentEngine::new(model);
        let mut recording = EnrollmentRecording::new(tone(0.3, 0.0), ENROLLMENT_SAMPLE_RATE_HZ);
        let profile = engine.enroll(
            &mut recording,
            &ProfileMetadata {
                id: "spk-dev".to_owned(),
                name: "Speaker".to_owned(),
                created_at_utc: "2026-10-02T12:00:00Z".to_owned(),
            },
            None,
        )?;
        profile.verify_integrity()?;
        assert!(!profile.film.is_identity());
        Ok(())
    }

    #[test]
    #[ignore = "reads the real M3 archive"]
    fn dev_archive_loads_the_real_m3_archive() -> TestResult {
        let path =
            "/home/joaorura/orca/projects/clearcore-train/runs/m3/voice-enrollment-asset-v1.tar.gz";
        let Ok(bytes) = fs::read(path) else {
            eprintln!("real M3 archive not present; skipping");
            return Ok(());
        };
        OnnxEnrollmentModel::from_dev_archive(&bytes, &sha256_hex(&bytes))?;
        Ok(())
    }

    #[test]
    fn garbage_onnx_fails_at_load_time() {
        assert!(matches!(
            OnnxEnrollmentModel::from_onnx_bytes(vec![1, 2, 3, 4]),
            Err(EnrollmentError::Model(_))
        ));
    }
}
