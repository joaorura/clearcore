use std::{
    env, fs,
    path::Path,
    process::{Command, ExitCode},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use realtime_noise_model::{
    ApprovedAssetManifest, CpuProfile, GoldenFixture, InferenceBackend, TractBackend,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const REQUESTED_SECONDS: u64 = 300;
const DEADLINE_MS: f64 = 10.0;
const P99_LIMIT_MS: f64 = 7.0;

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
    let started = unix_seconds()?;
    let manifest = ApprovedAssetManifest::verify(&root).map_err(|error| error.to_string())?;
    let host = host_evidence();
    let golden = match GoldenFixture::read(&root.join("fixtures/golden/frozen-reference.json")) {
        Ok(golden) => golden,
        Err(error) => {
            return write_report(
                &root,
                "BLOCKED_PENDING_GOLDEN",
                &manifest,
                &host,
                started,
                &json!({"reason": error.to_string()}),
            );
        }
    };
    if !host.qualifies {
        return write_report(
            &root,
            "BLOCKED_UNSUPPORTED_CPU_PROFILE",
            &manifest,
            &host,
            started,
            &json!({"golden_provenance_sha256": golden.provenance_sha256}),
        );
    }

    let mut backend =
        TractBackend::new(&manifest, CpuProfile::Avx2Minimum).map_err(|error| error.to_string())?;
    golden
        .validate_for(&backend.descriptor())
        .map_err(|error| error.to_string())?;
    for case in &golden.cases {
        for frame in &case.input_frames {
            let input = frame_to_array(frame)?;
            backend.process(&input).map_err(|error| error.to_string())?;
        }
    }

    let measurement_start = Instant::now();
    let mut elapsed_ms = Vec::new();
    while measurement_start.elapsed() < Duration::from_secs(REQUESTED_SECONDS) {
        for case in &golden.cases {
            for frame in &case.input_frames {
                let input = frame_to_array(frame)?;
                let started_hop = Instant::now();
                backend.process(&input).map_err(|error| error.to_string())?;
                elapsed_ms.push(started_hop.elapsed().as_secs_f64() * 1_000.0);
            }
        }
    }
    elapsed_ms.sort_by(f64::total_cmp);
    let measured = measurement_start.elapsed().as_secs_f64();
    let deadline_misses = elapsed_ms
        .iter()
        .filter(|&&elapsed| elapsed > DEADLINE_MS)
        .count();
    let metrics = json!({
        "golden_provenance_sha256": golden.provenance_sha256,
        "measured_duration_seconds": measured,
        "count": elapsed_ms.len(),
        "p50_ms": percentile(&elapsed_ms, 50),
        "p95_ms": percentile(&elapsed_ms, 95),
        "p99_ms": percentile(&elapsed_ms, 99),
        "max_ms": elapsed_ms.last(),
        "deadline_misses": deadline_misses,
        "allocation": {
            "mechanism": "unavailable-under-unsafe-code-forbid",
            "available": false,
            "per_hop_count": Value::Null,
            "per_hop_bytes": Value::Null
        }
    });
    write_report(
        &root,
        "BLOCKED_ALLOCATION_MEASUREMENT",
        &manifest,
        &host,
        started,
        &metrics,
    )
}

struct HostEvidence {
    os: String,
    kernel: String,
    cpu_model: String,
    avx2: bool,
    physical_cores: Option<usize>,
    ram_kib: Option<u64>,
    container: bool,
    qualifies: bool,
}

fn host_evidence() -> HostEvidence {
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let model = cpuinfo
        .lines()
        .find_map(|line| line.strip_prefix("model name\t: "))
        .unwrap_or("unavailable")
        .to_owned();
    let cores = cpuinfo
        .lines()
        .filter_map(|line| line.strip_prefix("core id\t\t: "))
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let ram = fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|contents| {
            contents.lines().find_map(|line| {
                line.strip_prefix("MemTotal:")?
                    .split_whitespace()
                    .next()?
                    .parse()
                    .ok()
            })
        });
    let container = Path::new("/.dockerenv").exists();
    let avx2 = cpuinfo.lines().any(|line| {
        line.starts_with("flags") && line.split_whitespace().any(|flag| flag == "avx2")
    });
    let qualifies = !container && avx2 && cores == 4 && model.contains("i5-10210U");
    HostEvidence {
        os: env::consts::OS.to_owned(),
        kernel: command_output("uname", &["-sr"]),
        cpu_model: model,
        avx2,
        physical_cores: (cores > 0).then_some(cores),
        ram_kib: ram,
        container,
        qualifies,
    }
}

