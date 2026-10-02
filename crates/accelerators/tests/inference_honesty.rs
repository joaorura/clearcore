#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::panic)]

//! Backends that only copy input to output must say so, and nothing may select them as if they
//! ran a neural network.

use realtime_noise_accelerators::{
    BackendSelection, CoreMlBackend, DetectedHardware, DirectMlBackend, HardwareAudit,
    HardwareScanner, OpenVINOBackend, RuntimeRecommendationTracker, RuntimeStatus, RyzenAiBackend,
    TensorRtBackend, VulkanBackend,
};
use realtime_noise_model::InferenceBackend;

fn installed(hardware: DetectedHardware, runtime: &str) -> HardwareAudit {
    HardwareAudit {
        hardware,
        device_name: "test device".to_owned(),
        pci_id: None,
        status: RuntimeStatus::Installed {
            runtime_name: runtime.to_owned(),
            path: Some("/usr/lib64/lib-test.so".to_owned()),
        },
    }
}

fn resolve(audits: &[HardwareAudit]) -> BackendSelection {
    let mut tracker = RuntimeRecommendationTracker::new();
    HardwareScanner::resolve_audits_to_backend(&mut tracker, audits).0
}

#[test]
fn passthrough_backends_do_not_claim_to_execute_inference() {
    assert!(!DirectMlBackend::new_mock_dgpu().executes_inference());
    assert!(!VulkanBackend::new_mock_dgpu().executes_inference());
    assert!(!RyzenAiBackend::new_mock_npu().executes_inference());
    assert!(!RyzenAiBackend::new_mock_gpu().executes_inference());
    assert!(!TensorRtBackend::new_mock().executes_inference());
    assert!(!CoreMlBackend::new_mock().executes_inference());
    // An OpenVINO backend without a compiled network is a passthrough stub.
    assert!(!OpenVINOBackend::new_mock_npu().executes_inference());
    assert!(!OpenVINOBackend::new_mock_cpu().executes_inference());
}

#[test]
fn backends_built_on_the_real_constructors_are_equally_honest() {
    // `new` degrades to a mock context when the host lacks the runtime, and even where the
    // runtime exists the compute path is a host copy: none of these may claim inference.
    assert!(
        !DirectMlBackend::new(
            "a",
            "b",
            realtime_noise_runtime_gpu_compute::DirectMlDeviceType::DedicatedGpu
        )
        .executes_inference()
    );
    assert!(
        !VulkanBackend::new(
            "a",
            "b",
            realtime_noise_runtime_gpu_compute::VulkanDeviceType::DedicatedGpu
        )
        .executes_inference()
    );
    assert!(!RyzenAiBackend::new("a", "b", "NPU").executes_inference());
    assert!(!TensorRtBackend::new("a", "b", 0).executes_inference());
    assert!(!CoreMlBackend::new("a", "b", "all").executes_inference());
}

#[test]
fn selection_reports_which_runtimes_run_inference_in_this_build() {
    let running = [
        BackendSelection::TractCpu,
        BackendSelection::OpenVinoNpu,
        BackendSelection::OpenVinoGpu,
        BackendSelection::OpenVinoCpu,
        BackendSelection::OpenVino,
    ];
    let passthrough = [
        BackendSelection::TensorRt,
        BackendSelection::DirectMl,
        BackendSelection::Vulkan,
        BackendSelection::RyzenAiNpu,
        BackendSelection::RyzenAiGpu,
        BackendSelection::RyzenAi,
        BackendSelection::CoreMl,
    ];
    for selection in running {
        assert!(selection.executes_inference(), "{}", selection.name());
    }
    for selection in passthrough {
        assert!(!selection.executes_inference(), "{}", selection.name());
    }
}

#[test]
fn hardware_instantiation_refuses_every_passthrough_runtime() {
    let dir = std::path::Path::new("/nonexistent/stateful");
    for selection in [
        BackendSelection::TensorRt,
        BackendSelection::DirectMl,
        BackendSelection::Vulkan,
        BackendSelection::RyzenAiNpu,
        BackendSelection::RyzenAiGpu,
        BackendSelection::RyzenAi,
        BackendSelection::CoreMl,
    ] {
        let error = selection
            .instantiate_hardware_backend(dir, "a", "b")
            .err()
            .unwrap_or_else(|| panic!("{} must not instantiate", selection.name()));
        let message = error.to_string();
        assert!(
            message.contains("does not execute inference"),
            "{}: {message}",
            selection.name()
        );
    }
}

#[test]
fn resolver_never_picks_a_backend_that_only_copies_samples() {
    // NVIDIA with TensorRT installed: no engine execution yet, so CPU.
    assert_eq!(
        resolve(&[installed(DetectedHardware::NvidiaGpu, "TensorRT")]),
        BackendSelection::TractCpu
    );
    // AMD NPU / AMD GPU / Apple Silicon: passthrough contexts, so CPU.
    assert_eq!(
        resolve(&[installed(DetectedHardware::AmdNpu, "Ryzen AI (XDNA)")]),
        BackendSelection::TractCpu
    );
    assert_eq!(
        resolve(&[installed(DetectedHardware::AmdGpu, "Vulkan loader")]),
        BackendSelection::TractCpu
    );
    assert_eq!(
        resolve(&[installed(DetectedHardware::AppleSilicon, "CoreML")]),
        BackendSelection::TractCpu
    );
}

#[test]
fn resolver_skips_passthrough_hardware_and_keeps_real_runtimes() {
    // A passthrough tier above a real one must not shadow it.
    let audits = [
        installed(DetectedHardware::NvidiaGpu, "TensorRT"),
        installed(DetectedHardware::AmdNpu, "Ryzen AI (XDNA)"),
        installed(DetectedHardware::IntelNpu, "OpenVINO (NPU)"),
    ];
    assert_eq!(resolve(&audits), BackendSelection::OpenVinoNpu);

    let audits = [
        installed(DetectedHardware::AmdGpu, "Vulkan loader"),
        installed(DetectedHardware::IntelGpu, "OpenVINO (GPU)"),
    ];
    assert_eq!(resolve(&audits), BackendSelection::OpenVinoGpu);
}

#[test]
fn tract_cpu_mock_is_labelled_as_tract_not_openvino() {
    let backend = BackendSelection::TractCpu.instantiate_mock_backend();
    let descriptor = backend.descriptor();
    assert_eq!(descriptor.backend, "tract");
    assert_eq!(descriptor.runtime, "tract");

    let openvino = BackendSelection::OpenVinoCpu.instantiate_mock_backend();
    assert_eq!(openvino.descriptor().backend, "openvino");
}
