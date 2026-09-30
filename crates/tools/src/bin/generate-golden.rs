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
        Err(err) => blocked(blocked_status(&err), &blocked_reason(&err)),
    }
}

#[expect(clippy::too_many_lines, reason = "linear qualification gate")]
fn run() -> Result<(), ManifestError> {
    let fresh = env::args().skip(1).eq(["--fresh"]);
    if !fresh {
        return Err(ManifestError::Block(
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
    let corpus = Manifest::parse(&corpus_bytes)?;
    validate_corpus(&root, &corpus)?;

    let mut backend =
        TractBackend::new(&manifest, CpuProfile::Avx2Minimum).map_err(golden_block)?;
    let descriptor = backend.descriptor();
    
    let (cases_iter, corpus_sha256, input_norm, q_metric, q_metric_v, q_thresh, q_obs, tol) = match &corpus {
        Manifest::V1(v1) => (Box::new(v1.cases.iter().map(|c| (c.case_id.clone(), c.frames_path.clone(), c.input_sha256.clone(), c.frame_count))) as Box<dyn Iterator<Item = (String, std::path::PathBuf, String, usize)>>, &v1.corpus_sha256, &v1.input_normalization, &v1.quality_metric, &v1.quality_metric_version, v1.quality_threshold, v1.quality_observed_value, &v1.tolerance),
        Manifest::V2(v2) => (Box::new(v2.cases.iter().map(|c| (c.case_id.clone(), c.frames_path.clone(), c.input_sha256.clone(), c.frame_count))) as Box<dyn Iterator<Item = _>>, &v2.corpus_sha256, &v2.input_normalization, &v2.quality_metric, &v2.quality_metric_version, v2.quality_threshold, v2.quality_observed_value, &v2.tolerance),
    };
    let mut cases = Vec::new();
    for (case_id, frames_path, input_sha256, frame_count) in cases_iter {

        let input_frames: Vec<Vec<f32>> = serde_json::from_slice(&read_regular(
            &root.join("fixtures/corpus").join(&frames_path),
        )?)
        .map_err(golden_block)?;
        
        // We defer validation since it needs the original case struct.
        // Actually, let's just do it inline here.
        if input_frames.len() != frame_count || input_frames.iter().any(|frame| {
            frame.len() != realtime_noise_contracts::HOP_SAMPLES || frame.iter().any(|sample| !sample.is_finite())
        }) || frames_sha256(&input_frames) != input_sha256 {
            return Err(ManifestError::Block("BLOCKED_PENDING_GOLDEN", "corpus frames do not match their approved binding".to_string()));
        }

        let mut output_frames = Vec::with_capacity(input_frames.len());
        for frame in &input_frames {
            let input: [f32; realtime_noise_contracts::HOP_SAMPLES] =
                frame
                    .as_slice()
                    .try_into()
                    .map_err(|_| ManifestError::Block("BLOCKED_PENDING_GOLDEN", "input frame shape is invalid".to_string()))?;
            output_frames.push(
                backend
                    .process(&input)
                    .map_err(golden_block)?
                    .samples
                    .to_vec(),
            );
        }
        cases.push(GoldenCase {
            case_id: case_id,
            input_sha256: input_sha256,
            output_sha256: frames_sha256(&output_frames),
            frame_count: frame_count,
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
        corpus_sha256: corpus_sha256.clone(),
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
        input_normalization: input_norm.clone(),
        quality_metric: q_metric.clone(),
        quality_metric_version: q_metric_v.clone(),
        quality_threshold: q_thresh,
        quality_observed_value: q_obs,
        tolerance: tol.clone(),
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
        return Err(ManifestError::Block("BLOCKED_PENDING_GOLDEN", 
            "fresh generation did not create a new provenance identity".to_string(),
        ));
    }
    let bytes = serde_json::to_vec_pretty(&fixture).map_err(golden_block)?;
    let temporary = output.with_extension("json.tmp");
    fs::write(&temporary, bytes).map_err(io_block)?;
    fs::rename(temporary, output).map_err(io_block)?;
    println!("{}", json!({"status": "GOLDEN_GENERATED"}));
    Ok(())
}



fn read_regular(path: &Path) -> Result<Vec<u8>, ManifestError> {
    let metadata = fs::symlink_metadata(path).map_err(io_block)?;
    if !metadata.file_type().is_file() {
        return Err(ManifestError::Block("BLOCKED_PENDING_GOLDEN", 
            "generation input is not a regular non-symlink file".to_string(),
        ));
    }
    fs::read(path).map_err(io_block)
}

fn required_environment(name: &str) -> Result<String, ManifestError> {
    env::var(name).map_err(|_| pending(format!("{name} is required")))
}

fn require_environment(name: &str, expected: &str) -> Result<(), ManifestError> {
    if required_environment(name)? == expected {
        Ok(())
    } else {
        Err(ManifestError::Block("BLOCKED_PENDING_GOLDEN", format!("{name} must equal {expected}")))
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





fn pending(reason: String) -> ManifestError {
    ManifestError::Block("BLOCKED_PENDING_GOLDEN", reason)
}

fn golden_block(error: impl std::fmt::Display) -> ManifestError {
    pending(error.to_string())
}

fn io_block(error: impl std::fmt::Display) -> ManifestError {
    ManifestError::Block("BLOCKED_PENDING_GOLDEN", error.to_string())
}

fn blocked(status: &str, reason: &str) -> ExitCode {
    eprintln!("{}", json!({"status": status, "reason": reason}));
    ExitCode::from(2)
}
