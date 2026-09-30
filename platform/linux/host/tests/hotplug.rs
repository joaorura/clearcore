#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::redundant_clone)]

use realtime_noise_contracts::{AudioBackend, DeviceStatus};
use realtime_noise_linux_host::pipewire::{PipeWireAudioBackend, PipeWireDeviceInfo};

#[test]
fn selected_device_loss_enters_waiting_without_selecting_another_mic() {
    let mut backend = PipeWireAudioBackend::new();

    // Register two distinct microphones
    let headset_mic = PipeWireDeviceInfo {
        id: "alsa_input.usb-headset-mic".to_string(),
        name: "USB Headset Microphone".to_string(),
        description: "High-fidelity USB headset".to_string(),
        is_default: false,
        is_busy: false,
    };
    let builtin_mic = PipeWireDeviceInfo {
        id: "alsa_input.pci-builtin-mic".to_string(),
        name: "Built-in Microphone".to_string(),
        description: "Internal analog microphone".to_string(),
        is_default: true,
        is_busy: false,
    };

    backend.register_device(headset_mic.clone());
    backend.register_device(builtin_mic);

    // Select the headset mic and start capture
    backend
        .start_capture(&headset_mic.id)
        .expect("start_capture should succeed on valid device");
    assert_eq!(backend.device_status(), DeviceStatus::Active);
    assert_eq!(backend.current_device(), Some(headset_mic.id.as_str()));

    // Disconnect headset microphone (hotplug removal event)
    backend.handle_device_disconnected(&headset_mic.id);

    // Verified requirement: backend transitions directly to WaitingForDevice
    assert_eq!(
        backend.device_status(),
        DeviceStatus::WaitingForDevice,
        "backend must transition to WaitingForDevice when selected device is lost"
    );

    // Verified requirement: backend must NOT fall back to another microphone (e.g. builtin mic)
    assert_eq!(
        backend.current_device(),
        Some(headset_mic.id.as_str()),
        "backend must retain selected device identifier and not arbitrarily fall back to another mic"
    );

    // When the selected device is reconnected, backend recovers
    backend.handle_device_reconnected(&headset_mic.id);
    assert_eq!(
        backend.device_status(),
        DeviceStatus::Active,
        "backend should resume Active status upon selected device reconnection"
    );
    assert_eq!(backend.current_device(), Some(headset_mic.id.as_str()));
}
