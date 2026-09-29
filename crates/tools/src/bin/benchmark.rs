use std::{
    env, fs,
    io::Read,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use realtime_noise_model::{
    ApprovedAssetManifest, CpuProfile, GoldenFixture, InferenceBackend, TractBackend,
};
use realtime_noise_tools::host_evidence::{
    BoundHostEvidence, HostEvidenceFiles, LocalHostFacts, RunBinding, bind_host_evidence,
    observe_local_host,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};

#[global_allocator]
static GLOBAL: &StatsAlloc<std::alloc::System> = &INSTRUMENTED_SYSTEM;

const REQUESTED_SECONDS: u64 = 300;
const MAXIMUM_HOST_EVIDENCE_BYTES: usize = 16 * 1024 * 1024;
const MAXIMUM_HOST_PROVENANCE_BYTES: usize = 64 * 1024;
const REQUESTED_SECONDS_F64: f64 = 300.0;
const DEADLINE_MS: f64 = 10.0;
const MAX_DURATION_SECONDS: f64 = REQUESTED_SECONDS_F64 + DEADLINE_MS / 1_000.0;
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
    if (REQUESTED_SECONDS_F64..=MAX_DURATION_SECONDS).contains(&duration_seconds)
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
        "verification_count": Value::Null,
        "verification_bytes": Value::Null,
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
    let arguments = validate_arguments()?;
    let root = env::current_dir().map_err(|error| error.to_string())?;
    let started = unix_seconds()?;
    let run_id = arguments
        .run_id
        .clone()
        .unwrap_or_else(|| format!("task4-{started}"));
    let clean_worktree = worktree_is_clean(&root);
    let host = host_evidence(&root, &arguments, &run_id, started);
    let manifest = match ApprovedAssetManifest::verify(&root) {
        Ok(manifest) => manifest,
        Err(error) => {
            return write_report(
                &root,
                "BLOCKED_NO_APPROVED_ASSET",
                None,
                &host,
                started,
                clean_worktree,
                &json!({"reason": error.to_string(), "allocation": allocation_not_run()}),
            );
        }
    };
    if !host.qualifies() {
        return write_report(
            &root,
            "BLOCKED_UNSUPPORTED_CPU_PROFILE",
            Some(&manifest),
            &host,
            started,
            clean_worktree,
            &json!({"reason": host.rejection_reason.as_deref(), "allocation": allocation_not_run()}),
        );
    }
    let golden = match GoldenFixture::read(&root.join("fixtures/golden/frozen-reference.json")) {
        Ok(golden) => golden,
        Err(error) => {
            return write_report(
                &root,
                "BLOCKED_PENDING_GOLDEN",
                Some(&manifest),
                &host,
                started,
                clean_worktree,
                &json!({"reason": error.to_string(), "allocation": allocation_not_run()}),
            );
        }
    };
    let initialization_region = Region::new(GLOBAL);
    let mut backend =
        TractBackend::new(&manifest, CpuProfile::Avx2Minimum).map_err(|error| error.to_string())?;
    let initialization = initialization_region.change();
    let initialization_count = initialization.allocations + initialization.reallocations;
    let initialization_bytes = allocation_bytes(&initialization)?;
    let verification_region = Region::new(GLOBAL);
    verify_golden(&mut backend, &golden)?;
    let verification = verification_region.change();
    let verification_count = verification.allocations + verification.reallocations;
    let verification_bytes = allocation_bytes(&verification)?;
    let benchmark = measure_backend(
        &mut backend,
        &golden,
        verification_count,
        verification_bytes,
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
        Some(&manifest),
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
    verification_count: usize,
    verification_bytes: isize,
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
            "verification_count": verification_count,
            "verification_bytes": verification_bytes,
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

struct BenchmarkArguments {
    command: Vec<String>,
    evidence_paths: Option<(PathBuf, PathBuf)>,
    run_id: Option<String>,
}

struct HostEvidence {
    local: LocalHostFacts,
    kernel: String,
    verified: Option<BoundHostEvidence>,
    rejection_reason: Option<String>,
    run_id: String,
    command: Vec<String>,
}

impl HostEvidence {
    const fn qualifies(&self) -> bool {
        self.verified.is_some()
    }
}

fn host_evidence(
    root: &Path,
    arguments: &BenchmarkArguments,
    run_id: &str,
    started: u64,
) -> HostEvidence {
    let local = observe_local_host();
    let result = arguments.evidence_paths.as_ref().map_or_else(
        || Err("host evidence was not supplied".to_owned()),
        |(document_path, provenance_path)| {
            let document = read_evidence_file(root, document_path, MAXIMUM_HOST_EVIDENCE_BYTES)?;
            let provenance =
                read_evidence_file(root, provenance_path, MAXIMUM_HOST_PROVENANCE_BYTES)?;
            bind_host_evidence(
                HostEvidenceFiles {
                    document: &document,
                    provenance: &provenance,
                },
                &local,
                &RunBinding {
                    run_id,
                    started_at_unix_seconds: started,
                },
            )
            .map_err(|error| error.to_string())
        },
    );
    let (verified, rejection_reason) = match result {
        Ok(evidence) => (Some(evidence), None),
        Err(reason) => (None, Some(reason)),
    };
    HostEvidence {
        local,
        kernel: command_output("uname", &["-sr"]),
        verified,
        rejection_reason,
        run_id: run_id.to_owned(),
        command: arguments.command.clone(),
    }
}

fn read_evidence_file(root: &Path, path: &Path, maximum_bytes: usize) -> Result<Vec<u8>, String> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let file = fs::File::open(&path).map_err(|error| error.to_string())?;
    let opened_metadata = file.metadata().map_err(|error| error.to_string())?;
    let path_metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    if !opened_metadata.file_type().is_file()
        || !path_metadata.file_type().is_file()
        || path_metadata.file_type().is_symlink()
        || opened_metadata.dev() != path_metadata.dev()
        || opened_metadata.ino() != path_metadata.ino()
    {
        return Err(format!(
            "host evidence is not a regular file: {}",
            path.display()
        ));
    }
    if opened_metadata.len() > maximum_bytes as u64 {
        return Err(format!(
            "host evidence exceeds size limit: {}",
            path.display()
        ));
    }
    let mut bytes = Vec::new();
    file.take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > maximum_bytes {
        return Err(format!(
            "host evidence exceeds size limit: {}",
            path.display()
        ));
    }
    Ok(bytes)
}

fn write_report(
    root: &Path,
    status: &'static str,
    manifest: Option<&ApprovedAssetManifest>,
    host: &HostEvidence,
    started: u64,
    clean_worktree: bool,
    measurements: &Value,
) -> Result<&'static str, String> {
    let revision = command_output("git", &["rev-parse", "HEAD"]);
    let sustained_frequency = json!(
        host.verified
            .as_ref()
            .map(|evidence| &evidence.sustained_frequency)
    );
    let mut report = json!({
        "schema_version": 1,
        "status": status,
        "run_id": host.run_id,
        "started_at_unix_seconds": started,
        "finished_at_unix_seconds": unix_seconds()?,
        "command": host.command,
        "git_revision": revision,
        "clean_worktree": clean_worktree,
        "worker_inference_only": true,
        "backend": "tract",
        "backend_version": "deep_filter-v0.5.6",
        "runtime": "tract",
        "runtime_version": "0.19.16",
        "profile": "avx2-minimum",
        "asset_id": manifest.map(ApprovedAssetManifest::asset_id),
        "asset_sha256": manifest.map(ApprovedAssetManifest::asset_sha256),
        "candidate_record_sha256": manifest.map(ApprovedAssetManifest::candidate_record_sha256),
        "legal_review_record_sha256": manifest.map(ApprovedAssetManifest::legal_review_record_sha256),
        "key_id": manifest.map(ApprovedAssetManifest::key_id),
        "requested_duration_seconds": REQUESTED_SECONDS,
        "warm_up_policy": "one ordered pass over every frozen corpus frame",
        "deadline_ms": DEADLINE_MS,
        "p99_limit_ms": P99_LIMIT_MS,
        "host": {
            "os": host.local.os,
            "architecture": host.local.architecture,
            "kernel": host.kernel,
            "cpu_model": host.local.cpu_model,
            "avx2": host.local.avx2,
            "physical_cores": host.local.physical_cores,
            "ram_kib": host.local.ram_kib,
            "container_or_vm_ambiguity": host.local.virtualized,
            "operating_conditions_observed": host.verified.is_some(),
            "reference_class_qualified": host.qualifies(),
            "host_evidence_sha256": host.verified.as_ref().map(|evidence| &evidence.evidence_sha256),
            "host_evidence_run_id": host.verified.as_ref().map(|evidence| &evidence.run_id),
            "observation_started_at_unix_seconds": host.verified.as_ref().map(|evidence| evidence.observation_started_at_unix_seconds),
            "observation_finished_at_unix_seconds": host.verified.as_ref().map(|evidence| evidence.observation_finished_at_unix_seconds),
            "frequency_ac_affinity_governor": host.verified.as_ref().map(|evidence| &evidence.operating_conditions),
            "provenance": host.verified.as_ref().map(|evidence| &evidence.provenance),
            "rejection_reason": host.rejection_reason
        },
        "measurements": measurements
    });
    report["host"]["topology"] = json!(&host.local.topology);
    report["host"]["sustained_frequency"] = sustained_frequency;
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

fn validate_arguments() -> Result<BenchmarkArguments, String> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let expected = [
        "--backend",
        "tract",
        "--profile",
        "avx2-minimum",
        "--duration",
        "300",
    ];
    if arguments.len() < expected.len()
        || !arguments
            .iter()
            .take(expected.len())
            .map(String::as_str)
            .eq(expected)
    {
        return Err(usage());
    }
    let extras = &arguments[expected.len()..];
    let (evidence_paths, run_id) = match extras {
        [] => (None, None),
        [
            evidence_flag,
            evidence,
            provenance_flag,
            provenance,
            run_id_flag,
            run_id,
        ] if evidence_flag == "--host-evidence"
            && provenance_flag == "--host-evidence-provenance"
            && run_id_flag == "--run-id"
            && valid_run_id(run_id) =>
        {
            (
                Some((PathBuf::from(evidence), PathBuf::from(provenance))),
                Some(run_id.clone()),
            )
        }
        _ => return Err(usage()),
    };
    let mut command = vec!["benchmark".to_owned()];
    command.extend(arguments);
    Ok(BenchmarkArguments {
        command,
        evidence_paths,
        run_id,
    })
}

