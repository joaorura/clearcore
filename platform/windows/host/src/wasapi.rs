#![forbid(unsafe_code)]

use realtime_noise_contracts::{AudioBackend, DeviceStatus, EndpointError};
use serde::{Deserialize, Serialize};

use std::time::{Duration, Instant};

/// Hysteresis hold time before suspending Bluetooth capture to return to high-fidelity A2DP (3.5 seconds).
pub const BLUETOOTH_RELEASE_HYSTERESIS_DURATION: Duration = Duration::from_millis(3500);

/// Detailed metadata describing an enumerated WASAPI audio capture device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasapiDeviceInfo {
    /// Unique endpoint ID (e.g. WASAPI device endpoint ID GUID string).
    pub id: String,
    /// Friendly human-readable name of the audio device.
    pub name: String,
    /// Whether this endpoint is currently marked as Windows default recording device.
    pub is_default: bool,
    /// Whether the endpoint is physically plugged in and active in Windows `PnP`.
    pub is_connected: bool,
}

impl WasapiDeviceInfo {
    /// Detects whether this audio endpoint corresponds to a Bluetooth device.
    ///
    /// Identifies Bluetooth devices based on known endpoint GUID prefixes (e.g. `BTHENUM`),
    /// Bluetooth audio device interfaces (`{0000111e-...}` Hands-Free, `{00001108-...}` Headset),
    /// or device friendly names containing Bluetooth indicators.
    pub fn is_bluetooth_device(&self) -> bool {
        let id_lower = self.id.to_lowercase();
        let name_lower = self.name.to_lowercase();

        id_lower.contains("bthenum")
            || id_lower.contains("bthhfp")
            || id_lower.contains("bluetooth")
            || id_lower.contains("{0000111e-0000-1000-8000-00805f9b34fb}")
            || id_lower.contains("{00001108-0000-1000-8000-00805f9b34fb}")
            || name_lower.contains("bluetooth")
            || name_lower.contains("hands-free")
            || name_lower.contains("handsfree")
            || name_lower.contains("airpods")
            || name_lower.contains("galaxy buds")
            || name_lower.contains("wh-1000xm")
            || name_lower.contains("wf-1000xm")
    }
}

/// Production WASAPI audio backend manager.
///
/// Implements [`AudioBackend`] providing device discovery, event-driven capture lifecycle,
/// strict hot-plug resilience (`WaitingForDevice` state preservation without fallback switching),
/// and Bluetooth capture lifecycle management with 3.5s hysteresis to return devices to A2DP.
#[derive(Debug, Clone)]
pub struct WasapiAudioBackend {
    devices: Vec<WasapiDeviceInfo>,
    current_device: Option<String>,
    status: DeviceStatus,
    /// Indicates whether the physical IAudioClient capture stream is currently active (IAudioClient::Start).
    is_client_started: bool,
    /// Timestamp when virtual mic stream count dropped to 0, arming the 3.5s release timer.
    idle_since: Option<Instant>,
}

impl Default for WasapiAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WasapiAudioBackend {
    /// Creates a new WASAPI audio backend in [`DeviceStatus::Ready`] state.
    pub fn new() -> Self {
        Self {
            devices: Vec::new(),
            current_device: None,
            status: DeviceStatus::Ready,
            is_client_started: false,
            idle_since: None,
        }
    }

    /// Creates a WASAPI audio backend pre-populated with known devices.
    pub fn with_devices(devices: Vec<WasapiDeviceInfo>) -> Self {
        Self {
            devices,
            current_device: None,
            status: DeviceStatus::Ready,
            is_client_started: false,
            idle_since: None,
        }
    }

    /// Registers or updates a device in the backend's known endpoint collection.
    pub fn register_device(&mut self, device: WasapiDeviceInfo) {
        if let Some(pos) = self.devices.iter().position(|d| d.id == device.id) {
            self.devices[pos] = device;
        } else {
            self.devices.push(device);
        }
    }

    /// Unregisters an endpoint from the known device collection.
    pub fn unregister_device(&mut self, device_id: &str) {
        self.devices.retain(|d| d.id != device_id);
    }

    /// Enumerates currently known WASAPI capture devices.
    pub fn enumerate_devices(&self) -> Vec<WasapiDeviceInfo> {
        self.devices.clone()
    }

    /// Returns the device ID currently targeted or active for audio capture.
    pub fn active_device(&self) -> Option<&str> {
        self.current_device.as_deref()
    }

    /// Returns whether the active capture device is detected as Bluetooth.
    pub fn is_current_device_bluetooth(&self) -> bool {
        self.current_device
            .as_deref()
            .and_then(|id| self.devices.iter().find(|d| d.id == id))
            .is_some_and(WasapiDeviceInfo::is_bluetooth_device)
    }

