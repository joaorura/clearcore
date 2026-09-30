use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DeviceStatus {
    Ready,
    Active,
    WaitingForDevice,
    UnavailableBusy,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EndpointStatus {
    Active,
    UnavailableBusy,
    WaitingForDevice,
    Muted,
    Silence,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointError {
    DeviceNotFound(String),
    DeviceDisconnected,
    DeviceBusy,
    UnsupportedFormat(String),
    IoError(String),
    NotImplemented,
}

impl fmt::Display for EndpointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DeviceNotFound(id) => write!(f, "device not found: {id}"),
            Self::DeviceDisconnected => write!(f, "device disconnected"),
            Self::DeviceBusy => write!(f, "device busy"),
            Self::UnsupportedFormat(fmt) => write!(f, "unsupported format: {fmt}"),
            Self::IoError(msg) => write!(f, "I/O error: {msg}"),
            Self::NotImplemented => write!(f, "not implemented"),
        }
    }
}

impl std::error::Error for EndpointError {}

pub trait AudioBackend: Send + Sync {
    fn start_capture(&mut self, device_id: &str) -> Result<(), EndpointError>;
    fn stop_capture(&mut self) -> Result<(), EndpointError>;
    fn device_status(&self) -> DeviceStatus;
}

pub trait VirtualMicrophone: Send + Sync {
    fn start(&mut self) -> Result<(), EndpointError>;
    fn stop(&mut self) -> Result<(), EndpointError>;
    fn status(&self) -> EndpointStatus;
}
