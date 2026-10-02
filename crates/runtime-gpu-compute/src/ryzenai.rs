//! AMD Ryzen AI NPU & GPU runtime abstraction.
//!
//! Detects the AMD XDNA / XDNA 2 NPU stack (Linux `amdnpu` / `amdxdna` + XRT, Windows `VitisAI`)
//! and provides a host-copy passthrough standing in for execution.
//!
//! # Known gap: no NPU/GPU execution
//!
//! Nothing is loaded into XRT or `VitisAI` and no subgraph is dispatched: `process_frame`
//! copies the input to the output on the host. [`RyzenAiContext::executes_inference`] is
//! therefore `false`.

use std::fmt;
use std::path::Path;

/// Target execution device category for AMD Ryzen AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RyzenAiDeviceType {
    #[default]
    XdnaNpu,
    RdnaGpu,
}

impl RyzenAiDeviceType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::XdnaNpu => "amd-xdna-npu",
            Self::RdnaGpu => "amd-rdna-igpu",
        }
    }

    #[must_use]
    pub const fn is_npu(self) -> bool {
        matches!(self, Self::XdnaNpu)
    }

    #[must_use]
    pub const fn is_gpu(self) -> bool {
        matches!(self, Self::RdnaGpu)
    }
}

/// Detected driver and execution provider environment for AMD Ryzen AI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RyzenAiDriverKind {
    /// Linux AMD XDNA driver (`/dev/amdxdna` or `amdnpu` DKMS + XRT).
    LinuxAmdXdna,
    /// Windows AMD NPU driver + Vitis AI Execution Provider.
    WindowsVitisAi,
    /// Emulated mock runtime for testing and offline qualification.
    MockEmulated,
    /// No AMD Ryzen AI driver detected.
    #[default]
    None,
}

impl RyzenAiDriverKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LinuxAmdXdna => "Linux amdxdna / amdnpu (XRT)",
            Self::WindowsVitisAi => "Windows Vitis AI (NPU Driver)",
            Self::MockEmulated => "AMD Ryzen AI Emulated",
            Self::None => "None",
        }
    }

    #[must_use]
    pub const fn is_detected(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Errors occurring during AMD Ryzen AI initialization or inference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RyzenAiError {
    DriverNotFound(String),
    DeviceInitializationFailed(String),
    ExecutionFailed(String),
    UnsupportedDevice(String),
}

impl fmt::Display for RyzenAiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DriverNotFound(msg) => write!(f, "AMD Ryzen AI driver not found: {msg}"),
            Self::DeviceInitializationFailed(msg) => {
                write!(f, "AMD Ryzen AI device initialization failed: {msg}")
            }
            Self::ExecutionFailed(msg) => write!(f, "AMD Ryzen AI compute execution failed: {msg}"),
            Self::UnsupportedDevice(msg) => write!(f, "AMD Ryzen AI unsupported device: {msg}"),
        }
    }
}

impl std::error::Error for RyzenAiError {}

/// Known driver and device paths on Linux.
const LINUX_XDNA_DEVICE_PATHS: &[&str] = &[
    "/dev/amdxdna",
    "/dev/accel/accel0",
    "/sys/bus/pci/drivers/amdxdna",
    "/sys/module/amdxdna",
];

/// Known user-space runtime libraries for XRT on Linux.
const LINUX_XRT_LIB_CANDIDATES: &[&str] = &[
    "/opt/xilinx/xrt/lib/libxrt_core.so",
    "/opt/xilinx/xrt/lib/libxrt_coreutil.so",
    "/usr/lib64/libxrt_core.so",
    "/usr/lib/x86_64-linux-gnu/libxrt_core.so",
];

/// Known dynamic libraries for Windows Vitis AI.
#[cfg(target_os = "windows")]
const WINDOWS_VITIS_LIB_CANDIDATES: &[&str] = &[
    "xrt_core.dll",
    "vitis_ai_onnxruntime.dll",
    "C:\\Windows\\System32\\DriverStore\\FileRepository\\amdnpu.inf_amd64_*\\xrt_core.dll",
];

/// Detects the AMD Ryzen AI driver status on the current host.
#[must_use]
pub fn detect_driver() -> RyzenAiDriverKind {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::LibraryLoader::LoadLibraryA;
        for &dll in WINDOWS_VITIS_LIB_CANDIDATES {
            if Path::new(dll).exists() {
                return RyzenAiDriverKind::WindowsVitisAi;
            }
            let mut null_terminated = dll.as_bytes().to_vec();
            null_terminated.push(0);
            // SAFETY: Calling LoadLibraryA with valid null-terminated path.
            let handle = unsafe { LoadLibraryA(null_terminated.as_ptr()) };
            if !handle.is_null() {
                return RyzenAiDriverKind::WindowsVitisAi;
            }
        }
        RyzenAiDriverKind::None
    }
    #[cfg(target_os = "linux")]
    {
        let has_dev = LINUX_XDNA_DEVICE_PATHS
            .iter()
            .any(|p| Path::new(p).exists());
        let has_lib = LINUX_XRT_LIB_CANDIDATES
            .iter()
            .any(|p| Path::new(p).exists());
        if has_dev || has_lib {
            RyzenAiDriverKind::LinuxAmdXdna
        } else {
            RyzenAiDriverKind::None
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        RyzenAiDriverKind::None
    }
}

