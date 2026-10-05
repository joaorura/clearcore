#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use std::path::{Path, PathBuf};

use realtime_noise_accelerators::{BackendSelection, OpenVINOBackend};
use realtime_noise_model::{
    APPROVED_ASSET_SHA256, ApprovedAssetManifest, CpuProfile, InferenceBackend, TractBackend,
};

/// Resolution metadata describing the selected backend and its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendResolutionInfo {
    pub name: String,
    pub runtime: String,
    pub device: String,
    pub is_hardware_accelerated: bool,
    pub is_fallback: bool,
    pub fallback_reason: Option<String>,
}

/// Attempts to locate the directory containing stateful model graphs (`models/stateful/enc.onnx`).
#[must_use]
pub fn find_stateful_model_dir() -> Option<PathBuf> {
    if let Ok(path) =
        std::env::var("CLEARCORE_STATEFUL_DIR").or_else(|_| std::env::var("CLEARCORE_MODEL_DIR"))
    {
        let p = PathBuf::from(path);
        if p.join("enc.onnx").exists() {
            return Some(p);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let mut cur = Some(cwd.as_path());
        for _ in 0..5 {
            if let Some(p) = cur {
                let candidate = p.join("models/stateful");
                if candidate.join("enc.onnx").exists() {
                    return Some(candidate);
                }
                cur = p.parent();
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent();
        for _ in 0..5 {
            if let Some(p) = cur {
                let candidate = p.join("models/stateful");
                if candidate.join("enc.onnx").exists() {
                    return Some(candidate);
                }
                let candidate_res = p.join("resources/models/stateful");
                if candidate_res.join("enc.onnx").exists() {
                    return Some(candidate_res);
                }
                cur = p.parent();
            }
        }
    }

    None
}

/// Attempts to locate the repository root containing governance records and manifests.
#[must_use]
pub fn find_repo_root() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("CLEARCORE_REPO_ROOT") {
        let p = PathBuf::from(path);
        if p.join("governance/model-assets").exists() {
            return Some(p);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let mut cur = Some(cwd.as_path());
        for _ in 0..5 {
            if let Some(p) = cur {
                if p.join("governance/model-assets").exists() {
                    return Some(p.to_path_buf());
                }
                cur = p.parent();
            }
        }
    }

    if let Ok(exe) = std::env::current_exe() {
        let mut cur = exe.parent();
        for _ in 0..5 {
            if let Some(p) = cur {
                if p.join("governance/model-assets").exists() {
                    return Some(p.to_path_buf());
                }
                cur = p.parent();
            }
        }
    }

    None
}

enum OpenVinoTarget {
    Auto,
    Device(String),
}

fn parse_openvino_target(request: &str) -> Option<OpenVinoTarget> {
    let lower = request.to_ascii_lowercase().replace('_', "-");
    match lower.as_str() {
        "auto" | "" | "default" | "openvino" => Some(OpenVinoTarget::Auto),
        "openvino-npu" | "npu" | "intel-npu" => Some(OpenVinoTarget::Device("NPU".to_string())),
        "openvino-gpu" | "gpu" | "intel-gpu" | "arc" => {
            Some(OpenVinoTarget::Device("GPU".to_string()))
        }
        "openvino-cpu" | "intel-cpu" => Some(OpenVinoTarget::Device("CPU".to_string())),
        _ => None,
    }
}

/// Instantiates an inference backend based on `request`, falling back safely to Tract CPU.
///
/// If `request` resolves to `OpenVINO` and stateful models (`models/stateful`) are present,
/// compiles and instantiates `OpenVINOBackend`. If hardware or models are missing, or if Tract CPU
/// is explicitly requested, instantiates `TractBackend` (or pure-Rust Tract baseline).
#[must_use]
pub fn instantiate_backend_with_fallback(
    request: &str,
    model_dir: Option<&Path>,
    repo_root: Option<&Path>,
) -> (Box<dyn InferenceBackend>, BackendResolutionInfo) {
    let resolved_model_dir = model_dir
        .map(Path::to_path_buf)
        .or_else(find_stateful_model_dir);
    let resolved_repo_root = repo_root.map(Path::to_path_buf).or_else(find_repo_root);

    let trimmed = request.trim();
    let is_tract_request = matches!(
        trimmed.to_ascii_lowercase().replace('_', "-").as_str(),
        "tract" | "cpu-tract" | "tract-cpu" | "cpu"
    );

    let mut fallback_reason = None;

    if !is_tract_request {
        if let Some(target) = parse_openvino_target(trimmed) {
            if OpenVINOBackend::is_available() {
                if let Some(ref dir) = resolved_model_dir {
                    let load_res = match target {
                        OpenVinoTarget::Auto => OpenVINOBackend::load_stateful_auto(
                            dir,
                            "df-compatible-release-asset-v1",
                            APPROVED_ASSET_SHA256,
                        ),
                        OpenVinoTarget::Device(ref dev) => OpenVINOBackend::load_stateful(
                            dir,
                            dev,
                            "df-compatible-release-asset-v1",
                            APPROVED_ASSET_SHA256,
                        ),
                    };

                    match load_res {
                        Ok(backend) => {
                            let device = backend.device().to_string();
                            let descriptor = backend.descriptor();
                            let is_accelerated = backend.is_hardware_accelerated();
                            let name = format!("openvino-{}", device.to_lowercase());
                            let info = BackendResolutionInfo {
                                name,
                                runtime: descriptor.runtime.to_string(),
                                device,
                                is_hardware_accelerated: is_accelerated,
                                is_fallback: false,
                                fallback_reason: None,
                            };
                            return (Box::new(backend), info);
                        }
                        Err(err) => {
                            fallback_reason =
                                Some(format!("OpenVINO stateful initialization failed: {err}"));
                        }
                    }
                } else {
                    fallback_reason = Some(
                        "models/stateful directory not found for OpenVINO stateful backend"
                            .to_string(),
                    );
                }
            } else {
                fallback_reason = Some("OpenVINO native runtime is not installed".to_string());
            }
        } else {
            fallback_reason = Some(format!(
                "Backend '{trimmed}' is not supported or requires fallback"
            ));
        }
    }

    // Safe fallback to Tract CPU
    let was_accelerator_requested = !is_tract_request;
    if let Some(ref root) = resolved_repo_root {
        let tract_backend = ApprovedAssetManifest::verify(root)
            .ok()
            .and_then(|manifest| TractBackend::new(&manifest, CpuProfile::Avx2Minimum).ok());

        if let Some(tract) = tract_backend {
            let desc = tract.descriptor();
            let info = BackendResolutionInfo {
                name: "tract".to_string(),
                runtime: desc.runtime.to_string(),
                device: "CPU".to_string(),
                is_hardware_accelerated: false,
                is_fallback: was_accelerator_requested,
                fallback_reason,
            };
            return (Box::new(tract), info);
        }
    }

    // Pure Rust fail-safe mock fallback (if manifest/assets are not in tree, e.g. minimal test)
    let mock = BackendSelection::TractCpu.instantiate_mock_backend();
    let desc = mock.descriptor();
    let info = BackendResolutionInfo {
        name: "tract-mock".to_string(),
        runtime: desc.runtime.to_string(),
        device: "CPU".to_string(),
        is_hardware_accelerated: false,
        is_fallback: was_accelerator_requested,
        fallback_reason: fallback_reason.or_else(|| {
            if was_accelerator_requested {
                Some("Fallback to mock Tract baseline".to_string())
            } else {
                None
            }
        }),
    };
    (mock, info)
}
