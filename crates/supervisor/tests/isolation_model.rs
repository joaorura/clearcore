//! Production policy: the bundled pDFNet3 Pro v3 full is the only isolation model, and
//! `supports_speaker_embedding()` is true exactly when `apply_speaker_conditioning()` accepts a
//! speaker conditioning.
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss
)]

use std::path::PathBuf;

use realtime_noise_accelerators::BackendSelection;
use realtime_noise_model::{PDFNET3_PRO_ASSET_ID, SpeakerConditioning, pdfnet3_bundle};
use realtime_noise_supervisor::{
    EngineSupervisor, ISOLATION_MODEL_BASE, ISOLATION_MODEL_NONE, ISOLATION_MODEL_PDFNET3_PRO,
};

fn repository_models() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
}

fn conditioned(id: &str) -> SpeakerConditioning {
    let mut speaker_embedding = [0.0f32; 192];
    for (index, value) in speaker_embedding.iter_mut().enumerate() {
        *value = ((index as f32) * 0.21).cos();
    }
    let norm = speaker_embedding.iter().map(|v| v * v).sum::<f32>().sqrt();
    speaker_embedding.iter_mut().for_each(|v| *v /= norm);
    SpeakerConditioning {
        profile_id: id.to_owned(),
        model_id: "pdfnet3-pro-v3-full".to_owned(),
        speaker_embedding,
        eq: None,
        intensity_percent: 100,
    }
}

/// The invariant the UI relies on: `supports` announces exactly what `apply` accepts.
fn assert_supports_matches_set(supervisor: &mut EngineSupervisor, context: &str) {
    let supports = supervisor.supports_speaker_embedding();
    let applied = supervisor
        .apply_speaker_conditioning(Some(&conditioned("p")))
        .is_ok();
    assert_eq!(supports, applied, "supports != apply for {context}");
    assert_eq!(
        supervisor.active_conditioning_id().is_some(),
        applied,
        "{context}"
    );
}

/// Bundled Pro v3 full; OpenVINO requests may run it on OpenVINO when the host has it.
fn pro_supervisor() -> EngineSupervisor {
    let archive = pdfnet3_bundle::load_isolation_archive_from(&[repository_models()]).unwrap();
    let mut supervisor = EngineSupervisor::default();
    supervisor.require_isolation_model(Ok(archive));
    supervisor
}

#[test]
fn bundled_pro_is_installed_for_every_request_and_applies_a_conditioned_profile() {
    // "gpu" is an OpenVINO GPU request: it may legitimately run on OpenVINO (pro_v3_full.rs).
    for request in ["auto", "tract", "tensorrt", "unknown"] {
        let mut supervisor = pro_supervisor();
        let info = supervisor.select_backend(request, None, None);
        assert_eq!(info.name, "tract", "{request}");
        assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_PDFNET3_PRO);
        assert_eq!(supervisor.isolation_model_error(), None);
        assert_eq!(
            supervisor.active_backend_descriptor().unwrap().asset_id,
            PDFNET3_PRO_ASSET_ID
        );
        assert!(supervisor.supports_speaker_embedding(), "{request}");
        assert_supports_matches_set(&mut supervisor, request);
    }
}

#[test]
fn accelerator_request_is_reported_as_cpu_fallback_with_a_reason() {
    let mut supervisor = pro_supervisor();
    // A non-OpenVINO accelerator cannot run the Pro: tract, flagged as a fallback with a reason.
    let info = supervisor.select_backend("tensorrt", None, None);
    assert_eq!(info.name, "tract");
    assert!(info.is_fallback);
    let reason = info.fallback_reason.unwrap();
    assert_eq!(
        supervisor
            .last_resolution()
            .and_then(|r| r.fallback_reason.clone()),
        Some(reason)
    );
    // An OpenVINO request either runs on OpenVINO or falls back to tract with the reason.
    let info = supervisor.select_backend("openvino-npu", None, None);
    if info.name == "tract" {
        assert!(info.is_fallback);
        assert!(info.fallback_reason.is_some());
    }
    assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_PDFNET3_PRO);
    // `auto` is tract by design, not a fallback.
    let info = supervisor.select_backend("auto", None, None);
    assert!(!info.is_fallback);
    assert_eq!(info.fallback_reason, None);
    assert_eq!(supervisor.last_resolution().unwrap().fallback_reason, None);
}

#[test]
fn missing_model_installs_no_backend_and_refuses_profiles() {
    for code in ["MODEL_ASSET_NOT_FOUND", "MODEL_HASH_MISMATCH"] {
        let mut supervisor = EngineSupervisor::default();
        supervisor.require_isolation_model(Err(code));
        let info = supervisor.select_backend("auto", None, None);
        assert_eq!(info.name, "none");
        assert!(info.is_fallback);
        assert!(info.fallback_reason.unwrap().contains(code));
        assert!(supervisor.backend().is_none(), "no base DFNet3 fallback");
        assert_eq!(supervisor.active_backend_name(), "none");
        assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_NONE);
        assert_eq!(supervisor.isolation_model_error(), Some(code));
        assert!(!supervisor.supports_speaker_embedding());
        assert_supports_matches_set(&mut supervisor, code);
        // Also for an explicit tract request.
        let _ = supervisor.select_backend("tract", None, Some(&repository_models()));
        assert!(supervisor.backend().is_none());
    }
}

#[test]
fn a_profile_applied_on_pro_is_dropped_when_the_model_becomes_unavailable() {
    let mut supervisor = pro_supervisor();
    let _ = supervisor.select_backend("auto", None, None);
    supervisor
        .apply_speaker_conditioning(Some(&conditioned("a")))
        .unwrap();
    assert_eq!(supervisor.active_conditioning_id(), Some("a"));
    supervisor.require_isolation_model(Err("MODEL_HASH_MISMATCH"));
    let _ = supervisor.select_backend("auto", None, None);
    assert_eq!(supervisor.active_conditioning_id(), None);
}

#[test]
fn backends_without_speaker_inputs_report_no_support_and_refuse_conditioning() {
    // Tract/accelerator mocks have no speaker inputs, whatever name they are installed under.
    for (selection, name) in [
        (BackendSelection::TractCpu, "tract"),
        (BackendSelection::TractCpu, "cpu-tract"),
        (BackendSelection::TensorRt, "nvidia-tensorrt"),
        (BackendSelection::OpenVinoNpu, "openvino-npu"),
        (BackendSelection::OpenVinoGpu, "openvino-gpu"),
    ] {
        let mut supervisor = EngineSupervisor::default();
        supervisor.set_backend(selection.instantiate_mock_backend(), name);
        assert!(!supervisor.supports_speaker_embedding(), "{name}");
        assert_supports_matches_set(&mut supervisor, name);
    }
}

#[test]
fn real_base_tract_model_reports_no_support_coherently() {
    // Legacy seam (no production policy): the approved base DFNet3 on tract has no speaker inputs.
    let root = realtime_noise_supervisor::find_repo_root().expect("repo root");
    let mut supervisor = EngineSupervisor::default();
    let info = supervisor.select_backend("tract", None, Some(&root));
    assert_eq!(info.name, "tract");
    assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_BASE);
    assert!(!supervisor.supports_speaker_embedding());
    assert_supports_matches_set(&mut supervisor, "base tract");
}
