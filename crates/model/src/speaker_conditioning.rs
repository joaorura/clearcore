//! Conditioning of the pDFNet3 Pro v3 full on a voice profile.
//!
//! Fields: the ECAPA speaker embedding
//! (`speaker_embedding`, `[1, 192]`, L2-normalised), the profile's spectral EQ and the isolation
//! intensity. Same fields as the service's `VoiceConditioning`, so the service hands it to
//! [`crate::InferenceBackend::set_speaker_conditioning`] (through the supervisor) as is.

use std::fmt;

use crate::{BandGains, InferenceError};

/// Values of the speaker embedding.
pub const SPEAKER_EMBEDDING_LEN: usize = 192;
/// Tolerance on the L2 norm of the embedding (it must be unit-norm).
pub const SPEAKER_EMBEDDING_NORM_TOLERANCE: f32 = 1e-2;

/// What a backend needs to condition the Pro v3 full on a speaker.
#[derive(Clone, PartialEq)]
pub struct SpeakerConditioning {
    pub profile_id: String,
    /// Model the embedding was built for (`"pdfnet3-pro-v3-full"`).
    pub model_id: String,
    pub speaker_embedding: [f32; SPEAKER_EMBEDDING_LEN],
    /// Spectral EQ applied right before the iSTFT (32 ERB gains in dB), or none.
    pub eq: Option<BandGains>,
    /// 0-100. Only gates the speaker: 0 → `speaker_present = 0.0` (neutral voice), anything else
    /// → `1.0` with the whole embedding (binary, as the training mask; the same rule as the filter).
    pub intensity_percent: u8,
    /// pDFNet3 Pro v7 speaker tokens (`enroll_tokens`, 48 x 128) of a v7 profile; `None` for a
    /// profile built for the v3 model, which the v7 runtime refuses.
    pub speaker_tokens: Option<std::sync::Arc<[f32]>>,
}

impl fmt::Debug for SpeakerConditioning {
    // The embedding is biometric data: never formatted.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SpeakerConditioning")
            .field("profile_id", &self.profile_id)
            .field("model_id", &self.model_id)
            .field("has_eq", &self.eq.is_some())
            .field("intensity_percent", &self.intensity_percent)
            .field("has_tokens", &self.speaker_tokens.is_some())
            .finish_non_exhaustive()
    }
}

impl SpeakerConditioning {
    /// Embedding finite and unit-norm, EQ within bounds, intensity <= 100.
    pub fn validate(&self) -> Result<(), InferenceError> {
        let invalid = |what: String| InferenceError::InputContract(what);
        if self.speaker_embedding.iter().any(|v| !v.is_finite()) {
            return Err(invalid(
                "speaker embedding has a non-finite value".to_owned(),
            ));
        }
        let norm = self
            .speaker_embedding
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt();
        if (norm - 1.0).abs() > SPEAKER_EMBEDDING_NORM_TOLERANCE {
            return Err(invalid(format!(
                "speaker embedding must be L2-normalised (norm {norm})"
            )));
        }
        if self.intensity_percent > 100 {
            return Err(invalid("intensity must be within 0-100".to_owned()));
        }
        if let Some(eq) = &self.eq {
            eq.validate()?;
        }
        if let Some(tokens) = &self.speaker_tokens
            && (tokens.len() != crate::voice_profile_v2::SPEAKER_TOKENS_LEN
                || tokens.iter().any(|v| !v.is_finite()))
        {
            return Err(invalid(
                "speaker tokens must be 48 x 128 finite values".to_owned(),
            ));
        }
        Ok(())
    }

    /// Value fed to `speaker_present`: binary (`0.0` at intensity 0, `1.0` otherwise); a
    /// fractional value would be outside the training distribution (the mask is 0/1).
    #[must_use]
    pub const fn speaker_present(&self) -> f32 {
        if self.intensity_percent == 0 {
            0.0
        } else {
            1.0
        }
    }
}
