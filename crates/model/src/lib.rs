#![allow(clippy::missing_errors_doc)]

mod archive;
#[cfg(test)]
mod archive_adversarial_tests;
mod asset_manifest;
#[cfg(feature = "tract")]
pub mod dsp_pipeline;
pub mod enrollment;
pub mod enrollment_builder;
mod error;
mod golden;
mod json;
mod m0_records;
pub mod microphone_eq;
pub mod model_registry;
pub mod profile_store;
pub mod resample;
pub mod spectral_eq;
pub mod speech_trim;
mod studio_backend;
#[cfg(feature = "tract")]
mod tract_backend;
pub mod voice_profile;
pub mod wav;

use realtime_noise_contracts::AudioFrame;

pub use asset_manifest::{APPROVED_ASSET_SHA256, ApprovedAssetManifest};
#[cfg(feature = "tract")]
pub use dsp_pipeline::{
    AgnosticDspBackend, DspPipeline, DspPipelineConfig, ExtractedFeatures, ModelFrameOutput,
    SpectralModelBackend,
};
pub use error::InferenceError;
pub use golden::{GoldenCase, GoldenFixture, GoldenProvenance, NumericalTolerance, frames_sha256};
pub use microphone_eq::{
    LtasMeasurement, MicrophoneEqConfig, MicrophoneEqError, SpeechTargetCurve, compute_ltas,
    embed_eq_in_profile, estimate_microphone_eq,
};
pub use model_registry::{
    AssetDescriptor, DEV_KEY_ID, ModelAssetRegistry, ModelRole, VerifiedAsset,
};
pub use profile_store::{ACTIVE_PROFILE_FILE_NAME, ProfileStore};
pub use studio_backend::{StudioBackend, StudioResetHandle};
#[cfg(feature = "tract")]
pub use tract_backend::TractBackend;
pub use voice_profile::{
    BandGains, FILM_HIDDEN_DIM, FiLMVectors, MAX_EQ_GAIN_DB, MIN_EQ_GAIN_DB, NUM_ERB_BANDS,
    VoiceProfile, VoiceProfileError,
};

pub const ALGORITHM_LATENCY_SAMPLES: u32 = 1_440;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuProfile {
    Avx2Minimum,
}

impl CpuProfile {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Avx2Minimum => "avx2-minimum",
        }
    }

    pub fn ensure_supported(self) -> Result<(), InferenceError> {
        self.ensure_supported_with(avx2_available())
    }

    fn ensure_supported_with(self, avx2: bool) -> Result<(), InferenceError> {
        match self {
            Self::Avx2Minimum if avx2 => Ok(()),
            Self::Avx2Minimum => Err(InferenceError::UnsupportedCpuProfile(
                "avx2-minimum requires an x86_64 CPU with AVX2".to_owned(),
            )),
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn avx2_available() -> bool {
    std::arch::is_x86_feature_detected!("avx2")
}

#[cfg(not(target_arch = "x86_64"))]
const fn avx2_available() -> bool {
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendDescriptor {
    pub backend: &'static str,
    pub backend_version: &'static str,
    pub runtime: &'static str,
    pub runtime_version: &'static str,
    pub asset_id: String,
    pub asset_sha256: String,
    pub cpu_profile: &'static str,
}

#[derive(Debug, Clone)]
pub struct ProcessedFrame {
    pub samples: AudioFrame,
    pub algorithmic_latency_samples: u32,
    pub provenance: BackendDescriptor,
}

impl ProcessedFrame {
    pub fn checked(
        samples: AudioFrame,
        algorithmic_latency_samples: u32,
        provenance: BackendDescriptor,
    ) -> Result<Self, InferenceError> {
        if samples.iter().all(|sample| sample.is_finite()) {
            Ok(Self {
                samples,
                algorithmic_latency_samples,
                provenance,
            })
        } else {
            Err(InferenceError::NonFiniteOutput)
        }
    }
}

pub trait InferenceBackend: Send {
    fn descriptor(&self) -> BackendDescriptor;
    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError>;
    fn algorithmic_latency_samples(&self) -> u32;
    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError>;

    /// Cheap capability check: whether [`Self::set_voice_profile`] can apply a conditioned
    /// (non-identity) profile, so the service can refuse an enrollment build up front instead
    /// of spending it on a profile the backend will reject. Backends that reject every profile
    /// through [`reject_unsupported_voice_profile`] must return `false`; the default (`true`)
    /// covers backends that accept profiles (test doubles included).
    fn supports_voice_profile(&self) -> bool {
        true
    }
}

/// Policy for backends that cannot condition on a voice profile: `None` is
/// accepted, `Some` is rejected explicitly instead of being silently ignored.
pub fn reject_unsupported_voice_profile(
    profile: Option<&VoiceProfile>,
) -> Result<(), InferenceError> {
    match profile {
        None => Ok(()),
        Some(_) => Err(InferenceError::UnsupportedFeature(
            "voice profile conditioning".into(),
        )),
    }
}

impl<T: InferenceBackend + ?Sized> InferenceBackend for Box<T> {
    fn descriptor(&self) -> BackendDescriptor {
        (**self).descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        (**self).process(input)
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        (**self).algorithmic_latency_samples()
    }

    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        (**self).set_voice_profile(profile)
    }

    fn supports_voice_profile(&self) -> bool {
        (**self).supports_voice_profile()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_policy_accepts_none_and_rejects_some() {
        assert!(reject_unsupported_voice_profile(None).is_ok());
        let p = VoiceProfile::identity("id", "n", "2026-10-05T00:00:00Z").unwrap();
        let err = reject_unsupported_voice_profile(Some(&p)).unwrap_err();
        assert!(matches!(err, InferenceError::UnsupportedFeature(_)));
    }

    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "test",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "test".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }

    #[test]
    fn unsupported_cpu_profile_is_typed() {
        assert!(matches!(
            CpuProfile::Avx2Minimum.ensure_supported_with(false),
            Err(InferenceError::UnsupportedCpuProfile(_))
        ));
    }

    #[test]
    fn processed_frame_rejects_non_finite_output() {
        let mut samples = [0.0; realtime_noise_contracts::HOP_SAMPLES];
        samples[0] = f32::NAN;
        assert!(matches!(
            ProcessedFrame::checked(samples, ALGORITHM_LATENCY_SAMPLES, descriptor()),
            Err(InferenceError::NonFiniteOutput)
        ));
    }
}
