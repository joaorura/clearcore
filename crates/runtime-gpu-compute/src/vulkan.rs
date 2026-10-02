//! Vulkan Compute (SPIR-V) runtime abstraction.
//!
//! Vulkan loader probing plus a host-copy passthrough standing in for compute execution.
//!
//! # Known gap: no GPU execution
//!
//! The crate probes for the Vulkan ICD loader and embeds a no-op SPIR-V module, but it creates
//! no `VkInstance`/`VkDevice`, builds no pipeline and submits no work: `process_frame` copies the
//! input to the output on the host. [`VulkanContext::executes_inference`] is therefore `false`.

use std::fmt;
use std::path::Path;

/// Target execution device category for Vulkan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum VulkanDeviceType {
    #[default]
    DedicatedGpu,
    IntegratedGpu,
}

impl VulkanDeviceType {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DedicatedGpu => "vulkan-dgpu",
            Self::IntegratedGpu => "vulkan-igpu",
        }
    }
}

/// Errors occurring during Vulkan runtime initialization or execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VulkanError {
    LoaderNotFound(String),
    DeviceNotFound(String),
    PipelineCreationFailed(String),
    ExecutionFailed(String),
}

impl fmt::Display for VulkanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LoaderNotFound(msg) => write!(f, "Vulkan ICD loader not found: {msg}"),
            Self::DeviceNotFound(msg) => write!(f, "Vulkan physical device not found: {msg}"),
            Self::PipelineCreationFailed(msg) => {
                write!(f, "Vulkan compute pipeline creation failed: {msg}")
            }
            Self::ExecutionFailed(msg) => write!(f, "Vulkan compute failed: {msg}"),
        }
    }
}

impl std::error::Error for VulkanError {}

/// Known dynamic library paths for the Vulkan loader across platforms.
const VULKAN_LIB_CANDIDATES: &[&str] = &[
    #[cfg(unix)]
    "libvulkan.so.1",
    #[cfg(unix)]
    "libvulkan.so",
    #[cfg(unix)]
    "/usr/lib64/libvulkan.so.1",
    #[cfg(unix)]
    "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
    #[cfg(windows)]
    "vulkan-1.dll",
    #[cfg(windows)]
    "C:\\Windows\\System32\\vulkan-1.dll",
];

/// Probes whether the Vulkan ICD loader library is available on the system.
#[must_use]
pub fn is_vulkan_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::LibraryLoader::LoadLibraryA;
        let mut found = false;
        for &dll in VULKAN_LIB_CANDIDATES {
            let mut null_terminated = dll.as_bytes().to_vec();
            null_terminated.push(0);
            // SAFETY: Calling LoadLibraryA with a valid null-terminated ASCII path.
            let handle = unsafe { LoadLibraryA(null_terminated.as_ptr()) };
            if !handle.is_null() {
                found = true;
                break;
            }
        }
        found
    }
    #[cfg(unix)]
    {
        use std::ffi::CString;
        for &candidate in VULKAN_LIB_CANDIDATES {
            if Path::new(candidate).exists() {
                return true;
            }
            if let Ok(c_str) = CString::new(candidate) {
                // SAFETY: Probing dlopen with RTLD_LAZY | RTLD_LOCAL; closing immediately if opened.
                let handle =
                    unsafe { libc::dlopen(c_str.as_ptr(), libc::RTLD_LAZY | libc::RTLD_LOCAL) };
                if !handle.is_null() {
                    // SAFETY: Closing valid handle opened above.
                    unsafe { libc::dlclose(handle) };
                    return true;
                }
            }
        }
        false
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        VULKAN_LIB_CANDIDATES.iter().any(|p| Path::new(p).exists())
    }
}

/// Valid SPIR-V 1.0 compute shader binary (32-bit words).
///
/// Dispatches a compute workgroup (LocalSize: 64, 1, 1) performing
/// element-wise audio buffer transformation.
pub const SPIRV_COMPUTE_WORDS: &[u32] = &[
    0x07230203, // Magic number
    0x00010000, // Version: 1.0
    0x00140001, // Generator ID
    20,         // Bound
    0,          // Schema
    // OpCapability Shader
    0x00020011, 1, // OpMemoryModel Logical Simple
    0x0003000e, 0, 0, // OpEntryPoint GLCompute %1 "main"
    0x0005000f, 5, 1, 0x6e69616d, 0, // OpExecutionMode %1 LocalSize 64 1 1
    0x00060010, 1, 17, 64, 1, 1, // OpTypeVoid %2
    0x00020013, 2, // OpTypeFunction %3 %2
    0x00030021, 3, 2, // OpFunction %2 %1 None %3
    0x00050036, 2, 1, 0, 3, // OpLabel %4
    0x000200f8, 4,          // OpReturn
    0x000100fd, // OpFunctionEnd
    0x00010038,
];

/// Returns the embedded SPIR-V compute shader bytecode as raw bytes.
#[must_use]
pub fn spirv_compute_bytes() -> &'static [u8] {
    // SAFETY: Transmuting &[u32] to &[u8] with length = words * 4 is safe and memory aligned.
    unsafe {
        std::slice::from_raw_parts(
            SPIRV_COMPUTE_WORDS.as_ptr().cast::<u8>(),
            std::mem::size_of_val(SPIRV_COMPUTE_WORDS),
        )
    }
}

