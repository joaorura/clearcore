#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use realtime_noise_contracts::{
    AudioBackend, DeviceStatus, EndpointError, EndpointStatus, VirtualMicrophone,
};
use realtime_noise_linux_host::endpoint::LinuxVirtualMicrophone;
use realtime_noise_linux_host::pipewire::{PipeWireAudioBackend, PipeWireDeviceInfo};

#[test]
fn device_contention_returns_device_busy_or_unavailable_busy() {
    // 1. PipeWireAudioBackend contention verification
    let mut backend = PipeWireAudioBackend::new();
    let mic_info = PipeWireDeviceInfo {
        id: "alsa_input.usb-mic".to_string(),
        name: "USB Mic".to_string(),
        description: "USB Microphone".to_string(),
        is_default: true,
        is_busy: true, // Marked busy by exclusive session
    };
    backend.register_device(mic_info.clone());

    let capture_res = backend.start_capture(&mic_info.id);
    assert_eq!(
        capture_res,
        Err(EndpointError::DeviceBusy),
        "start_capture must return DeviceBusy on contested device"
    );
    assert_eq!(
        backend.device_status(),
        DeviceStatus::UnavailableBusy,
        "backend device_status must report UnavailableBusy when contested"
    );

    // 2. LinuxVirtualMicrophone contention verification
    let mut vmic = LinuxVirtualMicrophone::new();
    vmic.set_contention(true);

    let start_res = vmic.start();
    assert_eq!(
        start_res,
        Err(EndpointError::DeviceBusy),
        "virtual mic start must return DeviceBusy on contention"
    );
    assert_eq!(
        vmic.status(),
        EndpointStatus::UnavailableBusy,
        "virtual mic status must report UnavailableBusy on contention"
    );

    // Clear contention: both components should start cleanly
    vmic.set_contention(false);
    assert!(
        vmic.start().is_ok(),
        "virtual mic start should succeed after contention clears"
    );
    assert_eq!(vmic.status(), EndpointStatus::Active);

    backend.set_device_busy(&mic_info.id, false);
    assert!(
        backend.start_capture(&mic_info.id).is_ok(),
        "backend start_capture should succeed after contention clears"
    );
    assert_eq!(backend.device_status(), DeviceStatus::Active);
}
