//! One-shot diagnosis of the OpenVINO runtime the process would use.
//!
//! Distinguishes the cases the UI and `GetStatus` must tell apart: no library at all, a library
//! of an unsupported release, a library that is present but cannot be loaded, an Intel NPU on the
//! host without the NPU plugin (the Fedora 2026.0 `openvino` package ships only CPU/GPU), an NPU
//! plugin that does not enumerate the device (driver/permissions), and a working runtime.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use crate::core::OpenVinoCore;
use crate::error::OpenVinoError;
use crate::ffi::{LibrarySource, NOT_FOUND_MESSAGE, OpenVinoLibrary, candidate_libraries};

/// Outcome of [`probe_runtime`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuntimeState {
    /// Loaded; every Intel accelerator present on the host is enumerated.
    Ok,
    /// No `libopenvino_c` in any search directory.
    NotFound,
    /// A library was found but reports a release outside `SUPPORTED_MAJOR_VERSIONS`.
    IncompatibleVersion,
    /// A library was found but could not be loaded (missing dependency, missing symbol, ...).
    LoadFailed,
    /// The host has an Intel NPU but the loaded runtime has no NPU plugin.
    NpuPluginMissing,
    /// The NPU plugin is present but OpenVINO does not list the device (driver, firmware or
    /// `/dev/accel` permissions).
    NpuDeviceUnavailable,
}

impl RuntimeState {
    /// Stable machine code (`GetStatus.openvino_runtime.status`).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::NotFound => "not_found",
            Self::IncompatibleVersion => "incompatible_version",
            Self::LoadFailed => "load_failed",
            Self::NpuPluginMissing => "npu_plugin_missing",
            Self::NpuDeviceUnavailable => "npu_device_unavailable",
        }
    }

    /// Whether inference can run on at least the CPU device.
    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(
            self,
            Self::Ok | Self::NpuPluginMissing | Self::NpuDeviceUnavailable
        )
    }
}

/// Result of [`probe_runtime`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeProbe {
    pub state: RuntimeState,
    /// Library that was loaded (or the first candidate that failed).
    pub library: Option<PathBuf>,
    /// Where [`Self::library`] came from (`env`, `embedded`, `system`).
    pub source: Option<LibrarySource>,
    /// `ov_get_openvino_version` build number when the library loaded.
    pub version: Option<String>,
    /// Devices OpenVINO enumerates (`CPU`, `GPU`, `NPU`, ...).
    pub devices: Vec<String>,
    /// Whether an Intel NPU device node exists on the host.
    pub npu_hardware: bool,
    /// Human-readable explanation (pt-BR, shown by the UI).
    pub message: String,
}

/// File name of the NPU plugin next to the core library.
#[must_use]
pub const fn npu_plugin_file_name() -> &'static str {
    if cfg!(windows) {
        "openvino_intel_npu_plugin.dll"
    } else {
        "libopenvino_intel_npu_plugin.so"
    }
}

/// Whether the host exposes an NPU device node (`/dev/accel/accel*`, the `intel_vpu` driver).
#[must_use]
pub fn npu_hardware_present() -> bool {
    if cfg!(target_os = "linux") {
        std::fs::read_dir("/dev/accel").is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with("accel"))
        })
    } else {
        false
    }
}

/// Classifies an already-loaded runtime. Pure, so every branch is unit-tested.
#[must_use]
pub fn classify_loaded(
    devices: &[String],
    npu_hardware: bool,
    npu_plugin_present: bool,
) -> (RuntimeState, &'static str) {
    let has_npu = devices
        .iter()
        .any(|device| device.split('.').next() == Some("NPU"));
    if !npu_hardware || has_npu {
        (RuntimeState::Ok, "Runtime OpenVINO carregada.")
    } else if npu_plugin_present {
        (
            RuntimeState::NpuDeviceUnavailable,
            "O plugin NPU do OpenVINO está presente, mas a NPU não foi listada: confira o driver \
             (intel-npu-driver / level-zero), o firmware e o acesso a /dev/accel (grupo render).",
        )
    } else {
        (
            RuntimeState::NpuPluginMissing,
            "A runtime OpenVINO carregada não tem o plugin NPU (libopenvino_intel_npu_plugin); \
             CPU e GPU continuam disponíveis. O pacote do Clearcore traz a runtime com o plugin \
             NPU em resources/openvino.",
        )
    }
}

/// Classifies a load failure.
#[must_use]
pub fn classify_failure(error: &OpenVinoError) -> RuntimeState {
    match error {
        OpenVinoError::UnsupportedVersion(_) => RuntimeState::IncompatibleVersion,
        OpenVinoError::LibraryNotFound(message) if message == NOT_FOUND_MESSAGE => {
            RuntimeState::NotFound
        }
        _ => RuntimeState::LoadFailed,
    }
}

/// Whether the NPU plugin sits next to `library` (wheel / embedded layout) or in an
/// `openvino-<version>/` sub-directory (Fedora layout: `/usr/lib64/openvino-2026.0.0/`).
fn plugin_next_to(library: &Path) -> bool {
    let Some(dir) = library.parent() else {
        return false;
    };
    if dir.join(npu_plugin_file_name()).is_file() {
        return true;
    }
    std::fs::read_dir(dir).is_ok_and(|entries| {
        entries.flatten().any(|entry| {
            entry.file_name().to_string_lossy().starts_with("openvino-")
                && entry.path().join(npu_plugin_file_name()).is_file()
        })
    })
}

