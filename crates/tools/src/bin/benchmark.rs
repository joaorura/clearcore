use std::{
    env, fs,
    process::ExitCode,
    time::{SystemTime, UNIX_EPOCH},
};

use realtime_noise_model::{ApprovedAssetManifest, GoldenFixture};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn main() -> ExitCode {
    match run() {
        Ok("M1_APPROVED") => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(2),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<&'static str, String> {
    validate_arguments()?;
    let root = env::current_dir().map_err(|error| error.to_string())?;
    let manifest = ApprovedAssetManifest::verify(&root).map_err(|error| error.to_string())?;
    let golden_path = root.join("fixtures/golden/frozen-reference.json");
    let golden_result = GoldenFixture::read(&golden_path);
    let status = if golden_result.is_err() {
        "BLOCKED_PENDING_GOLDEN"
    } else {
        "BLOCKED_UNSUPPORTED_CPU_PROFILE"
    };
    let report = json!({
        "schema_version": 1,
        "status": status,
        "blockers": [
            "BLOCKED_PENDING_GOLDEN",
            "BLOCKED_UNSUPPORTED_CPU_PROFILE",
            "BLOCKED_ALLOCATION_MEASUREMENT"
        ],
        "run_id": format!("task4-blocked-{}", unix_seconds()?),
        "backend": "tract",
        "profile": "avx2-minimum",
        "requested_duration_seconds": 300,
        "measured_duration_seconds": Value::Null,
        "worker_inference_only": true,
        "asset_id": manifest.asset_id(),
        "asset_sha256": manifest.asset_sha256(),
        "golden_available": golden_result.is_ok(),
        "physical_host_qualification": "not_observable_in_container",
        "allocation_measurement": "unavailable",
        "p99_ms": Value::Null,
        "deadline_ms": 10,
        "p99_limit_ms": 7,
        "deadline_misses": Value::Null
    });
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("benchmarks")).map_err(|error| error.to_string())?;
    fs::write(root.join("benchmarks/cpu-baseline.json"), &bytes)
        .map_err(|error| error.to_string())?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let markdown = format!(
        "# CPU Baseline\n\nStatus: `{status}`.\n\nCanonical report: `benchmarks/cpu-baseline.json` (SHA-256 `{digest}`).\n\nThis blocked receipt records inference-worker scope only. It does not claim product end-to-end latency or M1 approval. The frozen golden, physical host qualification, and per-hop allocation instrumentation are unavailable.\n"
    );
    fs::write(root.join("benchmarks/cpu-baseline.md"), markdown)
        .map_err(|error| error.to_string())?;
    println!(
        "{}",
        json!({"status": status, "report": "benchmarks/cpu-baseline.json"})
    );
    Ok(status)
}

fn validate_arguments() -> Result<(), String> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    let expected = [
        "--backend",
        "tract",
        "--profile",
        "avx2-minimum",
        "--duration",
        "300",
    ];
    if arguments.iter().map(String::as_str).eq(expected) {
        Ok(())
    } else {
        Err("benchmark requires --backend tract --profile avx2-minimum --duration 300".to_owned())
    }
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| error.to_string())
}
