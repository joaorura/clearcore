#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use realtime_noise_accelerators::{BackendSelection, OpenVINOBackend, TensorRtBackend};
use realtime_noise_contracts::HOP_SAMPLES;
use realtime_noise_model::{
    APPROVED_ASSET_SHA256, ApprovedAssetManifest, CpuProfile, InferenceBackend, TractBackend,
};

/// Maximum allowable runtime processing latency (in ms) per 480-sample hop (10.0 ms deadline)
/// for a candidate backend to be selected in `Auto` mode. Leaves margin for PipeWire/WaveRT I/O.
pub const AUTO_QUALIFICATION_MAX_HOP_MS: f32 = if cfg!(debug_assertions) {
    35.0
} else {
    9.0
};

/// Qualifies an inference backend in runtime by running a micro-warmup and latency benchmark.
///
/// Executes 2 warmup frames (to trigger JIT compilation, driver queue initialization, and buffer allocations)
/// followed by benchmark frames of synthetic audio (silence/finite samples).
/// Measures the processing time per frame and verifies real-time compliance within `max_hop_ms`.
pub fn qualify_backend_runtime(backend: &mut dyn InferenceBackend, max_hop_ms: f32) -> bool {
    let dummy_frame = [0.0f32; HOP_SAMPLES];

    // Warmup frames: JIT compilation, driver command queue warm up, and buffer allocations
    for w in 0..5 {
        if let Err(err) = backend.process(&dummy_frame) {
            eprintln!("[supervisor] Backend warmup frame {w} failed: {err}");
            return false;
        }
    }

    // Benchmark frames: Measure runtime inference latency under steady state
    let benchmark_runs = 3;
    let mut total_duration_ms = 0.0f32;

    for i in 0..benchmark_runs {
        let start = Instant::now();
        match backend.process(&dummy_frame) {
            Ok(output) => {
                let elapsed = start.elapsed().as_secs_f32() * 1000.0;
                total_duration_ms += elapsed;

                // Ensure output is finite
                if !output.samples.iter().all(|s| s.is_finite()) {
                    eprintln!("[supervisor] Backend qualification frame produced non-finite output");
                    return false;
                }

                if elapsed > max_hop_ms {
                    eprintln!(
                        "[supervisor] Backend runtime benchmark frame {i} exceeded deadline: {elapsed:.3} ms > {max_hop_ms:.3} ms"
                    );
                    return false;
                }
            }
            Err(err) => {
                eprintln!("[supervisor] Backend runtime benchmark frame {i} failed: {err}");
                return false;
            }
        }
    }

    let avg_ms = total_duration_ms / (benchmark_runs as f32);
    eprintln!(
        "[supervisor] Backend runtime qualified successfully: avg {avg_ms:.3} ms (threshold <= {max_hop_ms:.3} ms)"
    );
    true
}

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

fn has_stateful_models(p: &Path) -> bool {
    p.join("enc.onnx").exists()
        || p.join("tensorrt/enc.engine").exists()
        || p.join("enc.engine").exists()
}

