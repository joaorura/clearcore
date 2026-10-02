#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use realtime_noise_accelerators::{
    CalibrationReport, CoreMlBackend, DirectMlBackend, OpenVINOBackend, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, RyzenAiBackend, TensorRtBackend,
    VulkanBackend, evaluate_calibration,
};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::InferenceBackend;

struct CliArgs {
    duration_minutes: f64,
    duration_seconds: Option<f64>,
    backend: String,
    output_path: Option<PathBuf>,
    json_output: bool,
    force_promote: bool,
}

fn parse_args() -> Result<CliArgs, String> {
    let mut args = env::args().skip(1);
    let mut duration_minutes = 5.0;
    let mut duration_seconds = None;
    let mut backend = "cuda".to_owned();
    let mut output_path = None;
    let mut json_output = false;
    let mut force_promote = false;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--duration-minutes" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--duration-minutes requires a value".to_owned())?;
                duration_minutes = val
                    .parse::<f64>()
                    .map_err(|e| format!("invalid duration-minutes: {e}"))?;
            }
            "--duration-seconds" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--duration-seconds requires a value".to_owned())?;
                let secs = val
                    .parse::<f64>()
                    .map_err(|e| format!("invalid duration-seconds: {e}"))?;
                duration_seconds = Some(secs);
            }
            "--backend" => {
                backend = args
                    .next()
                    .ok_or_else(|| "--backend requires a value".to_owned())?;
            }
            "--output" => {
                let val = args
                    .next()
                    .ok_or_else(|| "--output requires a path".to_owned())?;
                output_path = Some(PathBuf::from(val));
            }
            "--json" => {
                json_output = true;
            }
            "--force-promote" => {
                force_promote = true;
            }
            "-h" | "--help" => {
                println!("Usage: calibrate-backend [OPTIONS]");
                println!();
                println!("Options:");
                println!(
                    "  --duration-minutes <MINUTES>  Calibration duration in audio minutes [default: 5]"
                );
                println!("  --duration-seconds <SECONDS>  Calibration duration in audio seconds");
                println!(
                    "  --backend <NAME>              Backend to calibrate (cuda, openvino, coreml) [default: cuda]"
                );
                println!("  --output <PATH>               Path to save JSON calibration report");
                println!("  --json                        Print report as JSON to stdout");
                println!("  --force-promote               Override staging gate for testing");
                println!("  -h, --help                    Print help");
                std::process::exit(0);
            }
            other => {
                return Err(format!("unknown argument: {other}"));
            }
        }
    }

    Ok(CliArgs {
        duration_minutes,
        duration_seconds,
        backend,
        output_path,
        json_output,
        force_promote,
    })
}

fn calculate_percentile(sorted_samples: &[f64], pct: f64) -> f64 {
    if sorted_samples.is_empty() {
        return 0.0;
    }
    let idx = ((pct / 100.0) * (sorted_samples.len() as f64 - 1.0)).round() as usize;
    sorted_samples[idx.min(sorted_samples.len() - 1)]
}

/// Marker prefixed to `reason` when the candidate only copies samples (no neural network runs).
const PASSTHROUGH_MARKER: &str = "PASSTHROUGH";

/// A calibration candidate and whether it runs real inference.
struct Candidate {
    backend: Box<dyn InferenceBackend>,
    executes_inference: bool,
}

/// Builds the candidate for `name`.
///
/// Every candidate built here is a mock or a stub (no model assets are loaded), and the
/// accelerator ones copy input to output, so their latency says nothing about a real network.
/// The flag comes from the backend itself, not from a list kept here, so it turns true for a
/// backend only when that backend starts executing inference.
fn build_candidate(name: &str) -> Result<Candidate, String> {
    macro_rules! candidate {
        ($backend:expr) => {{
            let backend = $backend;
            let executes_inference = backend.executes_inference();
            Candidate {
                backend: Box::new(backend),
                executes_inference,
            }
        }};
    }
    Ok(match name.to_lowercase().as_str() {
        "cuda" | "tensorrt" => candidate!(TensorRtBackend::new_mock()),
        "directml" | "dx12" => candidate!(DirectMlBackend::new_mock()),
        "vulkan" => candidate!(VulkanBackend::new_mock()),
        "ryzenai" | "ryzen-ai" | "vitisai" | "xdna" | "amd-npu" => {
            candidate!(RyzenAiBackend::new_mock_npu())
        }
        "ryzenai-gpu" | "ryzen-ai-gpu" | "amd-igpu" => candidate!(RyzenAiBackend::new_mock_gpu()),
        "openvino" | "npu" | "openvino-npu" => candidate!(OpenVINOBackend::new_mock_npu()),
        "openvino-gpu" | "gpu" => candidate!(OpenVINOBackend::new_mock_gpu()),
        // "cpu" is an alias kept for compatibility; the stub it measures is OpenVINO's.
        "openvino-cpu" | "cpu" => candidate!(OpenVINOBackend::new_mock_cpu()),
        "coreml" | "ane" => candidate!(CoreMlBackend::new_mock()),
        other => {
            return Err(format!(
                "unsupported accelerator backend for calibration: {other}"
            ));
        }
    })
}

