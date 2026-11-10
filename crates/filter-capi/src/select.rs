//! Model and runtime selection for the filter.
//!
//! Model (fail closed, no base-model fallback):
//! 1. `CLEARCORE_DEV_PDFNET3_V3_ASSET` + `_SHA256` (both or neither) — a v3 FULL archive;
//! 2. `CLEARCORE_DEV_PDFNET3_STATEFUL_ASSET` + `_SHA256` — a v2 stateful archive;
//! 3. the bundled v3 FULL archive `<models>/pdfnet3/pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz`
//!    (SHA-256 pinned in [`crate::v3::V3_BUNDLED_SHA256`]); present but not matching = no filter;
//!
//! The older bundled v2 stateful archive is no longer shipped: without the v3 full archive (and
//! without an override) there is no filter.
//!
//! Runtime: the `backend` of `settings.json` (the one the UI writes), overridden by
//! `CLEARCORE_FILTER_BACKEND`:
//!
//! | value | devices tried, in order |
//! | :--- | :--- |
//! | `openvino-npu`, `npu`, `intel-npu`, `openvino` | NPU, GPU, CPU (OpenVINO) |
//! | `openvino-gpu`, `intel-gpu`, `arc`, `gpu` | GPU, CPU (OpenVINO) |
//! | `openvino-cpu`, `intel-cpu` | CPU (OpenVINO) |
//! | anything else (`auto`, `tract`, `cpu`, `ort`, none) | ONNX Runtime CPU |
//!
//! Every OpenVINO device must compile and pass a real-time qualification (10 warm-up hops, 30
//! measured, slowest < 90 % of the hop budget); the first that passes is used with ONNX Runtime
//! CPU as hot standby (see [`crate::engine::FallbackEngine`]). If none passes, ONNX Runtime CPU
//! runs and the reason is reported in the status.

use std::path::{Path, PathBuf};

use realtime_noise_model::{
    ArchiveGraphs, BackendDescriptor, CpuProfile, PdfNet3DevArchive, pdfnet3_bundle,
};
use realtime_noise_runtime_openvino::{DeviceKind, OpenVinoCore, fallback_order};

use crate::core::ModelKind;
use crate::engine::{Engine, FallbackEngine, LatencyPolicy, qualify};
use crate::legacy::{V2OpenVinoEngine, V2OrtEngine};
use crate::v3::{
    ENV_DEV_V3_ASSET, ENV_DEV_V3_SHA256, V3_BUNDLED_FILE, V3_BUNDLED_SHA256, V3Asset,
    v3_openvino_engine, v3_ort_engine,
};

/// Development override of the v2 stateful archive (unchanged from earlier releases).
pub const ENV_DEV_STATEFUL_ASSET: &str = "CLEARCORE_DEV_PDFNET3_STATEFUL_ASSET";
pub const ENV_DEV_STATEFUL_SHA256: &str = "CLEARCORE_DEV_PDFNET3_STATEFUL_SHA256";
/// Runtime override (same values as the `backend` setting).
pub const ENV_FILTER_BACKEND: &str = "CLEARCORE_FILTER_BACKEND";

/// A verified model ready to be compiled.
pub enum ModelSource {
    V3(V3Asset),
    V2 {
        graphs: ArchiveGraphs,
        sha256: String,
        asset_id: &'static str,
    },
}

impl ModelSource {
    #[must_use]
    pub const fn kind(&self) -> ModelKind {
        match self {
            Self::V3(_) => ModelKind::V3Full,
            Self::V2 { .. } => ModelKind::V2Film,
        }
    }

    fn descriptor(&self) -> BackendDescriptor {
        let (asset_id, sha) = match self {
            Self::V3(asset) => (
                crate::v3::V3_MODEL_ID.to_owned(),
                asset.archive_sha256.clone(),
            ),
            Self::V2 {
                sha256, asset_id, ..
            } => ((*asset_id).to_owned(), sha256.clone()),
        };
        BackendDescriptor {
            backend: "clearcore-filter",
            backend_version: env!("CARGO_PKG_VERSION"),
            runtime: "onnxruntime+openvino",
            runtime_version: "ort-2.0.0-rc.13",
            asset_id,
            asset_sha256: sha,
            cpu_profile: CpuProfile::Avx2Minimum.name(),
        }
    }
}

fn env_pair(asset: &str, sha: &str) -> Result<Option<(PathBuf, String)>, String> {
    let a = std::env::var_os(asset).filter(|v| !v.is_empty());
    let s = std::env::var_os(sha).filter(|v| !v.is_empty());
    match (a, s) {
        (None, None) => Ok(None),
        (Some(a), Some(s)) => {
            let path = PathBuf::from(a);
            let s = s.into_string().map_err(|_| format!("{sha} is not UTF-8"))?;
            if path.is_absolute()
                && s.len() == 64
                && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
            {
                Ok(Some((path, s)))
            } else {
                Err(format!(
                    "{asset}/{sha}: absolute path and 64 lowercase hex digits required"
                ))
            }
        }
        _ => Err(format!("{asset} and {sha} must be set together")),
    }
}

/// `<repo_root>/models`, `<repo_root>/resources/models`, then the standard model directories.
#[must_use]
pub fn model_dirs(repo_root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![repo_root.join("models"), repo_root.join("resources/models")];
    dirs.extend(pdfnet3_bundle::candidate_model_dirs());
    dirs
}

fn v2_source(archive: &PdfNet3DevArchive) -> Result<ModelSource, String> {
    Ok(ModelSource::V2 {
        graphs: archive.graphs().map_err(|e| format!("v2 archive: {e:?}"))?,
        sha256: archive.sha256().to_owned(),
        asset_id: archive.asset_id(),
    })
}

