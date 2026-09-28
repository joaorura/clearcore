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
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<std::alloc::System> = &INSTRUMENTED_SYSTEM;

const REQUESTED_SECONDS: u64 = 300;
const REQUESTED_SECONDS_F64: f64 = 300.0;
const DEADLINE_MS: f64 = 10.0;
const P99_LIMIT_MS: f64 = 7.0;

struct LatencyMetrics {
    p99_ms: f64,
    max_ms: f64,
    deadline_misses: usize,
}

fn qualification_status(
    duration_seconds: f64,
    latency: &LatencyMetrics,
    allocation_count: usize,
    clean_worktree: bool,
) -> &'static str {
    if duration_seconds >= REQUESTED_SECONDS_F64
        && latency.p99_ms <= P99_LIMIT_MS
        && latency.max_ms <= DEADLINE_MS
        && latency.deadline_misses == 0
        && allocation_count == 0
        && clean_worktree
    {
        "M1_APPROVED"
    } else {
        "M1_FAILED_BENCHMARK"
    }
}

fn allocation_not_run() -> Value {
    json!({
        "mechanism": "stats_alloc-0.1.10",
        "available": true,
        "measurement_completed": false,
        "initialization_count": Value::Null,
        "initialization_bytes": Value::Null,
        "warm_up_count": Value::Null,
        "warm_up_bytes": Value::Null,
        "total_count": Value::Null,
        "total_bytes": Value::Null,
        "per_hop_count": Value::Null,
        "per_hop_bytes": Value::Null
    })
}

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
    let clean_worktree = worktree_is_clean(&root);
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
                clean_worktree,
                &json!({"reason": error.to_string(), "allocation": allocation_not_run()}),
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
            clean_worktree,
            &json!({"golden_provenance_sha256": golden.provenance_sha256, "allocation": allocation_not_run()}),
        );
    }

    let initialization_region = Region::new(GLOBAL);
    let mut backend =
        TractBackend::new(&manifest, CpuProfile::Avx2Minimum).map_err(|error| error.to_string())?;
    let initialization = initialization_region.change();
    let initialization_count = initialization.allocations + initialization.reallocations;
    let initialization_bytes = allocation_bytes(&initialization)?;
    verify_golden(&mut backend, &golden)?;
    let benchmark = measure_backend(
        &mut backend,
        &golden,
        initialization_count,
        initialization_bytes,
    )?;
    write_report(
        &root,
        qualification_status(
            benchmark.measured_duration_seconds,
            &benchmark.latency,
            benchmark.allocation_count,
            clean_worktree,
        ),
        &manifest,
        &host,
        started,
        clean_worktree,
        &benchmark.metrics,
    )
}

struct BenchmarkResult {
    metrics: Value,
    latency: LatencyMetrics,
    measured_duration_seconds: f64,
    allocation_count: usize,
}

fn verify_golden(backend: &mut TractBackend, golden: &GoldenFixture) -> Result<(), String> {
    golden
        .validate_for(&backend.descriptor())
        .map_err(|error| error.to_string())?;
    for case in &golden.cases {
        let mut actual = Vec::with_capacity(case.input_frames.len());
        for frame in &case.input_frames {
            let input = frame_to_array(frame)?;
            actual.push(
                backend
                    .process(&input)
                    .map_err(|error| error.to_string())?
                    .samples
                    .to_vec(),
            );
        }
        golden
            .compare_case(&case.case_id, &actual)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn measure_backend(
    backend: &mut TractBackend,
    golden: &GoldenFixture,
    initialization_count: usize,
    initialization_bytes: isize,
) -> Result<BenchmarkResult, String> {
    let warm_up_region = Region::new(GLOBAL);
    for case in &golden.cases {
        for frame in &case.input_frames {
            let input = frame_to_array(frame)?;
            backend.process(&input).map_err(|error| error.to_string())?;
        }
    }
    let warm_up = warm_up_region.change();
    let warm_up_count = warm_up.allocations + warm_up.reallocations;
    let warm_up_bytes = allocation_bytes(&warm_up)?;

    let measurement_start = Instant::now();
    let measurement_duration = Duration::from_secs(REQUESTED_SECONDS);
    let mut elapsed_ms = Vec::new();
    let mut per_hop_count = Vec::new();
    let mut per_hop_bytes = Vec::new();
    'measurement: while measurement_start.elapsed() < measurement_duration {
        for case in &golden.cases {
            for frame in &case.input_frames {
                if measurement_start.elapsed() >= measurement_duration {
                    break 'measurement;
                }
                let input = frame_to_array(frame)?;
                let region = Region::new(GLOBAL);
                let started_hop = Instant::now();
                backend.process(&input).map_err(|error| error.to_string())?;
                let hop_elapsed_ms = started_hop.elapsed().as_secs_f64() * 1_000.0;
                let allocation = region.change();
                let allocation_count = allocation.allocations + allocation.reallocations;
                let hop_allocation_bytes = allocation_bytes(&allocation)?;
                elapsed_ms.push(hop_elapsed_ms);
                per_hop_count.push(allocation_count);
                per_hop_bytes.push(hop_allocation_bytes);
            }
        }
    }
    elapsed_ms.sort_by(f64::total_cmp);
    let measured = measurement_start.elapsed().as_secs_f64();
    let deadline_misses = elapsed_ms
        .iter()
        .filter(|&&elapsed| elapsed > DEADLINE_MS)
        .count();
    let total_count = per_hop_count.iter().sum::<usize>();
    let total_bytes = per_hop_bytes.iter().sum::<isize>();
    let latency = LatencyMetrics {
        p99_ms: percentile(&elapsed_ms, 99).unwrap_or(f64::INFINITY),
        max_ms: elapsed_ms.last().copied().unwrap_or(f64::INFINITY),
        deadline_misses,
    };
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
            "mechanism": "stats_alloc-0.1.10",
            "available": true,
            "measurement_completed": true,
            "initialization_count": initialization_count,
            "initialization_bytes": initialization_bytes,
            "warm_up_count": warm_up_count,
            "warm_up_bytes": warm_up_bytes,
            "total_count": total_count,
            "total_bytes": total_bytes,
            "per_hop_count": per_hop_count,
            "per_hop_bytes": per_hop_bytes
        }
    });
    Ok(BenchmarkResult {
        metrics,
        latency,
        measured_duration_seconds: measured,
        allocation_count: total_count,
    })
}