fn write_report(
    root: &Path,
    status: &'static str,
    manifest: &ApprovedAssetManifest,
    host: &HostEvidence,
    started: u64,
    measurements: &Value,
) -> Result<&'static str, String> {
    let revision = command_output("git", &["rev-parse", "HEAD"]);
    let clean = Command::new("git")
        .args(["diff", "--quiet", "--ignore-submodules", "HEAD", "--"])
        .output()
        .is_ok_and(|output| output.status.success());
    let report = json!({
        "schema_version": 1,
        "status": status,
        "run_id": format!("task4-{started}"),
        "started_at_unix_seconds": started,
        "finished_at_unix_seconds": unix_seconds()?,
        "command": ["benchmark", "--backend", "tract", "--profile", "avx2-minimum", "--duration", "300"],
        "git_revision": revision,
        "clean_worktree": clean,
        "worker_inference_only": true,
        "backend": "tract",
        "backend_version": "deep_filter-v0.5.6",
        "runtime": "tract",
        "runtime_version": "0.19.16",
        "profile": "avx2-minimum",
        "asset_id": manifest.asset_id(),
        "asset_sha256": manifest.asset_sha256(),
        "candidate_record_sha256": manifest.candidate_record_sha256(),
        "legal_review_record_sha256": manifest.legal_review_record_sha256(),
        "key_id": manifest.key_id(),
        "requested_duration_seconds": REQUESTED_SECONDS,
        "warm_up_policy": "one ordered pass over every frozen corpus frame",
        "deadline_ms": DEADLINE_MS,
        "p99_limit_ms": P99_LIMIT_MS,
        "host": {
            "os": host.os,
            "kernel": host.kernel,
            "cpu_model": host.cpu_model,
            "avx2": host.avx2,
            "physical_cores": host.physical_cores,
            "ram_kib": host.ram_kib,
            "container_or_vm_ambiguity": host.container,
            "reference_class_qualified": host.qualifies,
            "frequency_ac_affinity_governor": "not_observable"
        },
        "measurements": measurements
    });
    let bytes = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    fs::create_dir_all(root.join("benchmarks")).map_err(|error| error.to_string())?;
    fs::write(root.join("benchmarks/cpu-baseline.json"), &bytes)
        .map_err(|error| error.to_string())?;
    let digest = format!("{:x}", Sha256::digest(&bytes));
    let markdown = format!(
        "# CPU Baseline\n\nStatus: `{status}`.\n\nCanonical report: `benchmarks/cpu-baseline.json` (SHA-256 `{digest}`).\n\nThis report measures inference-worker latency only and does not claim product end-to-end latency or M1 approval. See the canonical JSON for measured values and blockers.\n"
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
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let expected = [
        "--backend",
        "tract",
        "--profile",
        "avx2-minimum",
        "--duration",
        "300",
    ];
    arguments
        .iter()
        .map(String::as_str)
        .eq(expected)
        .then_some(())
        .ok_or_else(|| {
            "benchmark requires --backend tract --profile avx2-minimum --duration 300".to_owned()
        })
}

fn frame_to_array(frame: &[f32]) -> Result<[f32; realtime_noise_contracts::HOP_SAMPLES], String> {
    frame
        .try_into()
        .map_err(|_| "golden input frame has the wrong shape".to_owned())
}

fn percentile(values: &[f64], percentage: usize) -> Option<f64> {
    let index = values
        .len()
        .saturating_sub(1)
        .saturating_mul(percentage)
        .saturating_add(99)
        / 100;
    values.get(index).copied()
}

fn command_output(command: &str, arguments: &[&str]) -> String {
    Command::new(command)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map_or_else(
            || "unavailable".to_owned(),
            |output| output.trim().to_owned(),
        )
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| error.to_string())
}