/// Attempts to locate the directory containing stateful model graphs (`models/stateful/enc.onnx` or TensorRT engines).
#[must_use]
pub fn find_stateful_model_dir() -> Option<PathBuf> {
    if let Ok(path) =
        std::env::var("CLEARCORE_STATEFUL_DIR").or_else(|_| std::env::var("CLEARCORE_MODEL_DIR"))
    {
        let p = PathBuf::from(path);
        if has_stateful_models(&p) {
            return Some(p);
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        let mut cur = Some(cwd.as_path());
        for _ in 0..5 {
            if let Some(p) = cur {
                let candidate = p.join("models/stateful");
                if has_stateful_models(&candidate) {
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
                if has_stateful_models(&candidate) {
                    return Some(candidate);
                }
                let candidate_res = p.join("resources/models/stateful");
                if has_stateful_models(&candidate_res) {
                    return Some(candidate_res);
                }
                cur = p.parent();
            }
        }
    }

    if let Ok(data_home) = std::env::var("XDG_DATA_HOME") {
        let p = PathBuf::from(data_home).join("clearcore/models/stateful");
        if has_stateful_models(&p) {
            return Some(p);
        }
    }

    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(&home).join(".local/share/clearcore/models/stateful");
        if has_stateful_models(&p) {
            return Some(p);
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
        "openvino" => Some(OpenVinoTarget::Auto),
        "openvino-npu" | "npu" | "intel-npu" => Some(OpenVinoTarget::Device("NPU".to_string())),
        "openvino-gpu" | "intel-gpu" | "arc" => {
            Some(OpenVinoTarget::Device("GPU".to_string()))
        }
        "openvino-cpu" | "intel-cpu" => Some(OpenVinoTarget::Device("CPU".to_string())),
        _ => None,
    }
}

/// Instantiates an inference backend based on `request`, falling back safely to Tract CPU.
///
/// If `request` resolves to `TensorRT` or `OpenVINO` and hardware/models are present,
/// compiles and instantiates the respective hardware backend. If hardware or models are missing,
/// or if Tract CPU is explicitly requested, instantiates `TractBackend` (or pure-Rust Tract baseline).
/// Returns whether the backend request explicitly targets a hardware accelerator.
#[must_use]
pub fn is_explicit_accelerator_request(request: &str) -> bool {
    let norm = request.trim().to_ascii_lowercase().replace('_', "-");
    matches!(
        norm.as_str(),
        "tensorrt"
            | "nvidia-tensorrt"
            | "nvidia"
            | "cuda"
            | "gpu"
            | "openvino"
            | "openvino-npu"
            | "npu"
            | "intel-npu"
            | "openvino-gpu"
            | "intel-gpu"
            | "arc"
            | "directml"
            | "vulkan"
            | "ryzenai"
            | "coreml"
    )
}

fn instantiate_tensorrt_backend(
    model_dir: Option<&Path>,
) -> Result<TensorRtBackend, String> {
    if !TensorRtBackend::is_available() {
        return Err("TensorRT native runtime (libnvinfer) is not available on this host".to_string());
    }

    if let Some(dir) = model_dir {
        match TensorRtBackend::load_stateful(
            dir,
            0,
            "df-compatible-release-asset-v1",
            APPROVED_ASSET_SHA256,
        ) {
            Ok(backend) => return Ok(backend),
            Err(load_err) => {
                eprintln!(
                    "TensorRT load_stateful failed ({load_err}); falling back to hardware context"
                );
            }
        }
    }

    TensorRtBackend::try_new_hardware(
        "df-compatible-release-asset-v1",
        APPROVED_ASSET_SHA256,
        0,
    )
    .map_err(|e| format!("TensorRT hardware initialization failed: {e}"))
}

/// Instantiates an inference backend based on `request`, falling back safely to Tract CPU.
///
/// If `request` resolves to `TensorRT` or `OpenVINO` and hardware/models are present,
/// compiles and instantiates the respective hardware backend. If hardware or models are missing,
/// or if Tract CPU is explicitly requested, instantiates `TractBackend` (or pure-Rust Tract baseline).
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
    let norm = trimmed.to_ascii_lowercase().replace('_', "-");
    let is_tract_request = matches!(
        norm.as_str(),
        "tract" | "cpu-tract" | "tract-cpu" | "cpu"
    );

    let mut fallback_reason = None;

    if !is_tract_request {
        if matches!(norm.as_str(), "auto" | "" | "default") {
            // Auto order: NPU -> iGPU -> dGPU -> CPU fallback (prioritizing minimal power & zero CPU load)
            // 1. Try NPU first via OpenVINO (minimal power, dedicated neural accelerator)
            if OpenVINOBackend::is_available() {
                if let Some(ref dir) = resolved_model_dir {
                    match OpenVINOBackend::load_stateful(
                        dir,
                        "NPU",
                        "df-compatible-release-asset-v1",
                        APPROVED_ASSET_SHA256,
                    ) {
                        Ok(mut backend) => {
                            if qualify_backend_runtime(&mut backend, AUTO_QUALIFICATION_MAX_HOP_MS) {
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
                            fallback_reason = Some(format!(
                                "OpenVINO NPU exceeded real-time deadline of {AUTO_QUALIFICATION_MAX_HOP_MS:.1}ms in runtime qualification"
                            ));
                        }
                        Err(err) => {
                            fallback_reason =
                                Some(format!("OpenVINO NPU auto initialization skipped/failed: {err}"));
                        }
                    }
                }
            }

            // 2. Try iGPU second via OpenVINO GPU (integrated GPU, low power, offloads CPU)
            if OpenVINOBackend::is_available() {
                if let Some(ref dir) = resolved_model_dir {
                    match OpenVINOBackend::load_stateful(
                        dir,
                        "GPU",
                        "df-compatible-release-asset-v1",
                        APPROVED_ASSET_SHA256,
                    ) {
                        Ok(mut backend) => {
                            if qualify_backend_runtime(&mut backend, AUTO_QUALIFICATION_MAX_HOP_MS) {
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
                            fallback_reason = Some(format!(
                                "OpenVINO GPU exceeded real-time deadline of {AUTO_QUALIFICATION_MAX_HOP_MS:.1}ms in runtime qualification"
                            ));
                        }
                        Err(err) => {
                            fallback_reason =
                                Some(format!("OpenVINO GPU auto initialization skipped/failed: {err}"));
                        }
                    }
                }
            }

            // 3. Try dGPU third via TensorRT (dedicated NVIDIA GPU)
            if TensorRtBackend::is_available() {
                match instantiate_tensorrt_backend(resolved_model_dir.as_deref()) {
                    Ok(mut backend) if backend.executes_engine() => {
                        if qualify_backend_runtime(&mut backend, AUTO_QUALIFICATION_MAX_HOP_MS) {
                            let device = backend.device_name().to_string();
                            let descriptor = backend.descriptor();
                            let info = BackendResolutionInfo {
                                name: "nvidia-tensorrt".to_string(),
                                runtime: descriptor.runtime.to_string(),
                                device,
                                is_hardware_accelerated: true,
                                is_fallback: false,
                                fallback_reason: None,
                            };
                            return (Box::new(backend), info);
                        }
                        fallback_reason = Some(format!(
                            "TensorRT exceeded real-time deadline of {AUTO_QUALIFICATION_MAX_HOP_MS:.1}ms in runtime qualification"
                        ));
                    }
                    Ok(_) => {
                        fallback_reason = Some(
                            "TensorRT GPU detectada, mas os engines do modelo ainda não foram compilados localmente".to_string(),
                        );
                    }
                    Err(err) => {
                        fallback_reason =
                            Some(format!("TensorRT auto initialization failed: {err}"));
                    }
                }
            }

            // 4. Try any remaining OpenVINO accelerator (or generic fallback)
            if OpenVINOBackend::is_available() {
                if let Some(ref dir) = resolved_model_dir {
                    let devices = OpenVINOBackend::available_devices();
                    // Try devices that are accelerators (NPU, GPU)
                    let accel_devices: Vec<String> = devices
                        .into_iter()
                        .filter(|d| !d.eq_ignore_ascii_case("CPU"))
                        .collect();
                    if !accel_devices.is_empty() {
                        if let Ok(mut backend) = OpenVINOBackend::load_stateful_with_fallback(
                            dir,
                            &accel_devices,
                            "df-compatible-release-asset-v1",
                            APPROVED_ASSET_SHA256,
                        ) {
                            if qualify_backend_runtime(&mut backend, AUTO_QUALIFICATION_MAX_HOP_MS) {
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
                            fallback_reason = Some(format!(
                                "OpenVINO accelerator exceeded real-time deadline of {AUTO_QUALIFICATION_MAX_HOP_MS:.1}ms in runtime qualification"
                            ));
                        }
                    }
                } else if fallback_reason.is_none() {
                    fallback_reason = Some(
                        "models/stateful directory not found for OpenVINO stateful backend"
                            .to_string(),
                    );
                }
            } else if fallback_reason.is_none() {
                fallback_reason = Some("No hardware accelerator runtime available".to_string());
            }
            // 5. If all hardware accelerators fail, execution falls through to CPU (Tract CPU) as final fallback!
        } else if matches!(norm.as_str(), "tensorrt" | "nvidia-tensorrt" | "nvidia" | "cuda") {
            // Explicit TensorRT request
            if TensorRtBackend::is_available() {
                match instantiate_tensorrt_backend(resolved_model_dir.as_deref()) {
                    Ok(backend) => {
                        let device = backend.device_name().to_string();
                        let descriptor = backend.descriptor();
                        let info = BackendResolutionInfo {
                            name: "nvidia-tensorrt".to_string(),
                            runtime: descriptor.runtime.to_string(),
                            device,
                            is_hardware_accelerated: true,
                            is_fallback: false,
                            fallback_reason: None,
                        };
                        return (Box::new(backend), info);
                    }
                    Err(err) => {
                        fallback_reason =
                            Some(format!("TensorRT initialization failed: {err}"));
                    }
                }
            } else {
                fallback_reason = Some(
                    "TensorRT native runtime (libnvinfer) is not available on this host".to_string(),
                );
            }
        } else if norm == "gpu" {
            // Generic GPU request: try TensorRT first, then OpenVINO GPU
            if TensorRtBackend::is_available() {
                match instantiate_tensorrt_backend(resolved_model_dir.as_deref()) {
                    Ok(backend) => {
                        let device = backend.device_name().to_string();
                        let descriptor = backend.descriptor();
                        let info = BackendResolutionInfo {
                            name: "nvidia-tensorrt".to_string(),
                            runtime: descriptor.runtime.to_string(),
                            device,
                            is_hardware_accelerated: true,
                            is_fallback: false,
                            fallback_reason: None,
                        };
                        return (Box::new(backend), info);
                    }
                    Err(err) => {
                        fallback_reason =
                            Some(format!("TensorRT GPU initialization failed: {err}"));
                    }
                }
            }

            if OpenVINOBackend::is_available() {
                if let Some(ref dir) = resolved_model_dir {
                    match OpenVINOBackend::load_stateful(
                        dir,
                        "GPU",
                        "df-compatible-release-asset-v1",
                        APPROVED_ASSET_SHA256,
                    ) {
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
                                Some(format!("OpenVINO GPU initialization failed: {err}"));
                        }
                    }
                } else if fallback_reason.is_none() {
                    fallback_reason = Some(
                        "models/stateful directory not found for OpenVINO GPU backend".to_string(),
                    );
                }
            } else if fallback_reason.is_none() {
                fallback_reason = Some("No GPU acceleration runtime available".to_string());
            }
        } else if let Some(target) = parse_openvino_target(trimmed) {
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
