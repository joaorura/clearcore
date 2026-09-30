#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use std::collections::HashMap;
use realtime_noise_contracts::{AudioBackend, DeviceStatus, EndpointError};
use serde::{Deserialize, Serialize};

/// Detailed device descriptor for `PipeWire` input/capture endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PipeWireDeviceInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_default: bool,
    pub is_busy: bool,
}

/// Linux `PipeWire` host audio capture backend.
///
/// Implements `AudioBackend` trait with device enumeration, capture lifecycle,
/// device contention handling, and hot-plug resilience (`DeviceStatus::WaitingForDevice`).
#[derive(Debug, Clone)]
pub struct PipeWireAudioBackend {
    current_device: Option<String>,
    status: DeviceStatus,
    known_devices: HashMap<String, PipeWireDeviceInfo>,
    is_contended: bool,
}

impl Default for PipeWireAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl PipeWireAudioBackend {
    #[must_use]
    pub fn new() -> Self {
        Self {
            current_device: None,
            status: DeviceStatus::Ready,
            known_devices: HashMap::new(),
            is_contended: false,
        }
    }

    #[must_use]
    pub fn with_devices(devices: impl IntoIterator<Item = PipeWireDeviceInfo>) -> Self {
        let mut map = HashMap::new();
        for dev in devices {
            map.insert(dev.id.clone(), dev);
        }
        Self {
            current_device: None,
            status: DeviceStatus::Ready,
            known_devices: map,
            is_contended: false,
        }
    }

    pub fn register_device(&mut self, device: PipeWireDeviceInfo) {
        self.known_devices.insert(device.id.clone(), device);
    }

    pub fn unregister_device(&mut self, device_id: &str) {
        self.known_devices.remove(device_id);
        if self.current_device.as_deref() == Some(device_id) && self.status == DeviceStatus::Active {
            self.status = DeviceStatus::WaitingForDevice;
        }
    }

    #[must_use]
    pub fn enumerate_devices(&self) -> Vec<PipeWireDeviceInfo> {
        let mut list: Vec<PipeWireDeviceInfo> = self.known_devices.values().cloned().collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    #[must_use]
    pub fn current_device(&self) -> Option<&str> {
        self.current_device.as_deref()
    }

    pub fn set_device_busy(&mut self, device_id: &str, busy: bool) {
        if let Some(dev) = self.known_devices.get_mut(device_id) {
            dev.is_busy = busy;
        }
        if self.current_device.as_deref() == Some(device_id) && busy && self.status == DeviceStatus::Active {
            self.status = DeviceStatus::UnavailableBusy;
        }
    }

    pub fn set_contention(&mut self, busy: bool) {
        self.is_contended = busy;
        if busy {
            self.status = DeviceStatus::UnavailableBusy;
        }
    }

    /// Handles loss/disconnection of an input device.
    ///
    /// If the disconnected device matches the currently active device, the backend
    /// transitions directly to `DeviceStatus::WaitingForDevice` without falling back
    /// to another microphone.
    pub fn handle_device_disconnected(&mut self, device_id: &str) {
        if self.current_device.as_deref() == Some(device_id) {
            self.status = DeviceStatus::WaitingForDevice;
        }
    }

    /// Handles reconnection of a previously disconnected device.
    pub fn handle_device_reconnected(&mut self, device_id: &str) {
        if self.current_device.as_deref() == Some(device_id) {
            self.status = DeviceStatus::Active;
        }
    }
}

impl AudioBackend for PipeWireAudioBackend {
    fn start_capture(&mut self, device_id: &str) -> Result<(), EndpointError> {
        if self.is_contended {
            self.status = DeviceStatus::UnavailableBusy;
            return Err(EndpointError::DeviceBusy);
        }

        let Some(device) = self.known_devices.get(device_id) else {
            return Err(EndpointError::DeviceNotFound(device_id.to_string()));
        };

        if device.is_busy {
            self.status = DeviceStatus::UnavailableBusy;
            return Err(EndpointError::DeviceBusy);
        }

        self.current_device = Some(device_id.to_string());
        self.status = DeviceStatus::Active;
        Ok(())
    }

    fn stop_capture(&mut self) -> Result<(), EndpointError> {
        self.status = DeviceStatus::Ready;
        Ok(())
    }

    fn device_status(&self) -> DeviceStatus {
        self.status
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_lifecycle() {
        let mut backend = PipeWireAudioBackend::new();
        assert_eq!(backend.device_status(), DeviceStatus::Ready);
        assert_eq!(backend.current_device(), None);

        let dev = PipeWireDeviceInfo {
            id: "pw-mic-1".to_string(),
            name: "Microphone 1".to_string(),
            description: "Default Mic".to_string(),
            is_default: true,
            is_busy: false,
        };
        backend.register_device(dev);

        assert_eq!(backend.enumerate_devices().len(), 1);
        assert!(backend.start_capture("pw-mic-1").is_ok());
        assert_eq!(backend.device_status(), DeviceStatus::Active);
        assert_eq!(backend.current_device(), Some("pw-mic-1"));

        assert!(backend.stop_capture().is_ok());
        assert_eq!(backend.device_status(), DeviceStatus::Ready);
    }

    #[test]
    fn test_device_not_found() {
        let mut backend = PipeWireAudioBackend::new();
        let res = backend.start_capture("non-existent");
        assert_eq!(res, Err(EndpointError::DeviceNotFound("non-existent".to_string())));
    }
}
