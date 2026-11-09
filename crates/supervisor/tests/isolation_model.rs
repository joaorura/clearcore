//! Production policy: the bundled pDFNet3 Pro is the only isolation model, and
//! `supports_voice_profile()` is true exactly when `set_voice_profile()` accepts a conditioned
//! profile.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use realtime_noise_accelerators::BackendSelection;
use realtime_noise_model::{
    FILM_HIDDEN_DIM, FiLMVectors, PDFNET3_PRO_ASSET_ID, VoiceProfile, pdfnet3_bundle,
};
use realtime_noise_supervisor::{
    EngineSupervisor, ISOLATION_MODEL_BASE, ISOLATION_MODEL_NONE, ISOLATION_MODEL_PDFNET3_PRO,
};

fn repository_models() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
}

fn conditioned(id: &str) -> VoiceProfile {
    let film = FiLMVectors::new(
        vec![1.1; FILM_HIDDEN_DIM],
        vec![0.02; FILM_HIDDEN_DIM],
        vec![0.9; FILM_HIDDEN_DIM],
        vec![-0.02; FILM_HIDDEN_DIM],
    )
    .unwrap();
    VoiceProfile::new(id, "Speaker", "2026-11-09T12:00:00Z", film, None).unwrap()
}

/// The invariant the UI relies on: `supports` announces exactly what `set` accepts.
fn assert_supports_matches_set(supervisor: &mut EngineSupervisor, context: &str) {
    let supports = supervisor.supports_voice_profile();
    let applied = supervisor
        .set_voice_profile(Some(&conditioned("p")))
        .is_ok();
    assert_eq!(supports, applied, "supports != set for {context}");
    assert_eq!(
        supervisor.active_voice_profile_id().is_some(),
        applied,
        "{context}"
    );
}

/// Bundled Pro with the OpenVINO path made unavailable (its stateful export "missing"), so the
/// selection is deterministic on hosts with and without OpenVINO.
fn pro_supervisor() -> EngineSupervisor {
    let archive = pdfnet3_bundle::load_tract_archive_from(&[repository_models()]).unwrap();
    let mut supervisor = EngineSupervisor::default();
    supervisor.require_isolation_model(Ok(archive));
    supervisor.set_stateful_source(Some(Box::new(|| Err("MODEL_ASSET_NOT_FOUND"))));
    supervisor
}

/// Bundled Pro with the real bundled stateful export, for the OpenVINO path.
fn pro_supervisor_with_openvino() -> EngineSupervisor {
    let archive = pdfnet3_bundle::load_tract_archive_from(&[repository_models()]).unwrap();
    let mut supervisor = EngineSupervisor::default();
    supervisor.require_isolation_model(Ok(archive));
    supervisor.set_stateful_source(Some(Box::new(|| {
        pdfnet3_bundle::load_stateful_archive_from(&[repository_models()])
            .map_err(realtime_noise_model::BundledModelError::code)
    })));
    supervisor
}

fn openvino_cpu_usable() -> bool {
    let probe = realtime_noise_runtime_openvino::probe_runtime_cached();
    probe.state.is_usable() && probe.devices.iter().any(|device| device == "CPU")
}

#[test]
fn bundled_pro_is_installed_for_every_request_and_applies_a_conditioned_profile() {
    for request in [
        "auto",
        "tract",
        "openvino-npu",
        "tensorrt",
        "gpu",
        "unknown",
    ] {
        let mut supervisor = pro_supervisor();
        let info = supervisor.select_backend(request, None, None);
        assert_eq!(info.name, "tract", "{request}");
        assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_PDFNET3_PRO);
        assert_eq!(supervisor.isolation_model_error(), None);
        assert_eq!(
            supervisor.active_backend_descriptor().unwrap().asset_id,
            PDFNET3_PRO_ASSET_ID
        );
        assert!(supervisor.supports_voice_profile(), "{request}");
        assert_supports_matches_set(&mut supervisor, request);
    }
}

