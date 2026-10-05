//! Local-only end of the development path: the real base `DFNet3` denoiser (approved asset in
//! `vendor/approved/`) and the M3 development enrollment model from
//! `CLEARCORE_DEV_ENROLLMENT_ASSET` + `CLEARCORE_DEV_ENROLLMENT_SHA256`. Development-integrated,
//! not end-to-end and not a quality claim (spec 1, 3.1, 12.8).
//!
//! Run: `cargo test -p realtime-noise-service --features tract --test enrollment_real_model
//! -- --ignored --nocapture` with both variables set.
#![cfg(feature = "tract")]
#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]

mod common;

use base64::Engine as _;
use common::{TempDir, send};
use realtime_noise_contracts::AudioFrame;
use realtime_noise_ipc::{IpcCommand, IpcStatus};
use realtime_noise_model::{
    BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame, ProfileStore, VoiceProfile,
};
use realtime_noise_service::{EnrollmentHooks, ServiceDaemon};
use realtime_noise_supervisor::EngineSupervisor;
use serde_json::Value;
use std::path::PathBuf;
use std::time::{Duration, Instant};

struct ProfileBackend;

impl InferenceBackend for ProfileBackend {
    fn descriptor(&self) -> BackendDescriptor {
        BackendDescriptor {
            backend: "passthrough",
            backend_version: "1",
            runtime: "test",
            runtime_version: "1",
            asset_id: "test".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "test",
        }
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        ProcessedFrame::checked(*input, 1_440, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        1_440
    }

    fn set_voice_profile(&mut self, _p: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        Ok(())
    }
}

/// About 3 s of formant-synthesized "speech": a glottal-like harmonic source with a gliding
/// pitch, two alternating vowel formant sets and a syllabic envelope.
fn synthetic_speech(seconds: f32) -> Vec<f32> {
    let rate = 48_000.0_f32;
    let n = (seconds * rate) as usize;
    let mut phase = 0.0_f32;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let t = i as f32 / rate;
        let f0 = 120.0 + 30.0 * (2.0 * std::f32::consts::PI * 0.7 * t).sin();
        phase += 2.0 * std::f32::consts::PI * f0 / rate;
        let vowel_a = ((t / 0.3) as usize).is_multiple_of(2);
        let formants: [(f32, f32); 3] = if vowel_a {
            [(700.0, 110.0), (1_200.0, 120.0), (2_600.0, 160.0)]
        } else {
            [(300.0, 90.0), (2_300.0, 140.0), (3_000.0, 180.0)]
        };
        let mut v = 0.0_f32;
        for k in 1..=30 {
            let f = k as f32 * f0;
            let gain: f32 = formants
                .iter()
                .map(|(fc, bw)| 1.0 / (1.0 + ((f - fc) / bw).powi(2)))
                .sum();
            v += gain / k as f32 * (k as f32 * phase).sin();
        }
        let syllable = (2.0 * std::f32::consts::PI * 4.0 * t).sin().abs().sqrt();
        out.push(v * (0.25 + 0.75 * syllable));
    }
    let peak = out.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    out.iter().map(|v| v * 0.5 / peak).collect()
}

fn wait_job(daemon: &mut ServiceDaemon, id: &str) -> Value {
    let start = Instant::now();
    loop {
        let resp = send(
            daemon,
            IpcCommand::GetEnrollmentJob {
                job_id: id.to_owned(),
            },
        );
        assert_eq!(resp.status, IpcStatus::Ok, "{resp:?}");
        if resp.payload["state"] != "running" {
            return resp.payload;
        }
        assert!(start.elapsed() < Duration::from_secs(600), "job timed out");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "needs the M3 development asset (env) and the approved DFNet3; run locally"]
fn real_denoiser_and_development_model_build_and_apply_a_profile() {
    let asset = std::env::var("CLEARCORE_DEV_ENROLLMENT_ASSET").expect("asset env");
    let sha = std::env::var("CLEARCORE_DEV_ENROLLMENT_SHA256").expect("sha env");
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root");
    let temp = TempDir::new("enroll-real-model");
    let mut supervisor = EngineSupervisor::default();
    supervisor.set_backend(Box::new(ProfileBackend), "fake");
    let mut daemon =
        ServiceDaemon::with_supervisor(supervisor).with_enrollment_hooks(EnrollmentHooks {
            denoiser_factory: None,
            model_factory: None,
            dev_enrollment_asset: Some((PathBuf::from(asset), sha)),
            legacy_samples_dir: None,
        });
    daemon.set_repo_root(repo_root);
    daemon.attach_profile_store(ProfileStore::new(temp.path()));

    let clip = synthetic_speech(3.0);
    let bytes: Vec<u8> = clip.iter().flat_map(|v| v.to_le_bytes()).collect();
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut speech_total = 0.0;
    let ingest_start = Instant::now();
    for _ in 0..3 {
        let resp = send(
            &mut daemon,
            IpcCommand::AddVoiceSample {
                name: "Frase".to_owned(),
                pcm_f32_le_b64: b64.clone(),
                sample_rate: 48_000,
                device_label: "Mic".to_owned(),
                device_id_hash: "dev".to_owned(),
            },
        );
        assert_eq!(resp.status, IpcStatus::Ok, "{resp:?}");
        let id = resp.payload["job_id"].as_str().expect("job").to_owned();
        let job = wait_job(&mut daemon, &id);
        eprintln!("ingest job: {job}");
        assert_eq!(job["state"], "done", "{job}");
        speech_total += job["quality"]["speech_seconds"].as_f64().expect("speech");
        let wav = temp
            .path()
            .join("samples")
            .join(format!("{}.wav", job["sample_id"].as_str().expect("id")));
        let (stored, _) =
            realtime_noise_model::wav::decode_wav_pcm16_mono(&std::fs::read(wav).expect("wav"))
                .expect("decode");
        let max_diff = stored
            .iter()
            .zip(&clip)
            .fold(0.0_f32, |m, (s, r)| m.max((s - r).abs()));
        eprintln!("max |stored - raw| = {max_diff:.4}");
    }
    let ingest_elapsed = ingest_start.elapsed();
    eprintln!("3 x 3 s ingested in {ingest_elapsed:?}; stored speech {speech_total:.2} s");
    assert!(speech_total >= 6.0, "{speech_total}");

    let build_start = Instant::now();
    let resp = send(
        &mut daemon,
        IpcCommand::BuildVoiceProfile {
            name: "Dev".to_owned(),
        },
    );
    assert_eq!(resp.status, IpcStatus::Ok, "{resp:?}");
    let id = resp.payload["job_id"].as_str().expect("job").to_owned();
    let job = wait_job(&mut daemon, &id);
    let build_elapsed = build_start.elapsed();
    eprintln!("build job: {job} in {build_elapsed:?}");
    assert_eq!(job["state"], "done", "{job}");
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], job["profile_id"]);
}
