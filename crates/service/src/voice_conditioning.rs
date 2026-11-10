//! Delivery of the stored voice profile (pDFNet3 Pro v3) to the audio backend, and the service's
//! view of what is stored and what is applied.
//!
//! # Interface to the supervisor
//!
//! The service hands the backend a [`VoiceConditioning`] through [`VoiceConditioningTarget`]:
//!
//! ```ignore
//! pub struct VoiceConditioning {
//!     pub profile_id: String,
//!     pub model_id: String,                    // "pdfnet3-pro-v3-full"
//!     pub speaker_embedding: [f32; 192],       // ECAPA, L2-normalized
//!     pub eq: Option<BandGains>,               // 32 ERB gains in dB, or None
//!     pub intensity_percent: u8,               // 0-100 voice isolation intensity
//! }
//! trait VoiceConditioningTarget {
//!     fn supports_voice_conditioning(&self) -> bool;
//!     fn apply_voice_conditioning(&mut self, c: Option<&VoiceConditioning>) -> Result<(), ConditioningRejected>;
//!     fn active_conditioning_id(&self) -> Option<&str>;
//!     fn active_conditioning_has_eq(&self) -> bool;
//! }
//! ```
//!
//! `None` returns the backend to the neutral voice. [`EngineSupervisor`] implements the trait
//! natively: the embedding, the EQ and the intensity go to `apply_speaker_conditioning`, and the
//! supervisor derives `speaker_present` (binary) from the intensity.

use realtime_noise_model::voice_profile_v2::{
    OutdatedProfile, PDFNET3_PRO_V3_MODEL_ID, SPEAKER_EMBEDDING_DIM, VoiceProfileV2,
};
use realtime_noise_model::{
    BandGains, ProfileStore, SpeakerConditioning, StoredProfile, VoiceProfileError,
};
use realtime_noise_supervisor::EngineSupervisor;

use realtime_noise_ipc::enrollment_codes::ENROLL_PROFILE_OUTDATED;

/// Generic, constant reasons (never payload).
pub(crate) const NOT_APPLIED_MESSAGE: &str = "The backend could not apply the stored voice profile";
pub(crate) const LOAD_FAILED_MESSAGE: &str =
    "The stored voice profile failed validation or has insecure permissions";
pub(crate) const MISSING_MESSAGE: &str = "The stored voice profile is no longer on disk";
pub(crate) const RESTORE_FAILED_MESSAGE: &str =
    "The previous voice profile could not be restored on the backend";
/// `GetStatus.voice_profile_error` of a stored v1 profile: the IPC code itself.
pub(crate) const OUTDATED_ERROR: &str = ENROLL_PROFILE_OUTDATED;

/// What the audio backend needs of a voice profile.
#[derive(Clone, PartialEq)]
pub struct VoiceConditioning {
    pub profile_id: String,
    pub model_id: String,
    pub speaker_embedding: [f32; SPEAKER_EMBEDDING_DIM],
    pub eq: Option<BandGains>,
    pub intensity_percent: u8,
}

impl std::fmt::Debug for VoiceConditioning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Biometric data: the embedding is never printed.
        f.debug_struct("VoiceConditioning")
            .field("profile_id", &self.profile_id)
            .field("model_id", &self.model_id)
            .field("has_eq", &self.eq.is_some())
            .field("intensity_percent", &self.intensity_percent)
            .finish_non_exhaustive()
    }
}

impl VoiceConditioning {
    /// Validated conditioning of a stored profile at the given intensity.
    pub fn from_profile(
        profile: &VoiceProfileV2,
        intensity_percent: u8,
    ) -> Result<Self, VoiceProfileError> {
        let embedding = profile.speaker_embedding()?;
        Ok(Self {
            profile_id: profile.id.clone(),
            model_id: profile.model_id.clone(),
            speaker_embedding: *embedding.as_slice(),
            eq: profile.eq.clone(),
            intensity_percent: intensity_percent.min(100),
        })
    }
}

/// The backend refused the conditioning (no payload).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConditioningRejected;

/// Where the service delivers a [`VoiceConditioning`] (implemented by the supervisor).
pub trait VoiceConditioningTarget {
    /// The live backend can apply a conditioning at all.
    fn supports_voice_conditioning(&self) -> bool;
    /// Applies the conditioning; `None` goes back to the neutral voice. On error nothing changed.
    fn apply_voice_conditioning(
        &mut self,
        conditioning: Option<&VoiceConditioning>,
    ) -> Result<(), ConditioningRejected>;
    fn active_conditioning_id(&self) -> Option<&str>;
    fn active_conditioning_has_eq(&self) -> bool;
}

impl VoiceConditioning {
    /// The supervisor's view of this conditioning (same fields; `speaker_present` is derived from
    /// the intensity by [`SpeakerConditioning::speaker_present`], never here).
    #[must_use]
    pub fn to_speaker_conditioning(&self) -> SpeakerConditioning {
        SpeakerConditioning {
            profile_id: self.profile_id.clone(),
            model_id: self.model_id.clone(),
            speaker_embedding: self.speaker_embedding,
            eq: self.eq.clone(),
            intensity_percent: self.intensity_percent,
        }
    }
}