/// Forces a passthrough candidate to `NotPromoted` and says why, keeping any gate failures that
/// were already recorded. Not overridable by `--force-promote`: that flag bypasses the staging
/// gate, and a candidate that does not denoise has nothing to be staged.
fn reject_passthrough(report: &mut CalibrationReport) {
    let note = format!(
        "{PASSTHROUGH_MARKER}: candidate only copies input to output (no neural network executed); \
         its latency is not evidence for AUTO and it is never promoted"
    );
    report.decision = PromotionDecision::NotPromoted;
    report.reason = Some(match report.reason.take() {
        Some(existing) => format!("{note}; {existing}"),
        None => note,
    });
}

fn run_calibration(cli: &CliArgs) -> Result<CalibrationReport, String> {
    let effective_seconds = cli.duration_seconds.unwrap_or(cli.duration_minutes * 60.0);
    // In real-time audio at 48kHz with 480 samples per hop, 1 second = 100 hops.
    let total_hops = (effective_seconds * 100.0).round() as usize;
    // We calibrate over sample frames (up to 30,000 hops for 5 minutes, or minimum 100 hops)
    let hops_to_benchmark = total_hops.clamp(100, 30_000);

    let Candidate {
        backend: mut backend_instance,
        executes_inference,
    } = build_candidate(&cli.backend)?;
    if !executes_inference {
        eprintln!(
            "warning: backend '{}' is a passthrough here (no neural network runs); it will not be promoted",
            cli.backend
        );
    }

    let sample_frame: AudioFrame = [0.0; HOP_SAMPLES];
    let mut latencies_ms = Vec::with_capacity(hops_to_benchmark);
    let mut deadline_misses = 0usize;

    // Warm-up iteration
    let _ = backend_instance
        .process(&sample_frame)
        .map_err(|e| format!("warm-up failed: {e}"))?;

    for _ in 0..hops_to_benchmark {
        let t0 = Instant::now();
        let _ = backend_instance
            .process(&sample_frame)
            .map_err(|e| format!("inference hop failed: {e}"))?;
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1_000.0;
        if elapsed_ms > QUALIFICATION_MAX_DEADLINE_MS {
            deadline_misses += 1;
        }
        latencies_ms.push(elapsed_ms);
    }

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let p50 = calculate_percentile(&latencies_ms, 50.0);
    let p95 = calculate_percentile(&latencies_ms, 95.0);
    let p99 = calculate_percentile(&latencies_ms, 99.0);
    let max = *latencies_ms.last().unwrap_or(&0.0);

    let mut report = evaluate_calibration(
        &cli.backend,
        effective_seconds,
        total_hops,
        p50,
        p95,
        p99,
        max,
        deadline_misses,
        0, // Zero discontinuities
    );

    // Hardware Staging Gate Check:
    // On the current host platform, NVIDIA Blackwell (sm_120) and Intel NPU are staging backends.
    // In compliance with Project Hippocamp GA criteria, all non-CPU backends are recorded as
    // NOT_PROMOTED / GATED_STAGING for GA release, ensuring safe fallback to the qualified
    // tract CPU baseline.
    if !cli.force_promote {
        match cli.backend.to_lowercase().as_str() {
            "cuda" | "tensorrt" => {
                report.decision = PromotionDecision::NotPromoted;
                report.reason = Some(
                    "GATED_STAGING: NVIDIA Blackwell (sm_120) requires proprietary external driver/runtime; CPU baseline remains normative for GA".to_owned(),
                );
            }
            "openvino" | "npu" => {
                report.decision = PromotionDecision::NotPromoted;
                report.reason = Some(
                    "GATED_STAGING: Intel NPU requires external OneAPI/Level-Zero driver stack; CPU baseline remains normative for GA".to_owned(),
                );
            }
            "coreml" | "ane" => {
                report.decision = PromotionDecision::NotPromoted;
                report.reason = Some(
                    "GATED_STAGING: CoreML backend is in staging qualification; CPU baseline remains normative for GA".to_owned(),
                );
            }
            _ => {}
        }
    }

    // Outside the `force_promote` check above on purpose: passthrough is never promoted.
    if !executes_inference {
        reject_passthrough(&mut report);
    }

    Ok(report)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("Error: {err}");
            return ExitCode::from(1);
        }
    };

    match run_calibration(&args) {
        Ok(report) => {
            if let Some(ref path) = args.output_path {
                let serialized = match serde_json::to_string_pretty(&report) {
                    Ok(json) => json,
                    Err(err) => {
                        eprintln!("Failed to serialize report: {err}");
                        return ExitCode::from(1);
                    }
                };
                if let Err(err) = fs::write(path, serialized) {
                    eprintln!("Failed to write report to {}: {err}", path.display());
                    return ExitCode::from(1);
                }
            }

            if args.json_output {
                let json = serde_json::to_string_pretty(&report).unwrap_or_default();
                println!("{json}");
            } else {
                println!("====================================================");
                println!("Hippocamp Accelerator Calibration Report");
                println!("====================================================");
                println!("Backend:            {}", report.backend_name);
                println!(
                    "Calibration Time:   {:.1}s ({:.1} audio minutes)",
                    report.duration_seconds,
                    report.duration_seconds / 60.0
                );
                println!("Frames Evaluated:   {}", report.total_frames);
                println!("p50 Latency:        {:.3} ms", report.p50_latency_ms);
                println!("p95 Latency:        {:.3} ms", report.p95_latency_ms);
                println!("p99 Latency:        {:.3} ms", report.p99_latency_ms);
                println!("Max Latency:        {:.3} ms", report.max_latency_ms);
                println!("Deadline Misses:    {}", report.deadline_miss_count);
                println!("Discontinuities:    {}", report.discontinuities);
                println!(
                    "p99 Gate:           <= {:.1} ms [{}]",
                    QUALIFICATION_MAX_P99_MS,
                    if report.p99_latency_ms <= QUALIFICATION_MAX_P99_MS {
                        "PASS"
                    } else {
                        "FAIL"
                    }
                );
                println!(
                    "Decision:           {}",
                    match report.decision {
                        PromotionDecision::Promoted => "PROMOTED",
                        PromotionDecision::NotPromoted => "NOT_PROMOTED",
                    }
                );
                println!(
                    "Inference:          {}",
                    if report
                        .reason
                        .as_deref()
                        .is_some_and(|r| r.starts_with(PASSTHROUGH_MARKER))
                    {
                        "PASSTHROUGH (copies input to output; no neural network)"
                    } else {
                        "executed"
                    }
                );
                if let Some(ref reason) = report.reason {
                    println!("Reason:             {reason}");
                }
                println!("====================================================");
            }
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("Calibration failed: {err}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn cli(backend: &str, force_promote: bool) -> CliArgs {
        CliArgs {
            duration_minutes: 0.0,
            duration_seconds: Some(1.0),
            backend: backend.to_owned(),
            output_path: None,
            json_output: false,
            force_promote,
        }
    }

    #[test]
    fn every_mock_accelerator_candidate_is_a_passthrough() {
        for name in [
            "tensorrt",
            "cuda",
            "directml",
            "dx12",
            "vulkan",
            "ryzenai",
            "ryzenai-gpu",
            "openvino",
            "npu",
            "openvino-gpu",
            "gpu",
            "openvino-cpu",
            "cpu",
            "coreml",
            "ane",
        ] {
            let candidate = build_candidate(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                !candidate.executes_inference,
                "{name} is a mock and cannot be reported as running inference"
            );
        }
        assert!(build_candidate("no-such-backend").is_err());
    }

    #[test]
    fn passthrough_candidates_are_never_promoted_even_when_forced() {
        for name in ["vulkan", "directml", "ryzenai", "openvino-cpu", "tensorrt"] {
            for force in [false, true] {
                let report = run_calibration(&cli(name, force)).unwrap();
                assert_eq!(
                    report.decision,
                    PromotionDecision::NotPromoted,
                    "{name} {force}"
                );
                assert!(!report.is_promoted());
                let reason = report.reason.unwrap();
                assert!(reason.starts_with(PASSTHROUGH_MARKER), "{name}: {reason}");
            }
        }
    }

    #[test]
    fn rejection_keeps_the_earlier_gate_reason() {
        let mut report = evaluate_calibration("vulkan", 1.0, 100, 1.0, 1.0, 20.0, 20.0, 1, 0);
        let earlier = report.reason.clone().unwrap();
        reject_passthrough(&mut report);
        let reason = report.reason.unwrap();
        assert!(reason.starts_with(PASSTHROUGH_MARKER));
        assert!(reason.ends_with(&earlier));
    }
}