#[test]
fn accelerator_request_is_reported_as_cpu_fallback_with_a_reason() {
    let mut supervisor = pro_supervisor();
    // OpenVINO request whose stateful export is unavailable: tract, with the reason (either the
    // runtime is not usable on this host or the archive code).
    let info = supervisor.select_backend("openvino-npu", None, None);
    assert_eq!(info.name, "tract");
    assert!(info.is_fallback);
    let reason = info.fallback_reason.unwrap();
    assert!(
        reason.contains("MODEL_ASSET_NOT_FOUND") || reason.contains("OpenVINO"),
        "{reason}"
    );
    assert_eq!(
        supervisor
            .last_resolution()
            .and_then(|r| r.fallback_reason.clone()),
        Some(reason)
    );
    // A non-OpenVINO accelerator cannot feed FiLM at all.
    let info = supervisor.select_backend("tensorrt", None, None);
    assert!(info.is_fallback);
    assert!(info.fallback_reason.unwrap().contains("FiLM"));
    // `auto` is tract by design, not a fallback.
    let info = supervisor.select_backend("auto", None, None);
    assert!(!info.is_fallback);
    assert_eq!(info.fallback_reason, None);
    assert_eq!(supervisor.last_resolution().unwrap().fallback_reason, None);
}

#[test]
fn openvino_cpu_runs_the_pro_with_film_and_keeps_the_profile_coherent() {
    if !openvino_cpu_usable() {
        eprintln!("OpenVINO CPU not usable on this host; skipping");
        return;
    }
    let mut supervisor = pro_supervisor_with_openvino();
    let _ = supervisor.select_backend("tract", None, None);
    supervisor
        .set_voice_profile(Some(&conditioned("kept")))
        .unwrap();

    let info = supervisor.select_backend("openvino-cpu", None, None);
    assert_eq!(info.name, "openvino-cpu", "{info:?}");
    assert!(!info.is_fallback);
    assert_eq!(supervisor.active_backend_name(), "openvino-cpu");
    assert_eq!(supervisor.active_backend_device().as_deref(), Some("CPU"));
    assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_PDFNET3_PRO);
    // The profile applied on tract was re-applied (and confirmed) by the OpenVINO backend.
    assert_eq!(supervisor.active_voice_profile_id(), Some("kept"));
    assert!(supervisor.supports_voice_profile());
    assert_supports_matches_set(&mut supervisor, "openvino-cpu");
    let frame = [0.01f32; realtime_noise_contracts::HOP_SAMPLES];
    assert!(supervisor.process_frame(&frame).is_ok());

    // An OpenVINO device that does not exist falls back to tract with the reason.
    let info = supervisor.select_backend("openvino-gpu", None, None);
    if info.name == "tract" {
        assert!(info.is_fallback);
        assert!(info.fallback_reason.unwrap().contains("GPU"));
    }
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
        assert!(!supervisor.supports_voice_profile());
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
        .set_voice_profile(Some(&conditioned("a")))
        .unwrap();
    assert_eq!(supervisor.active_voice_profile_id(), Some("a"));
    supervisor.require_isolation_model(Err("MODEL_HASH_MISMATCH"));
    let _ = supervisor.select_backend("auto", None, None);
    assert_eq!(supervisor.active_voice_profile_id(), None);
}

#[test]
fn backends_without_film_report_no_support_and_refuse_profiles() {
    // Tract/accelerator mocks have no FiLM inputs, whatever name they are installed under (the
    // old code claimed support for "tract" and accelerator names and then failed in `apply`).
    for (selection, name) in [
        (BackendSelection::TractCpu, "tract"),
        (BackendSelection::TractCpu, "cpu-tract"),
        (BackendSelection::TensorRt, "nvidia-tensorrt"),
        (BackendSelection::OpenVinoNpu, "openvino-npu"),
        (BackendSelection::OpenVinoGpu, "openvino-gpu"),
    ] {
        let mut supervisor = EngineSupervisor::default();
        supervisor.set_backend(selection.instantiate_mock_backend(), name);
        assert!(!supervisor.supports_voice_profile(), "{name}");
        assert_supports_matches_set(&mut supervisor, name);
    }
}

#[test]
fn real_base_tract_model_reports_no_support_coherently() {
    // Legacy seam (no production policy): the approved base DFNet3 on tract has no FiLM.
    let root = realtime_noise_supervisor::find_repo_root().expect("repo root");
    let mut supervisor = EngineSupervisor::default();
    let info = supervisor.select_backend("tract", None, Some(&root));
    assert_eq!(info.name, "tract");
    assert_eq!(supervisor.isolation_model(), ISOLATION_MODEL_BASE);
    assert!(!supervisor.supports_voice_profile());
    assert_supports_matches_set(&mut supervisor, "base tract");
}
