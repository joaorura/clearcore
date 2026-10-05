//! Enrollment profile builder: from already trimmed, level-matched and joined 48 kHz speech to a
//! sealed `VoiceProfile` (microphone EQ + speaker conditioning).

use std::fmt;

use crate::enrollment::{
    ENROLLMENT_SAMPLE_RATE_HZ, EnrollmentError, EnrollmentRecording, ProfileMetadata,
    SpeakerEmbeddingModel, SpeakerEnrollmentEngine,
};
use crate::microphone_eq::{
    MicrophoneEqConfig, MicrophoneEqError, SpeechTargetCurve, estimate_microphone_eq,
};
use crate::resample::decimate_48k_to_16k;
use crate::voice_profile::VoiceProfile;

/// Longest joined speech (seconds, 48 kHz) the builder accepts.
pub const MAX_JOINED_SPEECH_SECONDS: f32 = 90.0;

const JOINED_SAMPLE_RATE_HZ: u32 = 48_000;
const MAX_JOINED_SAMPLES: usize = 90 * 48_000;

#[derive(Debug)]
pub enum BuildError {
    Eq(MicrophoneEqError),
    Enroll(EnrollmentError),
    TooMuchSpeech,
}

impl fmt::Display for BuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Eq(error) => write!(f, "microphone equalization failed: {error}"),
            Self::Enroll(error) => write!(f, "enrollment failed: {error}"),
            Self::TooMuchSpeech => write!(
                f,
                "joined speech exceeds the {MAX_JOINED_SPEECH_SECONDS} s limit"
            ),
        }
    }
}

impl std::error::Error for BuildError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Eq(error) => Some(error),
            Self::Enroll(error) => Some(error),
            Self::TooMuchSpeech => None,
        }
    }
}

/// Builds the sealed profile from `joined_48k`: already trimmed, level-matched and joined
/// 48 kHz mono speech.
///
/// Rejects more than 90 s before doing any work, estimates the microphone EQ at 48 kHz,
/// decimates to 16 kHz and enrolls. The 16 kHz copy is wiped by the engine whatever the outcome.
pub fn build_profile<M: SpeakerEmbeddingModel>(
    engine: &mut SpeakerEnrollmentEngine<M>,
    joined_48k: &[f32],
    metadata: &ProfileMetadata,
) -> Result<VoiceProfile, BuildError> {
    if joined_48k.len() > MAX_JOINED_SAMPLES {
        return Err(BuildError::TooMuchSpeech);
    }
    let eq = estimate_microphone_eq(
        joined_48k,
        JOINED_SAMPLE_RATE_HZ,
        &SpeechTargetCurve::default(),
        &MicrophoneEqConfig::default(),
    )
    .map_err(BuildError::Eq)?;
    let mut recording =
        EnrollmentRecording::new(decimate_48k_to_16k(joined_48k), ENROLLMENT_SAMPLE_RATE_HZ);
    engine
        .enroll(&mut recording, metadata, Some(eq))
        .map_err(BuildError::Enroll)
}

#[cfg(test)]
#[allow(clippy::suboptimal_flops)]
mod tests {
    use super::*;
    use crate::enrollment::RawFilmVectors;
    use crate::voice_profile::FILM_HIDDEN_DIM;
    use std::cell::Cell;
    use std::rc::Rc;

    const RATE_48K: usize = 48_000;

    /// Speech-like 48 kHz signal: 150 Hz voiced source with harmonics under a 3 Hz envelope that
    /// never reaches silence, so every analysis frame counts as speech.
    fn speech_48k(seconds: usize) -> Vec<f32> {
        (0..seconds * RATE_48K)
            .map(|n| {
                #[allow(clippy::cast_precision_loss)]
                let t = n as f32 / 48_000.0;
                let envelope = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
                let tau = 2.0 * std::f32::consts::PI * 150.0 * t;
                let voiced = 0.5 * tau.sin()
                    + 0.3 * (2.0 * tau).sin()
                    + 0.2 * (3.0 * tau).sin()
                    + 0.1 * (6.0 * tau).sin();
                0.3 * envelope * voiced
            })
            .collect()
    }

