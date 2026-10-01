#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used)]

use realtime_noise_accelerators::{
    BackendRequest, BackendSelection, DetectedHardware, DeviceTier, HardwareAudit, HardwareScanner,
    MAX_RUNTIME_INSTALL_PROMPTS, RuntimeRecommendationTracker, RuntimeStatus,
};

#[test]
fn max_runtime_install_prompts_is_three() {
    assert_eq!(MAX_RUNTIME_INSTALL_PROMPTS, 3);
}

#[test]
fn prompt_tracker_warns_three_times_then_suppresses() {
    let mut tracker = RuntimeRecommendationTracker::new();

    // 1st prompt: should warn with counter [1/3]
    let msg1 = tracker.record_prompt(DetectedHardware::NvidiaGpu);
    assert!(msg1.is_some());
    let m1 = msg1.unwrap();
    assert!(m1.contains("[1/3]"));
    assert!(m1.contains("TensorRT"));
    assert!(m1.contains("scripts/install-tensorrt.sh"));

    // 2nd prompt: should warn with counter [2/3]
    let msg2 = tracker.record_prompt(DetectedHardware::NvidiaGpu);
    assert!(msg2.is_some());
    let m2 = msg2.unwrap();
    assert!(m2.contains("[2/3]"));
    assert!(m2.contains("TensorRT"));

    // 3rd prompt: should warn with counter [3/3]
    let msg3 = tracker.record_prompt(DetectedHardware::NvidiaGpu);
    assert!(msg3.is_some());
    let m3 = msg3.unwrap();
    assert!(m3.contains("[3/3]"));

    // 4th prompt: suppressed! Returns None to allow silent continuation
    let msg4 = tracker.record_prompt(DetectedHardware::NvidiaGpu);
    assert!(msg4.is_none());

    // 5th prompt: still suppressed!
    let msg5 = tracker.record_prompt(DetectedHardware::NvidiaGpu);
    assert!(msg5.is_none());

    assert_eq!(tracker.prompt_count(DetectedHardware::NvidiaGpu), 5);
    assert!(!tracker.should_prompt(DetectedHardware::NvidiaGpu));
}

#[test]
fn independent_devices_track_separate_prompt_counts() {
    let mut tracker = RuntimeRecommendationTracker::new();

    for _ in 0..3 {
        assert!(tracker.record_prompt(DetectedHardware::NvidiaGpu).is_some());
    }
    // NVIDIA is now exhausted
    assert!(tracker.record_prompt(DetectedHardware::NvidiaGpu).is_none());

    // Intel NPU should still prompt
    assert!(tracker.should_prompt(DetectedHardware::IntelNpu));
    let npu_msg = tracker.record_prompt(DetectedHardware::IntelNpu);
    assert!(npu_msg.is_some());
    let nm = npu_msg.unwrap();
    assert!(nm.contains("[1/3]"));
    assert!(nm.contains("OpenVINO"));
    assert!(nm.contains("scripts/install-openvino.sh"));
}

#[test]
fn hardware_detected_without_runtime_prompts_and_falls_back_to_npu() {
    let mut tracker = RuntimeRecommendationTracker::new();

    let audits = vec![
        // Physical NVIDIA GPU present, but TensorRT runtime missing
        HardwareAudit {
            hardware: DetectedHardware::NvidiaGpu,
            device_name: "NVIDIA RTX PRO 1000 Blackwell".to_owned(),
            pci_id: Some("10de:2db8".to_owned()),
            status: RuntimeStatus::Missing {
                recommended_runtime: "TensorRT".to_owned(),
                install_script: "scripts/install-tensorrt.sh".to_owned(),
                install_instruction: "Execute scripts/install-tensorrt.sh".to_owned(),
            },
        },
        // Physical Intel NPU present, and OpenVINO runtime IS installed
        HardwareAudit {
            hardware: DetectedHardware::IntelNpu,
            device_name: "Intel NPU Core Ultra 200H".to_owned(),
            pci_id: Some("8086:7d1d".to_owned()),
            status: RuntimeStatus::Installed {
                runtime_name: "OpenVINO".to_owned(),
                path: Some("/lib64/libopenvino.so.2510".to_owned()),
            },
        },
    ];

    // 1st run: prompts for TensorRT, falls back to Intel NPU
    let (sel1, msgs1) = HardwareScanner::resolve_audits_to_backend(&mut tracker, &audits);
    assert_eq!(sel1, BackendSelection::OpenVinoNpu);
    assert_eq!(msgs1.len(), 1);
    assert!(msgs1[0].contains("[1/3]"));
    assert!(msgs1[0].contains("scripts/install-tensorrt.sh"));

    // 2nd run: prompts for TensorRT, falls back to Intel NPU
    let (sel2, msgs2) = HardwareScanner::resolve_audits_to_backend(&mut tracker, &audits);
    assert_eq!(sel2, BackendSelection::OpenVinoNpu);
    assert_eq!(msgs2.len(), 1);
    assert!(msgs2[0].contains("[2/3]"));

    // 3rd run: prompts for TensorRT, falls back to Intel NPU
    let (sel3, msgs3) = HardwareScanner::resolve_audits_to_backend(&mut tracker, &audits);
    assert_eq!(sel3, BackendSelection::OpenVinoNpu);
    assert_eq!(msgs3.len(), 1);
    assert!(msgs3[0].contains("[3/3]"));

    // 4th run: prompt is suppressed! Continues silently with Intel NPU
    let (sel4, msgs4) = HardwareScanner::resolve_audits_to_backend(&mut tracker, &audits);
    assert_eq!(sel4, BackendSelection::OpenVinoNpu);
    assert!(msgs4.is_empty(), "prompts must be suppressed after 3 times");
}