struct HostEvidence {
    os: String,
    kernel: String,
    cpu_model: String,
    avx2: bool,
    physical_cores: Option<usize>,
    ram_kib: Option<u64>,
    container: bool,
    operating_conditions: &'static str,
    qualifies: bool,
}

fn reference_host_qualifies(
    container: bool,
    avx2: bool,
    cores: usize,
    model: &str,
    ram_kib: Option<u64>,
    operating_conditions_observed: bool,
) -> bool {
    !container
        && avx2
        && cores == 4
        && model.contains("i5-10210U")
        && ram_kib.is_some_and(|ram| ram >= 8 * 1024 * 1024)
        && operating_conditions_observed
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
    let operating_conditions_observed = false;
    let qualifies = reference_host_qualifies(
        container,
        avx2,
        cores,
        &model,
        ram,
        operating_conditions_observed,
    );
    HostEvidence {
        os: env::consts::OS.to_owned(),
        kernel: command_output("uname", &["-sr"]),
        cpu_model: model,
        avx2,
        physical_cores: (cores > 0).then_some(cores),
        ram_kib: ram,
        container,
        operating_conditions: "not_observable",
        qualifies,
    }
}

fn write_report(
    root: &Path,
    status: &'static str,
    manifest: &ApprovedAssetManifest,
    host: &HostEvidence,
    started: u64,
    clean_worktree: bool,
    measurements: &Value,
) -> Result<&'static str, String> {
    let revision = command_output("git", &["rev-parse", "HEAD"]);
    let report = json!({
        "schema_version": 1,
        "status": status,
        "run_id": format!("task4-{started}"),
        "started_at_unix_seconds": started,
        "finished_at_unix_seconds": unix_seconds()?,
        "command": ["benchmark", "--backend", "tract", "--profile", "avx2-minimum", "--duration", "300"],
        "git_revision": revision,
        "clean_worktree": clean_worktree,
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
            "operating_conditions_observed": host.operating_conditions != "not_observable",
            "reference_class_qualified": host.qualifies,
            "frequency_ac_affinity_governor": host.operating_conditions
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

fn allocation_bytes(allocation: &stats_alloc::Stats) -> Result<isize, String> {
    let allocated_bytes = isize::try_from(allocation.bytes_allocated)
        .map_err(|_| "allocation byte count exceeds isize".to_owned())?;
    Ok(allocated_bytes + allocation.bytes_reallocated)
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

fn worktree_is_clean(root: &Path) -> bool {
    Command::new("git")
        .current_dir(root)
        .args(["status", "--porcelain", "--untracked-files=all"])
        .output()
        .is_ok_and(|output| output.status.success() && output.stdout.is_empty())
}

fn unix_seconds() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{LatencyMetrics, qualification_status, reference_host_qualifies};

    #[test]
    fn qualification_status_approves_zero_allocation_qualifying_run() {
        let latency = LatencyMetrics {
            p99_ms: 7.0,
            max_ms: 10.0,
            deadline_misses: 0,
        };

        assert_eq!(
            qualification_status(300.0, &latency, 0, true),
            "M1_APPROVED"
        );
    }

    #[test]
    fn qualification_status_rejects_nonzero_allocation_qualifying_run() {
        let latency = LatencyMetrics {
            p99_ms: 7.0,
            max_ms: 10.0,
            deadline_misses: 0,
        };

        assert_eq!(
            qualification_status(300.0, &latency, 1, true),
            "M1_FAILED_BENCHMARK"
        );
    }

    #[test]
    fn qualification_status_rejects_dirty_worktree() {
        let latency = LatencyMetrics {
            p99_ms: 7.0,
            max_ms: 10.0,
            deadline_misses: 0,
        };

        assert_eq!(
            qualification_status(300.0, &latency, 0, false),
            "M1_FAILED_BENCHMARK"
        );
    }

    #[test]
    fn host_qualification_rejects_unobserved_operating_conditions() {
        assert!(!reference_host_qualifies(
            false,
            true,
            4,
            "Intel(R) Core(TM) i5-10210U CPU",
            Some(8_388_608),
            false,
        ));
    }

    #[test]
    fn host_qualification_accepts_complete_reference_evidence() {
        assert!(reference_host_qualifies(
            false,
            true,
            4,
            "Intel(R) Core(TM) i5-10210U CPU",
            Some(8_388_608),
            true,
        ));
    }
}