fn valid_run_id(run_id: &str) -> bool {
    !run_id.is_empty()
        && run_id.len() <= 64
        && run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn usage() -> String {
    "benchmark requires --backend tract --profile avx2-minimum --duration 300 and optional --host-evidence <json> --host-evidence-provenance <record> --run-id <id>"
        .to_owned()
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
    use std::{fs, os::unix::fs::symlink, path::Path, time::SystemTime};

    use super::{
        LatencyMetrics, MAXIMUM_HOST_EVIDENCE_BYTES, qualification_status, read_evidence_file,
    };

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
    fn qualification_status_rejects_excessive_duration_overrun() {
        let latency = LatencyMetrics {
            p99_ms: 7.0,
            max_ms: 10.0,
            deadline_misses: 0,
        };

        assert_eq!(
            qualification_status(300.011, &latency, 0, true),
            "M1_FAILED_BENCHMARK"
        );
    }

    #[test]
    fn evidence_reader_rejects_symlink() -> Result<(), Box<dyn std::error::Error>> {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "realtime-noise-host-evidence-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&root)?;
        fs::write(root.join("target.json"), b"{}")?;
        symlink("target.json", root.join("evidence.json"))?;

        let result = read_evidence_file(
            &root,
            Path::new("evidence.json"),
            MAXIMUM_HOST_EVIDENCE_BYTES,
        );
        fs::remove_dir_all(&root)?;

        assert!(result.is_err());
        Ok(())
    }
}