/// Resolves the model per the module rules.
pub fn resolve_model(repo_root: &Path) -> Result<ModelSource, String> {
    if let Some((path, sha)) = env_pair(ENV_DEV_V3_ASSET, ENV_DEV_V3_SHA256)? {
        return V3Asset::read(&path, &sha).map(ModelSource::V3);
    }
    if let Some((path, sha)) = env_pair(ENV_DEV_STATEFUL_ASSET, ENV_DEV_STATEFUL_SHA256)? {
        let archive =
            PdfNet3DevArchive::read(&path, &sha).map_err(|e| format!("v2 override: {e:?}"))?;
        return v2_source(&archive);
    }
    let dirs = model_dirs(repo_root);
    let path = pdfnet3_bundle::find_in(&dirs, V3_BUNDLED_FILE)
        .ok_or_else(|| format!("bundled v3 {V3_BUNDLED_FILE}: not found"))?;
    V3Asset::read(&path, V3_BUNDLED_SHA256)
        .map(ModelSource::V3)
        .map_err(|e| format!("bundled v3 {}: {e}", path.display()))
}

/// OpenVINO device kinds to try for a `backend` value, or `None` for ONNX Runtime.
#[must_use]
pub fn openvino_chain(backend: Option<&str>) -> Option<Vec<DeviceKind>> {
    let value = backend?.trim().to_ascii_lowercase();
    match value.as_str() {
        "openvino-npu" | "npu" | "intel-npu" | "openvino" => {
            Some(vec![DeviceKind::Npu, DeviceKind::Gpu, DeviceKind::Cpu])
        }
        "openvino-gpu" | "intel-gpu" | "arc" | "gpu" => {
            Some(vec![DeviceKind::Gpu, DeviceKind::Cpu])
        }
        "openvino-cpu" | "intel-cpu" => Some(vec![DeviceKind::Cpu]),
        _ => None,
    }
}

/// The `backend` request: `CLEARCORE_FILTER_BACKEND`, else `settings.json`.
#[must_use]
pub fn requested_backend(settings_backend: Option<&str>) -> Option<String> {
    std::env::var(ENV_FILTER_BACKEND)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| settings_backend.map(str::to_owned))
}

fn ort_engine(source: &ModelSource) -> Result<Box<dyn Engine>, String> {
    CpuProfile::Avx2Minimum
        .ensure_supported()
        .map_err(|e| format!("{e:?}"))?;
    Ok(match source {
        ModelSource::V3(asset) => Box::new(v3_ort_engine(asset)?),
        ModelSource::V2 { graphs, .. } => Box::new(V2OrtEngine::new(graphs)?),
    })
}

/// Compiles `source` for one OpenVINO device.
pub fn openvino_engine(source: &ModelSource, device: &str) -> Result<Box<dyn Engine>, String> {
    Ok(match source {
        ModelSource::V3(asset) => Box::new(v3_openvino_engine(asset, device)?),
        ModelSource::V2 { graphs, .. } => Box::new(V2OpenVinoEngine::new(graphs, device)?),
    })
}

/// Builds the engine for `source` and the requested backend (see the module docs).
pub fn build_engine(
    source: &ModelSource,
    backend: Option<&str>,
    policy: LatencyPolicy,
) -> Result<FallbackEngine, String> {
    let descriptor = source.descriptor();
    let ort = ort_engine(source)?;
    let Some(chain) = openvino_chain(backend) else {
        return Ok(FallbackEngine::new(descriptor, ort, None, policy));
    };
    let mut failures = Vec::new();
    match OpenVinoCore::new().and_then(|core| core.available_devices()) {
        Err(e) => failures.push(format!("OpenVINO runtime unavailable: {e}")),
        Ok(available) => {
            let ordered = fallback_order(&available);
            for kind in chain {
                for device in ordered
                    .iter()
                    .filter(|d| DeviceKind::from_device_name(d) == kind)
                {
                    match openvino_engine(source, device) {
                        Err(e) => failures.push(format!("{device}: {e}")),
                        Ok(mut engine) => match qualify(engine.as_mut(), policy.budget, 10, 30) {
                            Ok(_) => {
                                return Ok(FallbackEngine::new(
                                    descriptor,
                                    engine,
                                    Some(ort),
                                    policy,
                                ));
                            }
                            Err((e, _)) => failures.push(format!("{device}: {e}")),
                        },
                    }
                }
            }
            if failures.is_empty() {
                failures.push(format!(
                    "no requested OpenVINO device present (lists: {})",
                    available.join(", ")
                ));
            }
        }
    }
    let mut engine = FallbackEngine::new(descriptor, ort, None, policy);
    engine.note_selection_fallback(failures.join("; "));
    Ok(engine)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_values_map_to_the_documented_device_chains() {
        use DeviceKind::{Cpu, Gpu, Npu};
        assert_eq!(
            openvino_chain(Some("openvino-npu")),
            Some(vec![Npu, Gpu, Cpu])
        );
        assert_eq!(openvino_chain(Some("OpenVINO-GPU")), Some(vec![Gpu, Cpu]));
        assert_eq!(openvino_chain(Some("openvino-cpu")), Some(vec![Cpu]));
        assert_eq!(openvino_chain(Some("openvino")), Some(vec![Npu, Gpu, Cpu]));
        for ort in [
            None,
            Some("auto"),
            Some("tract"),
            Some("cpu"),
            Some("tensorrt"),
        ] {
            assert_eq!(openvino_chain(ort), None, "{ort:?}");
        }
    }
}
