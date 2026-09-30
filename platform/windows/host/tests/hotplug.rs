#![forbid(unsafe_code)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use realtime_noise_contracts::{AudioBackend, DeviceStatus, EndpointError};
use realtime_noise_windows_host::wasapi::{WasapiAudioBackend, WasapiDeviceInfo};

#[test]
fn selected_device_loss_enters_waiting_without_selecting_another_mic() {
    let mut backend = WasapiAudioBackend::new();

    // Register two distinct microphones
    let mic_primary = WasapiDeviceInfo {
        id: "mic-primary-usb".to_string(),
        name: "Studio USB Microphone".to_string(),
        is_default: true,
        is_connected: true,
    };
    let mic_fallback = WasapiDeviceInfo {
        id: "mic-secondary-built-in".to_string(),
        name: "Internal Array Microphone".to_string(),
        is_default: false,
        is_connected: true,
    };

    backend.register_device(mic_primary);
    backend.register_device(mic_fallback);

    // Initial state before capture
    assert_eq!(backend.device_status(), DeviceStatus::Ready);
    assert_eq!(backend.active_device(), None);

    // Start capture on the primary microphone
    backend
        .start_capture("mic-primary-usb")
        .expect("start capture on valid primary mic must succeed");

    assert_eq!(backend.device_status(), DeviceStatus::Active);
    assert_eq!(backend.active_device(), Some("mic-primary-usb"));

    // Simulate physical device loss / AUDCLNT_E_DEVICE_INVALIDATED on primary mic
    backend.handle_device_invalidation("mic-primary-usb");

    // CRITICAL HOT-PLUG RESILIENCE POLICY:
    // When the user-selected microphone is disconnected or invalidated, the backend
    // MUST transition directly to WaitingForDevice. It MUST NOT arbitrarily switch
    // to the fallback microphone (mic-secondary-built-in).
    assert_eq!(
        backend.device_status(),
        DeviceStatus::WaitingForDevice,
        "Status must transition directly to WaitingForDevice upon device invalidation"
    );
    assert_eq!(
        backend.active_device(),
        Some("mic-primary-usb"),
        "Backend must retain user-selected device ID in WaitingForDevice and not switch to fallback"
    );

    // If another device appears or is connected while in WaitingForDevice,
    // the backend must remain waiting for the selected mic.
    let mic_unrelated = WasapiDeviceInfo {
        id: "mic-tertiary-bluetooth".to_string(),
        name: "Bluetooth Headset".to_string(),
        is_default: false,
        is_connected: true,
    };
    backend.register_device(mic_unrelated);

    assert_eq!(
        backend.device_status(),
        DeviceStatus::WaitingForDevice,
        "Connecting an unrelated device must not dismiss WaitingForDevice status"
    );
    assert_eq!(
        backend.active_device(),
        Some("mic-primary-usb"),
        "Active device target must still be the user-selected primary mic"
    );

    // When the primary microphone is reconnected / re-validated,
    // backend resumes Active status on the selected device.
    backend.handle_device_reconnected("mic-primary-usb");

    assert_eq!(
        backend.device_status(),
        DeviceStatus::Active,
        "Status must automatically return to Active when selected device is restored"
    );
    assert_eq!(backend.active_device(), Some("mic-primary-usb"));

    // Explicit switch to another mic works when user requests it
    backend
        .start_capture("mic-secondary-built-in")
        .expect("explicit switch to secondary mic must succeed");
    assert_eq!(backend.device_status(), DeviceStatus::Active);
    assert_eq!(backend.active_device(), Some("mic-secondary-built-in"));

    // Stop capture returns to Ready
    backend.stop_capture().expect("stop capture must succeed");
    assert_eq!(backend.device_status(), DeviceStatus::Ready);
}

#[test]
fn starting_capture_on_nonexistent_device_returns_not_found() {
    let mut backend = WasapiAudioBackend::new();
    let result = backend.start_capture("non-existent-device-id");
    assert_eq!(
        result,
        Err(EndpointError::DeviceNotFound(
            "non-existent-device-id".to_string()
        ))
    );
    assert_eq!(backend.device_status(), DeviceStatus::Ready);
}