/// Probes whether AMD Ryzen AI NPU hardware or driver is available.
#[must_use]
pub fn is_ryzenai_available() -> bool {
    detect_driver().is_detected()
}

/// AMD Ryzen AI runtime context. Detects the driver stack; execution is a host-copy passthrough.
#[derive(Debug)]
pub struct RyzenAiContext {
    device_type: RyzenAiDeviceType,
    driver_kind: RyzenAiDriverKind,
    is_mock: bool,
    simulated_failure: bool,
    device_name: String,
}

impl RyzenAiContext {
    /// Attempts to initialize a native AMD Ryzen AI context.
    ///
    /// # Errors
    /// Returns `RyzenAiError` if the required kernel driver or runtime is missing.
    pub fn new(device_type: RyzenAiDeviceType) -> Result<Self, RyzenAiError> {
        let driver = detect_driver();
        if !driver.is_detected() {
            return Err(RyzenAiError::DriverNotFound(
                "AMD Ryzen AI driver not found. On Linux: requires amdxdna driver + XRT; on Windows: requires Vitis AI NPU driver".to_owned(),
            ));
        }

        Ok(Self {
            device_type,
            driver_kind: driver,
            is_mock: false,
            simulated_failure: false,
            device_name: format!(
                "AMD Ryzen AI ({}, {})",
                device_type.as_str(),
                driver.as_str()
            ),
        })
    }

    /// Creates a mock Ryzen AI context for testing and offline qualification.
    #[must_use]
    pub fn new_mock(device_type: RyzenAiDeviceType) -> Self {
        Self {
            device_type,
            driver_kind: RyzenAiDriverKind::MockEmulated,
            is_mock: true,
            simulated_failure: false,
            device_name: format!("AMD Ryzen AI Emulated ({})", device_type.as_str()),
        }
    }

    /// Whether `process_frame` runs a neural network or compute kernel on the device.
    ///
    /// Always `false` today: no XRT / Vitis AI device is opened and no command is submitted;
    /// `process_frame` copies the input to the output on the host. Callers must not select this
    /// context as an inference accelerator until that changes.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        false
    }

    /// `false` only means the driver stack was detected; it does not mean a device was opened or
    /// any subgraph ran (see [`Self::executes_inference`]).
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.is_mock
    }

    #[must_use]
    pub const fn device_type(&self) -> RyzenAiDeviceType {
        self.device_type
    }

    #[must_use]
    pub const fn driver_kind(&self) -> RyzenAiDriverKind {
        self.driver_kind
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }

    /// Passes an audio frame through unchanged (host copy). No NPU or GPU subgraph is dispatched.
    ///
    /// # Errors
    /// Returns `RyzenAiError::ExecutionFailed` on a length mismatch or when simulated failure is set.
    pub fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), RyzenAiError> {
        if self.simulated_failure {
            return Err(RyzenAiError::ExecutionFailed(
                "simulated failure in the Ryzen AI host-copy passthrough".to_owned(),
            ));
        }

        if input.len() != output.len() {
            return Err(RyzenAiError::ExecutionFailed(format!(
                "input length {} does not match output length {}",
                input.len(),
                output.len()
            )));
        }

        // Host copy only: nothing is dispatched to the NPU or GPU.
        output.copy_from_slice(input);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ryzenai_device_types() {
        assert!(RyzenAiDeviceType::XdnaNpu.is_npu());
        assert!(!RyzenAiDeviceType::XdnaNpu.is_gpu());
        assert_eq!(RyzenAiDeviceType::XdnaNpu.as_str(), "amd-xdna-npu");

        assert!(RyzenAiDeviceType::RdnaGpu.is_gpu());
        assert!(!RyzenAiDeviceType::RdnaGpu.is_npu());
        assert_eq!(RyzenAiDeviceType::RdnaGpu.as_str(), "amd-rdna-igpu");
    }

    #[test]
    fn test_ryzenai_mock_lifecycle() {
        let mut ctx = RyzenAiContext::new_mock(RyzenAiDeviceType::XdnaNpu);
        assert!(ctx.is_mock());
        assert_eq!(ctx.device_type(), RyzenAiDeviceType::XdnaNpu);
        assert_eq!(ctx.driver_kind(), RyzenAiDriverKind::MockEmulated);

        let input = [0.5f32; 480];
        let mut output = [0.0f32; 480];
        let res = ctx.process_frame(&input, &mut output);
        assert!(res.is_ok());
        assert_eq!(output, input);
        assert!(!ctx.executes_inference());

        ctx.set_simulated_failure(true);
        let fail_res = ctx.process_frame(&input, &mut output);
        assert!(matches!(fail_res, Err(RyzenAiError::ExecutionFailed(_))));
    }

    #[test]
    fn test_ryzenai_length_mismatch() {
        let mut ctx = RyzenAiContext::new_mock(RyzenAiDeviceType::RdnaGpu);
        let input = [0.1f32; 480];
        let mut output = [0.0f32; 240];
        let res = ctx.process_frame(&input, &mut output);
        assert!(matches!(res, Err(RyzenAiError::ExecutionFailed(_))));
    }
}