/// Vulkan execution context managing physical device, compute queue, and SPIR-V dispatch.
#[derive(Debug)]
pub struct VulkanContext {
    device_type: VulkanDeviceType,
    is_mock: bool,
    simulated_failure: bool,
    device_name: String,
    workgroup_size: (u32, u32, u32),
}

impl VulkanContext {
    /// Attempts to initialize a native Vulkan compute context.
    ///
    /// # Errors
    /// Returns `VulkanError` if the Vulkan loader or compute device is not available.
    pub fn new(device_type: VulkanDeviceType) -> Result<Self, VulkanError> {
        if !is_vulkan_available() {
            return Err(VulkanError::LoaderNotFound(
                "libvulkan.so.1 / vulkan-1.dll was not found on the host system".to_owned(),
            ));
        }

        Ok(Self {
            device_type,
            is_mock: false,
            simulated_failure: false,
            device_name: format!(
                "Vulkan loader present, no device opened ({})",
                device_type.as_str()
            ),
            workgroup_size: (64, 1, 1),
        })
    }

    /// Creates a mock Vulkan context for testing and offline qualification.
    #[must_use]
    pub fn new_mock(device_type: VulkanDeviceType) -> Self {
        Self {
            device_type,
            is_mock: true,
            simulated_failure: false,
            device_name: format!("Vulkan Emulated Compute ({})", device_type.as_str()),
            workgroup_size: (64, 1, 1),
        }
    }

    /// Whether `process_frame` runs a neural network or compute kernel on the device.
    ///
    /// Always `false` today: no Vulkan device is opened and no command is submitted;
    /// `process_frame` copies the input to the output on the host. Callers must not select this
    /// context as an inference accelerator until that changes.
    #[must_use]
    pub const fn executes_inference(&self) -> bool {
        false
    }

    /// `false` only means the Vulkan loader was found; it does not mean a device was opened or
    /// any compute ran (see [`Self::executes_inference`]).
    #[must_use]
    pub const fn is_mock(&self) -> bool {
        self.is_mock
    }

    #[must_use]
    pub const fn device_type(&self) -> VulkanDeviceType {
        self.device_type
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    #[must_use]
    pub const fn workgroup_size(&self) -> (u32, u32, u32) {
        self.workgroup_size
    }

    pub const fn set_simulated_failure(&mut self, fail: bool) {
        self.simulated_failure = fail;
    }

    /// Passes an audio frame through unchanged (host copy). No SPIR-V pipeline is dispatched.
    ///
    /// # Errors
    /// Returns `VulkanError::ExecutionFailed` on a length mismatch or when simulated failure is set.
    pub fn process_frame(&mut self, input: &[f32], output: &mut [f32]) -> Result<(), VulkanError> {
        if self.simulated_failure {
            return Err(VulkanError::ExecutionFailed(
                "simulated failure in the Vulkan host-copy passthrough".to_owned(),
            ));
        }

        if input.len() != output.len() {
            return Err(VulkanError::ExecutionFailed(format!(
                "input length {} does not match output length {}",
                input.len(),
                output.len()
            )));
        }

        // Host copy only: nothing is submitted to a Vulkan queue.
        output.copy_from_slice(input);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vulkan_spirv_header() {
        assert_eq!(SPIRV_COMPUTE_WORDS[0], 0x07230203); // SPIR-V Magic
        assert_eq!(SPIRV_COMPUTE_WORDS[1], 0x00010000); // Version 1.0

        let bytes = spirv_compute_bytes();
        assert_eq!(bytes.len(), SPIRV_COMPUTE_WORDS.len() * 4);
        assert_eq!(bytes[0..4], [0x03, 0x02, 0x23, 0x07]);
    }

    #[test]
    fn test_vulkan_mock_lifecycle() {
        let mut ctx = VulkanContext::new_mock(VulkanDeviceType::DedicatedGpu);
        assert!(ctx.is_mock());
        assert_eq!(ctx.device_type(), VulkanDeviceType::DedicatedGpu);
        assert_eq!(ctx.device_type().as_str(), "vulkan-dgpu");
        assert_eq!(ctx.workgroup_size(), (64, 1, 1));

        let input = [0.25f32; 480];
        let mut output = [0.0f32; 480];
        let res = ctx.process_frame(&input, &mut output);
        assert!(res.is_ok());
        assert_eq!(output, input);
        assert!(!ctx.executes_inference());

        ctx.set_simulated_failure(true);
        let fail_res = ctx.process_frame(&input, &mut output);
        assert!(matches!(fail_res, Err(VulkanError::ExecutionFailed(_))));
    }

    #[test]
    fn test_vulkan_length_mismatch() {
        let mut ctx = VulkanContext::new_mock(VulkanDeviceType::IntegratedGpu);
        let input = [0.1f32; 480];
        let mut output = [0.0f32; 240];
        let res = ctx.process_frame(&input, &mut output);
        assert!(matches!(res, Err(VulkanError::ExecutionFailed(_))));
    }
}
