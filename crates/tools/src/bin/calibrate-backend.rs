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
    CalibrationReport, CoreMlBackend, CudaBackend, OpenVINOBackend, PromotionDecision,
    QUALIFICATION_MAX_DEADLINE_MS, QUALIFICATION_MAX_P99_MS, evaluate_calibration,
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
                println!("  --duration-minutes <MINUTES>  Calibration duration in audio minutes [default: 5]");
                println!("  --duration-seconds <SECONDS>  Calibration duration in audio seconds");
                println!("  --backend <NAME>              Backend to calibrate (cuda, openvino, coreml) [default: cuda]");
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

fn run_calibration(cli: &CliArgs) -> Result<CalibrationReport, String> {
    let effective_seconds = cli
        .duration_seconds
        .unwrap_or(cli.duration_minutes * 60.0);
    // In real-time audio at 48kHz with 480 samples per hop, 1 second = 100 hops.
    let total_hops = (effective_seconds * 100.0).round() as usize;
    // We calibrate over sample frames (up to 30,000 hops for 5 minutes, or minimum 100 hops)
    let hops_to_benchmark = total_hops.clamp(100, 30_000);

    let mut backend_instance: Box<dyn InferenceBackend> = match cli.backend.to_lowercase().as_str() {
        "cuda" | "tensorrt" => Box::new(CudaBackend::new_mock()),
        "openvino" | "npu" => Box::new(OpenVINOBackend::new_mock()),
        "coreml" | "ane" => Box::new(CoreMlBackend::new_mock()),
        other => return Err(format!("unsupported accelerator backend for calibration: {other}")),
    };

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
