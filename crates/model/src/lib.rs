#![allow(clippy::missing_errors_doc)]

mod archive;
#[cfg(test)]
mod archive_adversarial_tests;
mod asset_manifest;
mod error;
mod golden;
mod json;
mod m0_records;
#[cfg(feature = "tract")]
mod tract_backend;

use realtime_noise_contracts::AudioFrame;

pub use asset_manifest::{APPROVED_ASSET_SHA256, ApprovedAssetManifest};
pub use error::InferenceError;
pub use golden::{GoldenCase, GoldenFixture, GoldenProvenance, NumericalTolerance, frames_sha256};
#[cfg(feature = "tract")]
pub use tract_backend::TractBackend;

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