    /// Returns whether the underlying physical IAudioClient capture stream is currently running (`IAudioClient::Start`).
    pub const fn is_client_started(&self) -> bool {
        self.is_client_started
    }

    /// Returns whether the 3.5s idle hysteresis timer is currently armed.
    pub const fn is_release_timer_armed(&self) -> bool {
        self.idle_since.is_some()
    }

    /// Synchronizes physical capture lifecycle with the virtual microphone's active streams count.
    ///
    /// # Bluetooth Lifecycle & 3.5s Hysteresis
    ///
    /// - When `active_streams_count > 0`: Immediate re-activation (`IAudioClient::Start()`).
    ///   Any pending idle timer is immediately cancelled.
    /// - When `active_streams_count == 0`:
    ///   - If the active device is Bluetooth: arms a 3.5s timer (`BLUETOOTH_RELEASE_HYSTERESIS_DURATION`).
    ///     When the timer expires without new demand, `IAudioClient::Stop()` is executed to release the physical
    ///     headset microphone and allow Windows to restore high-fidelity A2DP profile.
    ///   - If non-Bluetooth: physical capture remains active or behaves according to standard power policy.
    pub fn sync_active_streams(&mut self, active_streams_count: u32, now: Instant) {
        if self.status != DeviceStatus::Active {
            return;
        }

        let is_bt = self.is_current_device_bluetooth();

        if active_streams_count > 0 {
            // Demand is active: cancel idle timer and ensure capture is running immediately
            self.idle_since = None;
            if !self.is_client_started {
                self.is_client_started = true;
            }
        } else if is_bt {
            // Demand dropped to 0 on Bluetooth capture device: handle hysteresis
            if self.is_client_started {
                match self.idle_since {
                    None => {
                        // Arm the 3.5s hysteresis timer
                        self.idle_since = Some(now);
                    }
                    Some(idle_start) => {
                        // Check if 3.5 seconds have elapsed
                        if now.saturating_duration_since(idle_start) >= BLUETOOTH_RELEASE_HYSTERESIS_DURATION {
                            // Hysteresis expired: pause capture stream (IAudioClient::Stop())
                            self.is_client_started = false;
                            self.idle_since = None;
                        }
                    }
                }
            }
        }
    }

    /// Handles a WASAPI device invalidation or disconnect event (`AUDCLNT_E_DEVICE_INVALIDATED`).
    ///
    /// # Hot-Plug Resilience Policy
    ///
    /// When the active microphone is disconnected or invalidated, the backend transitions
    /// directly to [`DeviceStatus::WaitingForDevice`]. It intentionally retains the target
    /// device ID and does **not** fall back to any other microphone or default device.
    pub fn handle_device_invalidation(&mut self, device_id: &str) {
        for dev in &mut self.devices {
            if dev.id == device_id {
                dev.is_connected = false;
            }
        }

        if self.current_device.as_deref() == Some(device_id) {
            self.status = DeviceStatus::WaitingForDevice;
            self.is_client_started = false;
            self.idle_since = None;
        }
    }

    /// Handles a WASAPI device reconnection or state change event (`DEVICE_STATE_ACTIVE`).
    ///
    /// If the reconnected device matches the user-selected device currently in
    /// [`DeviceStatus::WaitingForDevice`], capture is automatically restored to [`DeviceStatus::Active`].
    pub fn handle_device_reconnected(&mut self, device_id: &str) {
        for dev in &mut self.devices {
            if dev.id == device_id {
                dev.is_connected = true;
            }
        }

        if self.current_device.as_deref() == Some(device_id)
            && self.status == DeviceStatus::WaitingForDevice
        {
            self.status = DeviceStatus::Active;
            self.is_client_started = true;
            self.idle_since = None;
        }
    }
}

impl AudioBackend for WasapiAudioBackend {
    fn start_capture(&mut self, device_id: &str) -> Result<(), EndpointError> {
        let device = self
            .devices
            .iter()
            .find(|d| d.id == device_id)
            .ok_or_else(|| EndpointError::DeviceNotFound(device_id.to_string()))?;

        self.current_device = Some(device_id.to_string());
        self.idle_since = None;

        if device.is_connected {
            self.status = DeviceStatus::Active;
            self.is_client_started = true;
        } else {
            self.status = DeviceStatus::WaitingForDevice;
            self.is_client_started = false;
        }

        Ok(())
    }

    fn stop_capture(&mut self) -> Result<(), EndpointError> {
        self.status = DeviceStatus::Ready;
        self.current_device = None;
        self.is_client_started = false;
        self.idle_since = None;
        Ok(())
    }

    fn device_status(&self) -> DeviceStatus {
        self.status
    }
}
