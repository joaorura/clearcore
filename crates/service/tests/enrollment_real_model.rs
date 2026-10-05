//! Local-only end of the development path: the real base `DFNet3` denoiser (approved asset in
//! `vendor/approved/`) and the M3 development enrollment model from
//! `CLEARCORE_DEV_ENROLLMENT_ASSET` + `CLEARCORE_DEV_ENROLLMENT_SHA256`. Development-integrated,
//! not end-to-end and not a quality claim (spec 1, 3.1, 12.8).
//!
//! Run: `cargo test -p realtime-noise-service --release --test enrollment_real_model
//! -- --ignored --nocapture` with both variables set.
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

/// Stretches (seconds) of `speech_over_noise` that contain only the white noise.
const NOISE_ONLY: [(f32, f32); 2] = [(0.05, 0.55), (3.45, 3.95)];

/// 4 s: constant white noise at about -34 dBFS everywhere, synthetic speech from 0.6 to 3.4 s.
fn speech_over_noise() -> Vec<f32> {
    let speech = synthetic_speech(2.8);
    let mut seed = 0x9e37_79b9_u32;
    (0..192_000_usize)
        .map(|i| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let white = ((seed >> 8) as f32 / 8_388_608.0 - 1.0) * 0.035;
            let voice = i
                .checked_sub(28_800)
                .and_then(|j| speech.get(j))
                .copied()
                .unwrap_or(0.0);
            voice + white
        })
        .collect()
}

fn energy_db(x: &[f32]) -> f32 {
    let mean_square = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
    10.0 * (mean_square + 1e-12).log10()
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

    let clip = speech_over_noise();
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
        // Content check: the stretches with ONLY the constant white noise must come out at
        // least 6 dB quieter than they went in.
        for (start, end) in NOISE_ONLY {
            let range = (start * 48_000.0) as usize..(end * 48_000.0) as usize;
            let before = energy_db(&clip[range.clone()]);
            let after = energy_db(&stored[range]);
            eprintln!(
                "noise-only {start:.2}-{end:.2} s: in {before:.1} dB, out {after:.1} dB, drop {:.1} dB",
                before - after
            );
            assert!(before - after >= 6.0, "noise was not reduced by 6 dB");
        }
    }
    let ingest_elapsed = ingest_start.elapsed();
    eprintln!("3 x 4 s ingested in {ingest_elapsed:?}; stored speech {speech_total:.2} s");
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