/// Probes the runtime (loads the library, creates a core, lists the devices). Expensive: it loads
/// every device plugin. Use [`probe_runtime_cached`] for status polling.
#[must_use]
pub fn probe_runtime() -> RuntimeProbe {
    let npu_hardware = npu_hardware_present();
    let candidates = candidate_libraries();
    let source_of = |path: &Path| {
        candidates
            .iter()
            .find(|(candidate, _)| candidate == path)
            .map(|(_, source)| *source)
    };
    match OpenVinoLibrary::get_or_init() {
        Ok(lib) => {
            let library = lib.loaded_path().to_path_buf();
            let core = OpenVinoCore::new_with_lib(lib);
            let version = core.as_ref().ok().and_then(|core| core.version().ok());
            let devices = core
                .and_then(|core| core.available_devices())
                .unwrap_or_default();
            let (state, message) =
                classify_loaded(&devices, npu_hardware, plugin_next_to(&library));
            RuntimeProbe {
                state,
                source: source_of(&library),
                library: Some(library),
                version,
                devices,
                npu_hardware,
                message: message.to_owned(),
            }
        }
        Err(error) => {
            let state = classify_failure(&error);
            let message = match state {
                RuntimeState::NotFound => "OpenVINO não encontrado: nenhuma libopenvino_c em \
                     OPENVINO_LIB_PATH, resources/openvino do aplicativo, /usr/lib64, /lib64 ou \
                     /usr/local/lib64."
                    .to_owned(),
                RuntimeState::IncompatibleVersion => {
                    format!("Versão do OpenVINO incompatível: {error}")
                }
                _ => format!("OpenVINO encontrado, mas não carregou: {error}"),
            };
            let first = candidates.first();
            RuntimeProbe {
                state,
                library: first.map(|(path, _)| path.clone()),
                source: first.map(|(_, source)| *source),
                version: None,
                devices: Vec::new(),
                npu_hardware,
                message,
            }
        }
    }
}

static PROBE: OnceLock<RuntimeProbe> = OnceLock::new();

/// [`probe_runtime`] computed once per process (the library is a process-wide singleton anyway).
#[must_use]
pub fn probe_runtime_cached() -> &'static RuntimeProbe {
    PROBE.get_or_init(probe_runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn devices(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn a_runtime_without_npu_hardware_is_ok() {
        let (state, _) = classify_loaded(&devices(&["CPU", "GPU"]), false, false);
        assert_eq!(state, RuntimeState::Ok);
    }

    #[test]
    fn npu_hardware_with_the_device_listed_is_ok() {
        let (state, _) = classify_loaded(&devices(&["CPU", "GPU", "NPU"]), true, true);
        assert_eq!(state, RuntimeState::Ok);
        let (state, _) = classify_loaded(&devices(&["CPU", "NPU.0"]), true, false);
        assert_eq!(state, RuntimeState::Ok);
    }

    #[test]
    fn npu_hardware_without_the_plugin_is_reported_as_such() {
        let (state, message) = classify_loaded(&devices(&["CPU", "GPU"]), true, false);
        assert_eq!(state, RuntimeState::NpuPluginMissing);
        assert_eq!(state.code(), "npu_plugin_missing");
        assert!(message.contains("plugin NPU"), "{message}");
        assert!(state.is_usable());
    }

    #[test]
    fn a_plugin_that_does_not_list_the_npu_points_at_the_driver() {
        let (state, message) = classify_loaded(&devices(&["CPU"]), true, true);
        assert_eq!(state, RuntimeState::NpuDeviceUnavailable);
        assert!(message.contains("driver"), "{message}");
    }

    #[test]
    fn load_failures_are_told_apart() {
        assert_eq!(
            classify_failure(&OpenVinoError::LibraryNotFound(
                NOT_FOUND_MESSAGE.to_owned()
            )),
            RuntimeState::NotFound
        );
        assert_eq!(
            classify_failure(&OpenVinoError::UnsupportedVersion("2024.6".to_owned())),
            RuntimeState::IncompatibleVersion
        );
        assert_eq!(
            classify_failure(&OpenVinoError::LibraryNotFound(
                "dlopen failed for /x: libopenvino.so.2641: cannot open".to_owned()
            )),
            RuntimeState::LoadFailed
        );
        assert_eq!(
            classify_failure(&OpenVinoError::SymbolNotFound("ov_free".to_owned())),
            RuntimeState::LoadFailed
        );
        for state in [
            RuntimeState::NotFound,
            RuntimeState::IncompatibleVersion,
            RuntimeState::LoadFailed,
        ] {
            assert!(!state.is_usable());
        }
    }

    #[test]
    fn every_state_has_a_distinct_code() {
        let codes: std::collections::HashSet<_> = [
            RuntimeState::Ok,
            RuntimeState::NotFound,
            RuntimeState::IncompatibleVersion,
            RuntimeState::LoadFailed,
            RuntimeState::NpuPluginMissing,
            RuntimeState::NpuDeviceUnavailable,
        ]
        .iter()
        .map(|state| state.code())
        .collect();
        assert_eq!(codes.len(), 6);
    }

    #[test]
    fn the_live_probe_is_consistent() {
        let probe = probe_runtime();
        if probe.state.is_usable() {
            assert!(probe.library.is_some());
            assert!(probe.version.is_some());
            assert!(probe.devices.iter().any(|d| d == "CPU"), "{probe:?}");
        }
        assert!(!probe.message.is_empty());
    }
}
