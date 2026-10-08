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

#[test]
fn bluetooth_detection_and_3_5s_hysteresis_release() {
    use std::time::{Duration, Instant};

    let mut backend = WasapiAudioBackend::new();

    let bt_mic = WasapiDeviceInfo {
        id: r"{0.0.1.00000000}.{BTHENUM\DEV_112233445566}".to_string(),
        name: "Sony WH-1000XM4 Hands-Free AG Audio".to_string(),
        is_default: false,
        is_connected: true,
    };
    assert!(bt_mic.is_bluetooth_device(), "Must detect Bluetooth Hands-Free endpoint");

    let usb_mic = WasapiDeviceInfo {
        id: r"{0.0.1.00000000}.{USB\VID_1234&PID_5678}".to_string(),
        name: "Studio USB Microphone".to_string(),
        is_default: true,
        is_connected: true,
    };
    assert!(!usb_mic.is_bluetooth_device(), "USB mic is not Bluetooth");

    backend.register_device(bt_mic);
    backend.register_device(usb_mic);

    // 1. Start capture on Bluetooth device
    backend
        .start_capture(r"{0.0.1.00000000}.{BTHENUM\DEV_112233445566}")
        .expect("start capture on BT mic must succeed");

    assert_eq!(backend.device_status(), DeviceStatus::Active);
    assert!(backend.is_current_device_bluetooth());
    assert!(backend.is_client_started());
    assert!(!backend.is_release_timer_armed());

    let t0 = Instant::now();

    // 2. Virtual mic demand is active (1 active stream)
    backend.sync_active_streams(1, t0);
    assert!(backend.is_client_started());
    assert!(!backend.is_release_timer_armed());

    // 3. Demand drops to 0 active streams -> 3.5s hysteresis timer armed
    backend.sync_active_streams(0, t0);
    assert!(
        backend.is_client_started(),
        "Capture must remain active immediately when streams drop to 0 (hysteresis hold)"
    );
    assert!(
        backend.is_release_timer_armed(),
        "3.5s release timer must be armed"
    );

    // 4. Time advances by 2.0s (less than 3.5s threshold) -> still holding
    let t_2s = t0 + Duration::from_millis(2000);
    backend.sync_active_streams(0, t_2s);
    assert!(
        backend.is_client_started(),
        "Capture must still be running at 2.0s into the 3.5s hysteresis window"
    );
    assert!(backend.is_release_timer_armed());

    // 5. New stream begins at 2.5s -> cancels timer immediately
    let t_2_5s = t0 + Duration::from_millis(2500);
    backend.sync_active_streams(1, t_2_5s);
    assert!(backend.is_client_started());
    assert!(
        !backend.is_release_timer_armed(),
        "Incoming stream demand must disarm release timer immediately"
    );

    // 6. Demand drops to 0 again at 3.0s -> re-arms timer
    let t_3s = t0 + Duration::from_millis(3000);
    backend.sync_active_streams(0, t_3s);
    assert!(backend.is_release_timer_armed());
    assert!(backend.is_client_started());

    // 7. Time advances past 3.5s hysteresis (3.0s + 3.5s = 6.5s) -> releases capture
    let t_6_5s = t_3s + Duration::from_millis(3500);
    backend.sync_active_streams(0, t_6_5s);
    assert!(
        !backend.is_client_started(),
        "Capture must be stopped after 3.5s idle to release Bluetooth headset profile back to A2DP"
    );
    assert!(
        !backend.is_release_timer_armed(),
        "Timer disarms once expired and acted upon"
    );

    // 8. New application opens mic -> immediately re-activates capture
    let t_7s = t_3s + Duration::from_millis(4000);
    backend.sync_active_streams(1, t_7s);
    assert!(
        backend.is_client_started(),
        "Capture must immediately re-activate when new virtual stream arrives"
    );
}