/// Native delivery to the supervisor: the speaker embedding, the EQ and the intensity reach the
/// pDFNet3 Pro v3 full graphs through [`EngineSupervisor::apply_speaker_conditioning`].
impl VoiceConditioningTarget for EngineSupervisor {
    fn supports_voice_conditioning(&self) -> bool {
        self.supports_speaker_embedding()
    }

    fn apply_voice_conditioning(
        &mut self,
        conditioning: Option<&VoiceConditioning>,
    ) -> Result<(), ConditioningRejected> {
        let speaker = conditioning.map(VoiceConditioning::to_speaker_conditioning);
        self.apply_speaker_conditioning(speaker.as_ref())
            .map_err(|_| ConditioningRejected)
    }

    fn active_conditioning_id(&self) -> Option<&str> {
        EngineSupervisor::active_conditioning_id(self)
    }

    fn active_conditioning_has_eq(&self) -> bool {
        EngineSupervisor::active_conditioning_has_eq(self)
    }
}

/// What the service knows about the stored profile and what it has delivered.
#[derive(Debug, Default)]
pub(crate) struct VoiceState {
    /// Id of the valid (v2) profile persisted on disk.
    pub stored_id: Option<String>,
    /// Generic reason the stored profile is not applied; `ENROLL_PROFILE_OUTDATED` for a v1 file.
    pub error: Option<String>,
    /// A v1 (`FiLM`) profile is on disk: it must be regenerated from the stored samples.
    pub outdated: Option<OutdatedProfile>,
    /// The conditioning last delivered successfully (restore point after a failed persist).
    pub applied: Option<VoiceConditioning>,
}

impl VoiceState {
    /// `none`, `active`, `stored` (valid but not applied) or `outdated`.
    pub fn status_name(&self, applied_id: Option<&str>) -> &'static str {
        if self.outdated.is_some() {
            "outdated"
        } else if self.stored_id.is_some() && applied_id.is_some() {
            "active"
        } else if self.stored_id.is_some() {
            "stored"
        } else {
            "none"
        }
    }

    /// Records what the store holds and (when `apply`) delivers it. Never panics, never carries
    /// payload in `error`.
    pub fn load_and_apply(
        &mut self,
        target: &mut impl VoiceConditioningTarget,
        store: &ProfileStore,
        apply: bool,
        intensity_percent: u8,
    ) {
        match store.load_active_any() {
            Ok(Some(StoredProfile::Current(profile))) => {
                self.outdated = None;
                self.stored_id = Some(profile.id.clone());
                if !apply {
                    self.error = None;
                    return;
                }
                self.deliver(target, &profile, intensity_percent);
            }
            Ok(Some(StoredProfile::Outdated(info))) => {
                // Never applied: the audio stays neutral and the UI asks to regenerate.
                let _ = target.apply_voice_conditioning(None);
                self.applied = None;
                self.stored_id = None;
                self.outdated = Some(info);
                self.error = Some(OUTDATED_ERROR.to_owned());
            }
            Ok(None) => {
                if self.stored_id.is_some() {
                    // The file vanished behind our back: the id no longer names anything on disk.
                    self.stored_id = None;
                    self.error = Some(MISSING_MESSAGE.to_owned());
                }
                self.outdated = None;
            }
            Err(_) => {
                self.error = Some(LOAD_FAILED_MESSAGE.to_owned());
            }
        }
    }

    fn deliver(
        &mut self,
        target: &mut impl VoiceConditioningTarget,
        profile: &VoiceProfileV2,
        intensity_percent: u8,
    ) {
        let result = VoiceConditioning::from_profile(profile, intensity_percent)
            .map_err(|_| ConditioningRejected)
            .and_then(|conditioning| {
                target.apply_voice_conditioning(Some(&conditioning))?;
                Ok(conditioning)
            });
        match result {
            Ok(conditioning) => {
                self.applied = Some(conditioning);
                self.error = None;
            }
            Err(ConditioningRejected) => {
                self.error = Some(NOT_APPLIED_MESSAGE.to_owned());
            }
        }
    }

    /// Puts the backend back on `applied` after a failed persist; a failed restore is surfaced.
    pub fn restore(
        &mut self,
        target: &mut impl VoiceConditioningTarget,
        previous: Option<VoiceConditioning>,
    ) {
        if target.apply_voice_conditioning(previous.as_ref()).is_err() {
            eprintln!("Previous voice profile could not be restored on the backend");
            self.error = Some(RESTORE_FAILED_MESSAGE.to_owned());
        } else {
            self.applied = previous;
        }
    }

    /// Identity of the backend model the profile targets (status field).
    pub const fn model_id() -> &'static str {
        PDFNET3_PRO_V3_MODEL_ID
    }
}
