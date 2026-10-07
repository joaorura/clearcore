//! The development pDFNet3 (unsigned, FiLM) overrides every backend selection while set.
#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_model::{FILM_HIDDEN_DIM, FiLMVectors, PdfNet3DevArchive, VoiceProfile};
use realtime_noise_supervisor::{DEV_BASE_MODEL_BASE, DEV_BASE_MODEL_PDFNET3, EngineSupervisor};
use std::path::Path;

const M3_PDFNET3: &str =
    "/home/joaorura/orca/projects/clearcore-train/runs/m3/pdfnet3-release-asset-v1.tar.gz";
const M3_PDFNET3_SHA256: &str = "42dfc577fdf8a881ecbafce7777bf6f0a4cf914ffc1aaff2580aec0cbac79505";

#[test]
fn without_a_development_model_the_status_is_base_without_error() {
    let mut supervisor = EngineSupervisor::default();
    assert_eq!(supervisor.dev_base_model(), DEV_BASE_MODEL_BASE);
    let _ = supervisor.select_backend("tract", None, None);
    assert_eq!(supervisor.dev_base_model(), DEV_BASE_MODEL_BASE);
    assert_eq!(supervisor.dev_base_model_error(), None);
    supervisor.set_dev_base_model_error(Some("DEV_MODEL_HASH_MISMATCH"));
    assert_eq!(
        supervisor.dev_base_model_error(),
        Some("DEV_MODEL_HASH_MISMATCH")
    );
}

#[test]
#[ignore = "reads the real M3 pDFNet3 archive from the training repository"]
fn real_pdfnet3_used_for_default_and_tract_and_accepts_a_profile() {
    if !Path::new(M3_PDFNET3).is_file() {
        eprintln!("real M3 pDFNet3 archive not present; skipping");
        return;
    }
    let archive = PdfNet3DevArchive::read(Path::new(M3_PDFNET3), M3_PDFNET3_SHA256).unwrap();
    let mut supervisor = EngineSupervisor::default();
    supervisor.set_dev_base_model(Some(archive));
    let info = supervisor.select_backend("tract", None, None);
    assert_eq!(info.name, "tract");
    assert!(!info.is_fallback);
    assert_eq!(supervisor.active_backend_name(), "tract");
    assert_eq!(supervisor.requested_backend_name(), "tract");
    assert_eq!(supervisor.dev_base_model(), DEV_BASE_MODEL_PDFNET3);
    assert_eq!(supervisor.dev_base_model_error(), None);
    assert!(supervisor.supports_voice_profile());

    let film = FiLMVectors::new(
        vec![1.1; FILM_HIDDEN_DIM],
        vec![0.02; FILM_HIDDEN_DIM],
        vec![0.9; FILM_HIDDEN_DIM],
        vec![-0.02; FILM_HIDDEN_DIM],
    )
    .unwrap();
    let profile = VoiceProfile::new("spk", "Speaker", "2026-10-05T12:00:00Z", film, None).unwrap();
    supervisor.set_voice_profile(Some(&profile)).unwrap();
    assert_eq!(supervisor.active_voice_profile_id(), Some("spk"));
    // The profile survives a reselection on tract (same model, fresh backend).
    let _ = supervisor.select_backend("tract", None, None);
    assert_eq!(supervisor.active_voice_profile_id(), Some("spk"));
}

#[test]
#[ignore = "reads the real M3 pDFNet3 archive from the training repository"]
fn explicit_accelerator_request_honored_when_dev_base_model_present() {
    if !Path::new(M3_PDFNET3).is_file() {
        eprintln!("real M3 pDFNet3 archive not present; skipping");
        return;
    }
    let archive = PdfNet3DevArchive::read(Path::new(M3_PDFNET3), M3_PDFNET3_SHA256).unwrap();
    let mut supervisor = EngineSupervisor::default();
    supervisor.set_dev_base_model(Some(archive));

    let info = supervisor.select_backend("tensorrt", None, None);
    if realtime_noise_accelerators::TensorRtBackend::is_available() {
        assert_eq!(info.name, "nvidia-tensorrt");
        assert!(info.is_hardware_accelerated);
        assert!(!info.is_fallback);
        assert_eq!(supervisor.active_backend_name(), "nvidia-tensorrt");
        assert!(supervisor.is_hardware_accelerated());
        assert_eq!(supervisor.requested_backend_name(), "tensorrt");
    }

    let info_npu = supervisor.select_backend("openvino-npu", None, None);
    if realtime_noise_accelerators::OpenVINOBackend::is_available() {
        assert_eq!(info_npu.name, "openvino-npu");
        assert!(info_npu.is_hardware_accelerated);
        assert!(!info_npu.is_fallback);
        assert_eq!(supervisor.active_backend_name(), "openvino-npu");
        assert!(supervisor.is_hardware_accelerated());
        assert_eq!(supervisor.requested_backend_name(), "openvino-npu");
    }
}

