#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    BackendDescriptor, FILM_HIDDEN_DIM, FiLMVectors, InferenceBackend, InferenceError,
    ProcessedFrame, VoiceProfile, reject_unsupported_voice_profile,
};
use realtime_noise_supervisor::EngineSupervisor;

/// Test backend. When `supports` is true it accepts every profile except the one whose id is
/// `"bad"`, which it rejects, so the failure path can be exercised on a supporting backend.
struct ProfileBackend {
    supports: bool,
}

impl ProfileBackend {
    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "passthrough",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "test".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }
}

impl InferenceBackend for ProfileBackend {
    fn descriptor(&self) -> BackendDescriptor {
        Self::descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        ProcessedFrame::checked(*input, 1_440, Self::descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        1_440
    }

    fn set_voice_profile(&mut self, p: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        if !self.supports {
            return reject_unsupported_voice_profile(p);
        }
        match p {
            Some(profile) if profile.id == "bad" => {
                Err(InferenceError::UnsupportedFeature("bad profile".into()))
            }
            _ => Ok(()),
        }
    }
}

fn profile(id: &str) -> VoiceProfile {
    VoiceProfile::identity(id, "n", "2026-10-05T00:00:00Z").unwrap()
}

#[test]
fn applied_profile_is_reported_only_after_backend_success() {
    let mut sup = EngineSupervisor::default();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "fake");
    sup.set_voice_profile(Some(&profile("a"))).unwrap();
    assert_eq!(sup.active_voice_profile_id(), Some("a"));
    assert_eq!(sup.active_voice_profile().map(|p| p.id.as_str()), Some("a"));
    sup.set_voice_profile(None).unwrap();
    assert_eq!(sup.active_voice_profile_id(), None);
}

#[test]
fn unsupported_backend_never_reports_active() {
    let mut sup = EngineSupervisor::default();
    sup.set_backend(Box::new(ProfileBackend { supports: false }), "fake");
    assert!(sup.set_voice_profile(Some(&profile("a"))).is_err());
    assert_eq!(sup.active_voice_profile_id(), None);
}

#[test]
fn failed_set_keeps_the_previous_profile() {
    let mut sup = EngineSupervisor::default();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "ok");
    sup.set_voice_profile(Some(&profile("a"))).unwrap();
    assert!(sup.set_voice_profile(Some(&profile("bad"))).is_err());
    assert_eq!(sup.active_voice_profile_id(), Some("a"));
}

#[test]
fn no_backend_is_an_error() {
    let mut sup = EngineSupervisor::default();
    let err = sup.set_voice_profile(Some(&profile("a"))).unwrap_err();
    assert!(matches!(err, InferenceError::UnsupportedFeature(_)));
    assert_eq!(sup.active_voice_profile_id(), None);
}

#[test]
fn backend_swap_to_supported_reapplies_the_profile() {
    let mut sup = EngineSupervisor::default();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "a");
    sup.set_voice_profile(Some(&profile("a"))).unwrap();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "b");
    assert_eq!(sup.active_voice_profile_id(), Some("a"));
}

#[test]
fn backend_swap_to_unsupported_drops_the_active_profile() {
    let mut sup = EngineSupervisor::default();
    sup.set_backend(Box::new(ProfileBackend { supports: true }), "a");
    sup.set_voice_profile(Some(&profile("a"))).unwrap();
    sup.set_backend(Box::new(ProfileBackend { supports: false }), "b");
    assert_eq!(sup.active_voice_profile_id(), None);
}

/// Confirms on the real stack why `BuildVoiceProfile` cannot activate a profile on the shipped
/// model: the approved base `DFNet3` (`vendor/approved/df-compatible-release-asset-v1.bin`, the
/// same weights as `models/stateful`) declares no `gamma`/`beta` inputs, so the real
/// `TractBackend` refuses any non-identity profile.
#[test]
fn real_tract_base_model_refuses_a_conditioned_profile() {
    let root = realtime_noise_supervisor::find_repo_root().expect("repo root");
    let mut sup = EngineSupervisor::default();
    let info = sup.select_backend("tract", None, Some(&root));
    assert_eq!(
        info.name, "tract",
        "the real tract backend must load, not the mock"
    );
    let film = FiLMVectors::new(
        vec![1.1; FILM_HIDDEN_DIM],
        vec![0.02; FILM_HIDDEN_DIM],
        vec![0.9; FILM_HIDDEN_DIM],
        vec![-0.02; FILM_HIDDEN_DIM],
    )
    .unwrap();
    let built = VoiceProfile::new("p-built", "n", "2026-10-05T00:00:00Z", film, None).unwrap();
    let err = sup.set_voice_profile(Some(&built)).unwrap_err();
    eprintln!("real tract set_voice_profile(non-identity) -> {err:?}");
    assert!(
        matches!(&err, InferenceError::InputContract(m)
            if m == "backend model does not support speaker conditioning"),
        "{err:?}"
    );
    assert_eq!(sup.active_voice_profile_id(), None);
    // The neutral profile is still accepted by the same backend.
    sup.set_voice_profile(Some(&profile("neutral"))).unwrap();
}
