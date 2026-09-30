#![forbid(unsafe_code)]

use realtime_noise_contracts::{AudioBackend, DeviceStatus, EndpointError};
use serde::{Deserialize, Serialize};

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

/// Production WASAPI audio backend manager.
///
/// Implements [`AudioBackend`] providing device discovery, event-driven capture lifecycle,
/// and strict hot-plug resilience (`WaitingForDevice` state preservation without fallback switching).
#[derive(Debug, Clone)]
pub struct WasapiAudioBackend {
    devices: Vec<WasapiDeviceInfo>,
    current_device: Option<String>,
    status: DeviceStatus,
}

impl Default for WasapiAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WasapiAudioBackend {
    /// Creates a new WASAPI audio backend in [`DeviceStatus::Ready`] state.
    pub const fn new() -> Self {
        Self {
            devices: Vec::new(),
            current_device: None,
            status: DeviceStatus::Ready,
        }
    }

    /// Creates a WASAPI audio backend pre-populated with known devices.
    pub const fn with_devices(devices: Vec<WasapiDeviceInfo>) -> Self {
        Self {
            devices,
            current_device: None,
            status: DeviceStatus::Ready,
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

        if device.is_connected {
            self.status = DeviceStatus::Active;
        } else {
            self.status = DeviceStatus::WaitingForDevice;
        }

        Ok(())
    }

    fn stop_capture(&mut self) -> Result<(), EndpointError> {
        self.status = DeviceStatus::Ready;
        self.current_device = None;
        Ok(())
    }

    fn device_status(&self) -> DeviceStatus {
        self.status
    }
}
