//! DirectML (DirectX 12 Compute) runtime abstraction.
//!
//! DirectML/DirectX 12 library probing plus a host-copy passthrough standing in for execution.
//!
//! # Known gap: no GPU execution
//!
//! No `ID3D12Device`/`IDMLDevice` is created and no DirectML operator is dispatched:
//! `process_frame` copies the input to the output on the host.
//! [`DirectMlContext::executes_inference`] is therefore `false`.

use std::fmt;
use std::path::Path;

/// Target execution device category for DirectML.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum DirectMlDeviceType {
    #[default]
    DedicatedGpu,
    IntegratedGpu,
}

impl DirectMlDeviceType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DedicatedGpu => "directml-dgpu",
            Self::IntegratedGpu => "directml-igpu",
        }
    }
}

/// Errors occurring during DirectML runtime initialization or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DirectMlError {
    LoaderNotFound(String),
    DeviceInitializationFailed(String),
    ExecutionFailed(String),
    UnsupportedPlatform(String),
}

impl fmt::Display for DirectMlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoaderNotFound(msg) => write!(f, "DirectML runtime loader not found: {msg}"),
            Self::DeviceInitializationFailed(msg) => {
                write!(f, "DirectML device initialization failed: {msg}")
            }
            Self::ExecutionFailed(msg) => write!(f, "DirectML compute execution failed: {msg}"),
            Self::UnsupportedPlatform(msg) => write!(f, "DirectML unsupported on platform: {msg}"),
        }
    }
}

impl std::error::Error for DirectMlError {}

/// Known dynamic library names for DirectML and DirectX 12 runtime on Windows.
const DIRECTML_DLL_CANDIDATES: &[&str] = &[
    "DirectML.dll",
    "d3d12.dll",
    "dxgi.dll",
    "C:\\Windows\\System32\\DirectML.dll",
    "C:\\Windows\\System32\\d3d12.dll",
];

/// Probes whether DirectML and DirectX 12 runtime libraries are available on the host system.
#[must_use]
pub fn is_directml_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::LibraryLoader::LoadLibraryA;
        let mut dml_found = false;
        for &dll in DIRECTML_DLL_CANDIDATES {
            let mut null_terminated = dll.as_bytes().to_vec();
            null_terminated.push(0);
            // SAFETY: Calling LoadLibraryA with a valid null-terminated ASCII path.
            let handle = unsafe { LoadLibraryA(null_terminated.as_ptr()) };
            if !handle.is_null() {
                dml_found = true;
                break;
            }
        }
        dml_found
    }
    #[cfg(not(target_os = "windows"))]
    {
        DIRECTML_DLL_CANDIDATES
            .iter()
            .any(|p| Path::new(p).exists())
    }
}

/// DirectML execution context managing device state, command queue, and tensor dispatch.
#[derive(Debug)]
pub struct DirectMlContext {
    device_type: DirectMlDeviceType,
    is_mock: bool,
    simulated_failure: bool,
    feature_level: u32,
    device_name: String,
}

impl DirectMlContext {
    /// Attempts to initialize a native DirectML runtime context on Windows.
    ///
    /// # Errors
    /// Returns `DirectMlError` if initialization fails or DirectML is unavailable.
    pub fn new(device_type: DirectMlDeviceType) -> Result<Self, DirectMlError> {
        #[cfg(target_os = "windows")]
        {
            if !is_directml_available() {
                return Err(DirectMlError::LoaderNotFound(
                    "DirectML.dll or d3d12.dll was not found on the host system".to_owned(),
                ));
            }

            Ok(Self {
                device_type,
                is_mock: false,
                simulated_failure: false,
                feature_level: 0x5000, // DML_FEATURE_LEVEL_5_0
                device_name: format!(
                    "DirectML libraries present, no device opened ({})",
                    device_type.as_str()
                ),
            })
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = device_type;
            Err(DirectMlError::UnsupportedPlatform(
                "DirectML requires Windows (DirectX 12 Compute); use Vulkan Compute on Linux"
                    .to_owned(),
            ))
        }
    }

    /// Creates a mock DirectML context for testing and offline qualification.
    #[must_use]
    pub fn new_mock(device_type: DirectMlDeviceType) -> Self {
        Self {
            device_type,
            is_mock: true,
            simulated_failure: false,
            feature_level: 0x5000,
            device_name: format!("DirectML Emulated ({})", device_type.as_str()),
        }
    }

    /// Whether `process_frame` runs a neural network or compute kernel on the device.
    ///
    /// Always `false` today: no DirectX 12 device is opened and no command is submitted;
    /// `process_frame` copies the input to the output on the host. Callers must not select this
    /// context as an inference accelerator until that changes.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        false
    }

    /// `false` only means the DirectML libraries were found; it does not mean a device was
    /// opened or any operator ran (see [`Self::executes_inference`]).
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.is_mock
    }

    #[must_use]
    pub const fn device_type(&self) -> DirectMlDeviceType {
        self.device_type
    }

    #[must_use]
    pub const fn feature_level(&self) -> u32 {
        self.feature_level
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }

    /// Passes an audio frame through unchanged (host copy). No DirectML operator is dispatched.
    ///
    /// # Errors
    /// Returns `DirectMlError::ExecutionFailed` on a length mismatch or when simulated failure is set.
    pub fn process_frame(
        &mut self,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<(), DirectMlError> {
        if self.simulated_failure {
            return Err(DirectMlError::ExecutionFailed(
                "simulated failure in the DirectML host-copy passthrough".to_owned(),
            ));
        }

        if input.len() != output.len() {
            return Err(DirectMlError::ExecutionFailed(format!(
                "input length {} does not match output length {}",
                input.len(),
                output.len()
            )));
        }

        // Host copy only: nothing is recorded on a DirectX 12 command queue.
        output.copy_from_slice(input);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_directml_mock_lifecycle() {
        let mut ctx = DirectMlContext::new_mock(DirectMlDeviceType::DedicatedGpu);
        assert!(ctx.is_mock());
        assert_eq!(ctx.device_type(), DirectMlDeviceType::DedicatedGpu);
        assert_eq!(ctx.device_type().as_str(), "directml-dgpu");
        assert_eq!(ctx.feature_level(), 0x5000);

        let input = [0.1f32; 480];
        let mut output = [0.0f32; 480];
        let res = ctx.process_frame(&input, &mut output);
        assert!(res.is_ok());
        assert_eq!(output, input);
        assert!(!ctx.executes_inference());

        ctx.set_simulated_failure(true);
        let fail_res = ctx.process_frame(&input, &mut output);
        assert!(matches!(fail_res, Err(DirectMlError::ExecutionFailed(_))));
    }

    #[test]
    fn test_directml_length_mismatch() {
        let mut ctx = DirectMlContext::new_mock(DirectMlDeviceType::IntegratedGpu);
        let input = [0.1f32; 480];
        let mut output = [0.0f32; 240];
        let res = ctx.process_frame(&input, &mut output);
        assert!(matches!(res, Err(DirectMlError::ExecutionFailed(_))));
    }
}