    struct RecordingModel {
        seen_len: Rc<Cell<Option<usize>>>,
    }

    impl SpeakerEmbeddingModel for RecordingModel {
        fn extract(&mut self, samples_16khz: &[f32]) -> Result<RawFilmVectors, EnrollmentError> {
            self.seen_len.set(Some(samples_16khz.len()));
            Ok(RawFilmVectors {
                gamma_enc: vec![1.1; FILM_HIDDEN_DIM],
                beta_enc: vec![0.02; FILM_HIDDEN_DIM],
                gamma_df: vec![0.9; FILM_HIDDEN_DIM],
                beta_df: vec![-0.02; FILM_HIDDEN_DIM],
            })
        }
    }

    fn metadata() -> ProfileMetadata {
        ProfileMetadata {
            id: "spk-builder".to_owned(),
            name: "Speaker".to_owned(),
            created_at_utc: "2026-10-05T12:00:00Z".to_owned(),
        }
    }

    fn engine() -> (
        SpeakerEnrollmentEngine<RecordingModel>,
        Rc<Cell<Option<usize>>>,
    ) {
        let seen_len = Rc::new(Cell::new(None));
        let engine = SpeakerEnrollmentEngine::new(RecordingModel {
            seen_len: Rc::clone(&seen_len),
        });
        (engine, seen_len)
    }

    #[test]
    fn twenty_seconds_build_a_sealed_profile_with_eq() -> Result<(), Box<dyn std::error::Error>> {
        let (mut engine, _) = engine();
        let profile = build_profile(&mut engine, &speech_48k(20), &metadata())?;
        assert!(profile.eq.is_some());
        assert!(!profile.integrity_hash.is_empty());
        profile.verify_integrity()?;
        Ok(())
    }

    #[test]
    fn model_receives_exactly_a_third_of_the_samples() -> Result<(), Box<dyn std::error::Error>> {
        let joined = speech_48k(20);
        let (mut engine, seen_len) = engine();
        build_profile(&mut engine, &joined, &metadata())?;
        assert_eq!(seen_len.get(), Some(joined.len() / 3));
        assert_eq!(joined.len() / 3, 20 * ENROLLMENT_SAMPLE_RATE_HZ as usize);
        Ok(())
    }

    #[test]
    fn ninety_one_seconds_are_too_much_and_the_model_is_untouched() {
        let (mut engine, seen_len) = engine();
        let result = build_profile(&mut engine, &speech_48k(91), &metadata());
        assert!(matches!(result, Err(BuildError::TooMuchSpeech)));
        assert_eq!(seen_len.get(), None);
    }

    #[test]
    fn exactly_ninety_seconds_pass_the_ceiling() {
        let (mut engine, _) = engine();
        let result = build_profile(&mut engine, &speech_48k(90), &metadata());
        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn three_seconds_are_too_short_for_enrollment() {
        let (mut engine, _) = engine();
        let result = build_profile(&mut engine, &speech_48k(3), &metadata());
        assert!(
            matches!(
                result,
                Err(BuildError::Enroll(EnrollmentError::TooShort { .. }))
            ),
            "{result:?}"
        );
    }

    #[test]
    fn eq_failure_is_reported_as_eq() {
        let (mut engine, _) = engine();
        let result = build_profile(&mut engine, &[], &metadata());
        assert!(matches!(result, Err(BuildError::Eq(_))), "{result:?}");
    }

    #[cfg(feature = "tract")]
    #[test]
    fn full_path_through_the_onnx_contract_model() -> Result<(), Box<dyn std::error::Error>> {
        use crate::enrollment::OnnxEnrollmentModel;
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/enrollment-contract-test.onnx");
        let bytes = std::fs::read(path)?;
        let mut engine = SpeakerEnrollmentEngine::new(OnnxEnrollmentModel::from_onnx_bytes(bytes)?);
        let profile = build_profile(&mut engine, &speech_48k(20), &metadata())?;
        profile.verify_integrity()?;
        assert!(profile.eq.is_some());
        assert!(!profile.film.is_identity());
        Ok(())
    }
}