#[test]
fn hardware_all_runtimes_missing_falls_back_to_tract_cpu() {
    let mut tracker = RuntimeRecommendationTracker::new();

    let audits = vec![
        HardwareAudit {
            hardware: DetectedHardware::NvidiaGpu,
            device_name: "NVIDIA GPU".to_owned(),
            pci_id: Some("10de".to_owned()),
            status: RuntimeStatus::Missing {
                recommended_runtime: "TensorRT".to_owned(),
                install_script: "scripts/install-tensorrt.sh".to_owned(),
                install_instruction: "Execute scripts/install-tensorrt.sh".to_owned(),
            },
        },
        HardwareAudit {
            hardware: DetectedHardware::AmdNpu,
            device_name: "AMD Ryzen AI NPU".to_owned(),
            pci_id: Some("1022:1502".to_owned()),
            status: RuntimeStatus::Missing {
                recommended_runtime: "Ryzen AI Software".to_owned(),
                install_script: "scripts/install-ryzenai.sh".to_owned(),
                install_instruction: "Execute scripts/install-ryzenai.sh".to_owned(),
            },
        },
    ];

    let (sel, msgs) = HardwareScanner::resolve_audits_to_backend(&mut tracker, &audits);
    // Falls back to Tract CPU baseline (always safe)
    assert_eq!(sel, BackendSelection::TractCpu);
    assert_eq!(sel.name(), "tract");
    assert_eq!(sel.tier(), DeviceTier::Cpu);
    // Both missing devices are prompted
    assert_eq!(msgs.len(), 2);
}

#[test]
fn sem_cuda_so_tensorrt_contract() {
    // 1. In BackendSelection, NVIDIA maps strictly to TensorRt
    assert_eq!(
        realtime_noise_accelerators::backend_selection_from_name("cuda"),
        BackendSelection::TensorRt
    );
    assert_eq!(
        realtime_noise_accelerators::backend_selection_from_name("tensorrt"),
        BackendSelection::TensorRt
    );
    assert_eq!(
        realtime_noise_accelerators::backend_selection_from_name("nvidia"),
        BackendSelection::TensorRt
    );

    // 2. Selection name is always "tensorrt"
    let trt = BackendSelection::TensorRt;
    assert_eq!(trt.name(), "tensorrt");
    assert!(trt.is_nvidia());
    assert!(trt.is_dedicated_gpu());
    assert_eq!(trt.tier(), DeviceTier::DedicatedGpu);

    // 3. User requesting legacy BackendRequest::Cuda resolves to TensorRt
    let policy = realtime_noise_accelerators::AutoPolicy::new();
    let candidates = [realtime_noise_accelerators::CalibrationReport {
        backend_name: "cuda".to_owned(),
        duration_seconds: 300.0,
        total_frames: 30_000,
        p50_latency_ms: 0.5,
        p95_latency_ms: 0.8,
        p99_latency_ms: 1.1,
        max_latency_ms: 1.8,
        deadline_miss_count: 0,
        discontinuities: 0,
        decision: realtime_noise_accelerators::PromotionDecision::Promoted,
        reason: None,
    }];
    let sel = policy.resolve_candidates(BackendRequest::Cuda, &candidates);
    assert_eq!(sel, BackendSelection::TensorRt);
}
