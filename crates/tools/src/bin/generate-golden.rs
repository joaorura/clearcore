use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    process::{Command, ExitCode},
};

use realtime_noise_model::{
    ALGORITHM_LATENCY_SAMPLES, ApprovedAssetManifest, CpuProfile, GoldenCase, GoldenFixture,
    GoldenProvenance, InferenceBackend, NumericalTolerance, TractBackend, frames_sha256,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use realtime_noise_tools::golden_manifest::*;



fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err((status, reason)) => blocked(status, &reason),
    }
}

#[expect(clippy::too_many_lines, reason = "linear qualification gate")]
fn run() -> Result<(), (&'static str, String)> {
    let fresh = env::args().skip(1).eq(["--fresh"]);
    if !fresh {
        return Err((
            "BLOCKED_PENDING_GOLDEN",
            "generation requires explicit --fresh mode".to_owned(),
        ));
    }
    require_environment("M1_ISOLATED_OFFLINE", "1")?;
    let generated_at = required_environment("M1_GENERATED_AT_UTC")?;
    let host_os = required_environment("M1_HOST_OS")?;
    let host_cpu = required_environment("M1_HOST_CPU")?;
    let root = env::current_dir().map_err(io_block)?;
    let manifest = ApprovedAssetManifest::verify(&root).map_err(golden_block)?;
    let corpus_path = root.join("fixtures/corpus/corpus-manifest.json");
    let corpus_bytes = read_regular(&corpus_path)?;
    let corpus: CorpusManifest = serde_json::from_slice(&corpus_bytes).map_err(golden_block)?;
    validate_corpus(&root, &corpus)?;

    let mut backend =
        TractBackend::new(&manifest, CpuProfile::Avx2Minimum).map_err(golden_block)?;
    let descriptor = backend.descriptor();
    let mut cases = Vec::with_capacity(corpus.cases.len());
    for case in &corpus.cases {
        let input_frames: Vec<Vec<f32>> = serde_json::from_slice(&read_regular(
            &root.join("fixtures/corpus").join(&case.frames_path),
        )?)
        .map_err(golden_block)?;
        validate_input_case(case, &input_frames)?;
        let mut output_frames = Vec::with_capacity(input_frames.len());
        for frame in &input_frames {
            let input: [f32; realtime_noise_contracts::HOP_SAMPLES] =
                frame
                    .as_slice()
                    .try_into()
                    .map_err(|_| pending("input frame shape is invalid"))?;
            output_frames.push(
                backend
                    .process(&input)
                    .map_err(golden_block)?
                    .samples
                    .to_vec(),
            );
        }
        cases.push(GoldenCase {
            case_id: case.case_id.clone(),
            input_sha256: case.input_sha256.clone(),
            output_sha256: frames_sha256(&output_frames),
            frame_count: case.frame_count,
            input_frames,
            output_frames,
        });
    }
    let provenance = GoldenProvenance {
        schema_version: 1,
        asset_id: manifest.asset_id().to_owned(),
        asset_sha256: manifest.asset_sha256().to_owned(),
        candidate_record_sha256: manifest.candidate_record_sha256().to_owned(),
        legal_review_record_sha256: manifest.legal_review_record_sha256().to_owned(),
        key_id: manifest.key_id().to_owned(),
        backend: descriptor.backend.to_owned(),
        backend_version: descriptor.backend_version.to_owned(),
        runtime: descriptor.runtime.to_owned(),
        runtime_version: descriptor.runtime_version.to_owned(),
        cpu_profile: descriptor.cpu_profile.to_owned(),
        corpus_sha256: corpus.corpus_sha256,
        generator_revision: command_output("git", &["rev-parse", "HEAD"]),
        generator_command: "generate-golden --fresh".to_owned(),
        generated_at_utc: generated_at,
        host_os,
        host_cpu,
        isolated_offline: true,
        sample_rate_hz: realtime_noise_contracts::SAMPLE_RATE_HZ,
        channels: realtime_noise_contracts::CHANNELS,
        hop_samples: realtime_noise_contracts::HOP_SAMPLES,
        algorithmic_latency_samples: ALGORITHM_LATENCY_SAMPLES,
        input_normalization: corpus.input_normalization,
        quality_metric: corpus.quality_metric,
        quality_metric_version: corpus.quality_metric_version,
        quality_threshold: corpus.quality_threshold,
        quality_observed_value: corpus.quality_observed_value,
        tolerance: corpus.tolerance,
    };
    let provenance_sha256 = sha256(&serde_json::to_vec(&provenance).map_err(golden_block)?);
    let fixture = GoldenFixture {
        provenance,
        provenance_sha256,
        cases,
    };
    fixture.validate().map_err(golden_block)?;
    let output = root.join("fixtures/golden/frozen-reference.json");
    if let Ok(existing) = GoldenFixture::read(&output)
        && existing.provenance_sha256 == fixture.provenance_sha256
    {
        return Err(pending(
            "fresh generation did not create a new provenance identity",
        ));
    }
    let bytes = serde_json::to_vec_pretty(&fixture).map_err(golden_block)?;
    let temporary = output.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(io_block)?;
    fs::rename(temporary, output).map_err(io_block)?;
    println!("{}", json!({"status": "GOLDEN_GENERATED"}));
    Ok(())
}



fn read_regular(path: &Path) -> Result<Vec<u8>, (&'static str, String)> {
    let metadata = fs::symlink_metadata(path).map_err(io_block)?;
    if !metadata.file_type().is_file() {
        return Err(pending(
            "generation input is not a regular non-symlink file",
        ));
    }
    fs::read(path).map_err(io_block)
}

fn required_environment(name: &str) -> Result<String, (&'static str, String)> {
    env::var(name).map_err(|_| pending(&format!("{name} is required")))
}

fn require_environment(name: &str, expected: &str) -> Result<(), (&'static str, String)> {
    if required_environment(name)? == expected {
        Ok(())
    } else {
        Err(pending(&format!("{name} must equal {expected}")))
    }
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




fn io_block(error: impl std::fmt::Display) -> (&'static str, String) {
    pending(&error.to_string())
}

fn blocked(status: &str, reason: &str) -> ExitCode {
    eprintln!("{}", json!({"status": status, "reason": reason}));
    ExitCode::from(2)
}
