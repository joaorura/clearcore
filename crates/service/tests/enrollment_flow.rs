#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

mod common;

use base64::Engine as _;
use common::{TempDir, send};
use realtime_noise_contracts::AudioFrame;
use realtime_noise_ipc::enrollment_codes::{
    ENROLL_BUDGET_EXCEEDED, ENROLL_BUSY, ENROLL_INVALID_AUDIO, ENROLL_JOB_NOT_FOUND,
    ENROLL_MODEL_NOT_CONFIGURED, ENROLL_PAYLOAD_TOO_LARGE, ENROLL_TOO_LITTLE_SPEECH,
    MAX_REQUEST_LINE_BYTES,
};
use realtime_noise_ipc::{IpcCommand, IpcRequest, IpcResponse, IpcStatus};
use realtime_noise_model::enrollment::{EnrollmentError, RawFilmVectors, SpeakerEmbeddingModel};
use realtime_noise_model::wav::decode_wav_pcm16_mono;
use realtime_noise_model::{
    ACTIVE_PROFILE_FILE_NAME, BackendDescriptor, BandGains, FiLMVectors, InferenceBackend,
    InferenceError, ProcessedFrame, ProfileStore, VoiceProfile,
};
use realtime_noise_service::{
    Denoiser, EnrollError, EnrollmentHooks, IntakeTake, ServiceDaemon, VoiceSample,
    VoiceSampleManager,
};
use realtime_noise_supervisor::EngineSupervisor;
use serde_json::{Value, json};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- test doubles

/// Backend that accepts every profile (as in `tests/voice_profile.rs`), except ids starting with
/// `reject_prefix` when set.
struct ProfileBackend {
    reject_prefix: Option<&'static str>,
}

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

    fn set_voice_profile(&mut self, p: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        match (p, self.reject_prefix) {
            (Some(profile), Some(prefix)) if profile.id.starts_with(prefix) => {
                Err(InferenceError::UnsupportedFeature("injected".into()))
            }
            _ => Ok(()),
        }
    }
}

/// Scales by `gain` (0.5 by default), so the stored WAV is provably the denoised signal and
/// not the raw one.
struct GainDenoiser {
    delay: Duration,
    gain: f32,
}

impl Denoiser for GainDenoiser {
    fn denoise(&mut self, pcm48: &[f32]) -> Result<Vec<f32>, EnrollError> {
        std::thread::sleep(self.delay);
        Ok(pcm48.iter().map(|v| v * self.gain).collect())
    }
}

/// Returns valid `FiLM` vectors; optionally sleeps and records the peak of its 16 kHz input.
struct FakeModel {
    delay: Duration,
    seen_peak: Arc<Mutex<Option<f32>>>,
}

impl SpeakerEmbeddingModel for FakeModel {
    fn extract(&mut self, samples_16khz: &[f32]) -> Result<RawFilmVectors, EnrollmentError> {
        std::thread::sleep(self.delay);
        let peak = samples_16khz.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        *self.seen_peak.lock().unwrap() = Some(peak);
        Ok(RawFilmVectors {
            gamma_enc: vec![1.1; 256],
            beta_enc: vec![0.02; 256],
            gamma_df: vec![0.9; 256],
            beta_df: vec![-0.02; 256],
        })
    }
}

struct Setup {
    denoise_delay: Duration,
    /// `None` = no denoiser factory (production path; no repo root in these tests).
    denoise_gain: Option<f32>,
    /// `None` = production loader without a development asset (not configured).
    model_delay: Option<Duration>,
    legacy: Option<PathBuf>,
    /// Backend rejects built profiles (ids `p-...`).
    reject_built: bool,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            denoise_delay: Duration::ZERO,
            denoise_gain: Some(0.5),
            model_delay: Some(Duration::ZERO),
            legacy: None,
            reject_built: false,
        }
    }
}

fn daemon_with(dir: &Path, setup: Setup) -> (ServiceDaemon, Arc<Mutex<Option<f32>>>) {
    let seen_peak = Arc::new(Mutex::new(None));
    let peak_slot = Arc::clone(&seen_peak);
    let delay = setup.denoise_delay;
    let denoiser_factory: Option<realtime_noise_service::DenoiserFactory> =
        setup.denoise_gain.map(|gain| {
            let factory: realtime_noise_service::DenoiserFactory =
                Box::new(move || Box::new(GainDenoiser { delay, gain }));
            factory
        });
    let model_factory: Option<realtime_noise_service::ModelFactory> =
        setup.model_delay.map(|model_delay| {
            let factory: realtime_noise_service::ModelFactory = Box::new(move || {
                Ok(Box::new(FakeModel {
                    delay: model_delay,
                    seen_peak: Arc::clone(&peak_slot),
                }) as Box<dyn SpeakerEmbeddingModel + Send>)
            });
            factory
        });
    let hooks = EnrollmentHooks {
        denoiser_factory,
        model_factory,
        dev_enrollment_asset: None,
        legacy_samples_dir: setup.legacy,
    };
    let mut supervisor = EngineSupervisor::default();
    supervisor.set_backend(
        Box::new(ProfileBackend {
            reject_prefix: setup.reject_built.then_some("p-"),
        }),
        "fake",
    );
    let mut daemon = ServiceDaemon::with_supervisor(supervisor).with_enrollment_hooks(hooks);
    daemon.attach_profile_store(ProfileStore::new(dir));
    (daemon, seen_peak)
}

fn daemon(dir: &Path) -> ServiceDaemon {
    daemon_with(dir, Setup::default()).0
}

// ---------------------------------------------------------------- audio helpers

/// Speech-like 48 kHz signal (150 Hz voiced source with harmonics, 3 Hz envelope that never
/// reaches silence) at `scale`; every 20 ms frame counts as speech.
fn speech_pcm(seconds: f32, scale: f32) -> Vec<f32> {
    (0..(seconds * 48_000.0) as usize)
        .map(|n| {
            let t = n as f32 / 48_000.0;
            let envelope = 0.4f32.mul_add((2.0 * std::f32::consts::PI * 3.0 * t).sin(), 0.6);
            let tau = 2.0 * std::f32::consts::PI * 150.0 * t;
            let voiced = 0.2f32.mul_add(
                (3.0 * tau).sin(),
                0.5f32.mul_add(tau.sin(), 0.3 * (2.0 * tau).sin()),
            );
            scale * envelope * voiced
        })
        .collect()
}

fn b64(pcm: &[f32]) -> String {
    let bytes: Vec<u8> = pcm.iter().flat_map(|v| v.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn add_cmd(pcm: &[f32], device: &str) -> IpcCommand {
    IpcCommand::AddVoiceSample {
        name: "Frase".to_owned(),
        pcm_f32_le_b64: b64(pcm),
        sample_rate: 48_000,
        device_label: format!("Mic {device}"),
        device_id_hash: device.to_owned(),
    }
}

fn take_cmd(pcm: &[f32], device: &str) -> IpcCommand {
    IpcCommand::AddIntakeSuggestion {
        take_json: json!({
            "pcm_f32_le_b64": b64(pcm),
            "sample_rate": 48_000,
            "device_label": format!("Mic {device}"),
            "device_id_hash": device,
        })
        .to_string(),
    }
}

fn error_code(resp: &IpcResponse) -> &str {
    resp.error.as_ref().map_or("", |e| e.code.as_str())
}

fn job_id(resp: &IpcResponse) -> String {
    assert_eq!(resp.status, IpcStatus::Ok, "{resp:?}");
    resp.payload["job_id"].as_str().expect("job id").to_owned()
}

/// Polls `GetEnrollmentJob` until the job leaves `running`; returns the final payload.
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
        assert!(start.elapsed() < Duration::from_secs(60), "job timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Adds a sample over IPC and waits for it; returns the final job payload.
fn add_sample(daemon: &mut ServiceDaemon, seconds: f32, device: &str) -> Value {
    let resp = send(daemon, add_cmd(&speech_pcm(seconds, 0.3), device));
    let id = job_id(&resp);
    wait_job(daemon, &id)
}

/// Seeds a stored sample that counts in the budget (active, with audio).
fn seed_sample(daemon: &mut ServiceDaemon, id: &str, ts: u64, device: &str, speech: f32) {
    let sample = VoiceSample {
        id: id.to_owned(),
        timestamp: ts.to_string(),
        name: "seed".to_owned(),
        audio_path: Some(format!("/nonexistent/{id}.wav")),
        embedding: Vec::new(),
        is_active: true,
        device_label: format!("Mic {device}"),
        device_id_hash: device.to_owned(),
        capture_sample_rate: 48_000,
        speech_seconds: speech,
    };
    daemon.voice_samples_mut().add_sample(sample).expect("seed");
}

fn list(daemon: &mut ServiceDaemon) -> Value {
    let resp = send(daemon, IpcCommand::ListVoiceSamples);
    assert_eq!(resp.status, IpcStatus::Ok);
    resp.payload
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read dir") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(unix)]
fn mode_of(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).expect("meta").permissions().mode() & 0o777
}

fn film_vector(base: f32, step: f32) -> Vec<f32> {
    (0..256_u16)
        .map(|i| step.mul_add(f32::from(i), base))
        .collect()
}

fn test_profile(id: &str) -> VoiceProfile {
    let film = FiLMVectors::new(
        film_vector(0.5, 0.003),
        film_vector(-0.25, 0.001),
        film_vector(0.75, 0.002),
        film_vector(0.125, -0.0005),
    )
    .expect("film");
    let eq = BandGains::clamped(&[1.5; 32]).expect("eq");
    VoiceProfile::new(id, "Anterior", "2026-10-02T12:00:00Z", film, Some(eq)).expect("profile")
}

fn now_ms() -> u64 {
    u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_millis(),
    )
    .expect("ms")
}

fn request_line(command: IpcCommand) -> String {
    format!(
        "{}\n",
        IpcRequest::new(command, json!({})).to_json().expect("ser")
    )
}

fn responses(raw: &[u8]) -> Vec<IpcResponse> {
    String::from_utf8(raw.to_vec())
        .expect("utf8")
        .lines()
        .map(|l| IpcResponse::from_json(l).expect("parse"))
        .collect()
}

#[test]
fn oversized_request_line_is_rejected_and_the_connection_survives() {
    let _temp = TempDir::new("enroll-oversized");
    let mut daemon =
        ServiceDaemon::with_supervisor(realtime_noise_supervisor::EngineSupervisor::default());
    let mut input = vec![b'x'; MAX_REQUEST_LINE_BYTES + 1];
    input.push(b'\n');
    input.extend_from_slice(request_line(IpcCommand::GetStatus).as_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(Cursor::new(input), &mut writer)
        .expect("serve");
    let replies = responses(&writer.into_inner());
    assert_eq!(replies.len(), 2);
    assert_eq!(replies[0].status, IpcStatus::InvalidCommand);
    assert_eq!(
        replies[0].error.as_ref().expect("error").code,
        ENROLL_PAYLOAD_TOO_LARGE
    );
    assert_eq!(replies[1].status, IpcStatus::Ok);
    assert!(replies[1].payload.get("state").is_some());
}

#[test]
fn invalid_utf8_line_gets_a_fixed_parse_error_and_the_connection_survives() {
    let mut daemon =
        ServiceDaemon::with_supervisor(realtime_noise_supervisor::EngineSupervisor::default());
    let mut input = vec![0xff, 0xfe, b'S', b'E', b'C', b'R', b'E', b'T', b'\n'];
    input.extend_from_slice(request_line(IpcCommand::GetStatus).as_bytes());
    let mut writer = Cursor::new(Vec::new());
    daemon
        .serve_client(Cursor::new(input), &mut writer)
        .expect("serve");
    let raw = writer.into_inner();
    assert!(!String::from_utf8_lossy(&raw).contains("SECRET"));
    let replies = responses(&raw);
    assert_eq!(replies.len(), 2);
    assert_eq!(
        replies[0].error.as_ref().expect("error").code,
        "JSON_PARSE_ERROR"
    );
    assert_eq!(replies[1].status, IpcStatus::Ok);
}

// ---------------------------------------------------------------- samples and budget

#[test]
fn add_sample_then_job_done_then_sample_listed_with_quality() {
    let temp = TempDir::new("enroll-add");
    let mut daemon = daemon(temp.path());
    let job = add_sample(&mut daemon, 4.0, "mic-a");
    assert_eq!(job["state"], "done", "{job}");
    let sample_id = job["sample_id"].as_str().expect("sample id").to_owned();
    let quality = &job["quality"];
    assert!(quality["speech_seconds"].as_f64().expect("speech") > 3.5);
    assert!(quality["peak"].as_f64().expect("peak") > 0.1);
    assert!(quality["rms_dbfs"].as_f64().expect("rms") > -40.0);
    assert!(quality["active_fraction"].as_f64().expect("fraction") > 0.5);

    let listed = list(&mut daemon);
    assert_eq!(listed["total_count"], 1);
    let item = &listed["samples"][0];
    assert_eq!(item["id"], sample_id.as_str());
    assert_eq!(item["device_label"], "Mic mic-a");
    assert_eq!(item["used_in_profile"], true);
    assert_eq!(item["needs_reenroll"], false);
    assert_eq!(item["other_microphone"], false);
    assert!(item["speech_seconds"].as_f64().expect("speech") > 3.5);
    assert!(
        item.get("audio_path").is_none(),
        "the absolute path is never exposed"
    );
    let budget = &listed["budget"];
    assert_eq!(budget["max_seconds"], 90.0);
    let used = budget["used_seconds"].as_f64().expect("used");
    assert!(used > 3.5);
    assert!(
        (budget["remaining_seconds"].as_f64().expect("remaining") - (90.0 - used)).abs() < 1e-3
    );

    let wav = temp.path().join("samples").join(format!("{sample_id}.wav"));
    assert!(wav.is_file());
    #[cfg(unix)]
    {
        assert_eq!(mode_of(&wav), 0o600);
        assert_eq!(mode_of(&temp.path().join("samples")), 0o700);
    }
}

#[test]
fn sample_timestamps_are_epoch_milliseconds() {
    let temp = TempDir::new("enroll-timestamp");
    let mut daemon = daemon(temp.path());
    let before = now_ms();
    add_sample(&mut daemon, 3.0, "mic-a");
    let after = now_ms();
    let listed = list(&mut daemon);
    let ts: u64 = listed["samples"][0]["timestamp"]
        .as_str()
        .expect("timestamp")
        .parse()
        .expect("numeric timestamp");
    assert!(
        (before..=after).contains(&ts),
        "{before} <= {ts} <= {after}"
    );
}

#[test]
fn invalid_base64_or_rate_is_invalid_audio_and_stores_nothing() {
    let temp = TempDir::new("enroll-invalid");
    let mut daemon = daemon(temp.path());
    let mut cmd = add_cmd(&speech_pcm(1.0, 0.3), "mic-a");
    if let IpcCommand::AddVoiceSample { sample_rate, .. } = &mut cmd {
        *sample_rate = 44_100;
    }
    assert_eq!(error_code(&send(&mut daemon, cmd)), ENROLL_INVALID_AUDIO);
    let bad = IpcCommand::AddVoiceSample {
        name: "n".into(),
        pcm_f32_le_b64: "not-base64!".into(),
        sample_rate: 48_000,
        device_label: "Mic".into(),
        device_id_hash: "h".into(),
    };
    let resp = send(&mut daemon, bad);
    assert_eq!(error_code(&resp), ENROLL_INVALID_AUDIO);
    assert!(!resp.to_json().expect("json").contains("not-base64"));
    assert_eq!(list(&mut daemon)["total_count"], 0);
}

#[test]
fn unknown_job_is_not_found() {
    let temp = TempDir::new("enroll-unknown-job");
    let mut daemon = daemon(temp.path());
    for id in ["job-1", "sample-job-999", "profile-job-1", ""] {
        let resp = send(
            &mut daemon,
            IpcCommand::GetEnrollmentJob {
                job_id: id.to_owned(),
            },
        );
        assert_eq!(error_code(&resp), ENROLL_JOB_NOT_FOUND, "{id}");
    }
}

#[test]
fn manual_sample_over_budget_is_refused_and_nothing_is_stored() {
    let temp = TempDir::new("enroll-over-budget");
    let mut daemon = daemon(temp.path());
    seed_sample(&mut daemon, "seed-88", 1, "mic-a", 88.0);
    let job = add_sample(&mut daemon, 5.0, "mic-a");
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], ENROLL_BUDGET_EXCEEDED);
    assert!((job["remaining_seconds"].as_f64().expect("remaining") - 2.0).abs() < 1e-3);
    assert!(job["sample_id"].is_null());
    assert_eq!(list(&mut daemon)["total_count"], 1);
    assert!(
        !temp.path().join("samples").exists()
            || files_under(&temp.path().join("samples")).is_empty()
    );

    // Another microphone has its own budget.
    let other = add_sample(&mut daemon, 5.0, "mic-b");
    assert_eq!(other["state"], "done", "{other}");
}

#[test]
fn three_simultaneous_uploads_get_enroll_busy() {
    let temp = TempDir::new("enroll-busy");
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            denoise_delay: Duration::from_millis(400),
            ..Setup::default()
        },
    );
    let pcm = speech_pcm(2.0, 0.3);
    let first = send(&mut daemon, add_cmd(&pcm, "mic-a"));
    let second = send(&mut daemon, add_cmd(&pcm, "mic-a"));
    let third = send(&mut daemon, add_cmd(&pcm, "mic-a"));
    let (a, b) = (job_id(&first), job_id(&second));
    assert_ne!(third.status, IpcStatus::Ok);
    assert_eq!(error_code(&third), ENROLL_BUSY);
    assert_eq!(wait_job(&mut daemon, &a)["state"], "done");
    assert_eq!(wait_job(&mut daemon, &b)["state"], "done");
    assert_eq!(list(&mut daemon)["total_count"], 2);
    // Slots are free again once the jobs were drained.
    assert_eq!(
        send(&mut daemon, add_cmd(&pcm, "mic-a")).status,
        IpcStatus::Ok
    );
}

// ---------------------------------------------------------------- takes

#[test]
fn take_with_four_seconds_of_margin_is_silently_not_recorded() {
    let temp = TempDir::new("enroll-take-margin");
    let mut daemon = daemon(temp.path());
    seed_sample(&mut daemon, "seed-86", 1, "mic-a", 86.0);
    let resp = send(&mut daemon, take_cmd(&speech_pcm(3.0, 0.3), "mic-a"));
    let job = wait_job(&mut daemon, &job_id(&resp));
    assert_eq!(job["state"], "done", "{job}");
    assert!(job["error_code"].is_null());
    assert_eq!(job["recorded"], false);
    assert_eq!(job["reason"], "budget");
    let pending = send(&mut daemon, IpcCommand::ListIntakeSuggestions);
    assert_eq!(pending.payload["count"], 0);
    assert!(
        !temp.path().join("samples").exists()
            || files_under(&temp.path().join("samples")).is_empty()
    );
}

#[test]
fn take_longer_than_the_margin_is_not_recorded() {
    let temp = TempDir::new("enroll-take-long");
    let mut daemon = daemon(temp.path());
    seed_sample(&mut daemon, "seed-80", 1, "mic-a", 80.0);
    let resp = send(&mut daemon, take_cmd(&speech_pcm(11.0, 0.3), "mic-a"));
    let job = wait_job(&mut daemon, &job_id(&resp));
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(job["recorded"], false);
    assert_eq!(
        send(&mut daemon, IpcCommand::ListIntakeSuggestions).payload["count"],
        0
    );

    // A take that fits the remaining 10 s is recorded.
    let fits = send(&mut daemon, take_cmd(&speech_pcm(6.0, 0.3), "mic-a"));
    let job = wait_job(&mut daemon, &job_id(&fits));
    assert_eq!(job["recorded"], true, "{job}");
    assert_eq!(
        send(&mut daemon, IpcCommand::ListIntakeSuggestions).payload["count"],
        1
    );
}

#[test]
fn approving_a_take_over_budget_keeps_the_take() {
    let temp = TempDir::new("enroll-approve");
    let mut daemon = daemon(temp.path());
    let resp = send(&mut daemon, take_cmd(&speech_pcm(4.0, 0.3), "mic-a"));
    let job = wait_job(&mut daemon, &job_id(&resp));
    assert_eq!(job["recorded"], true, "{job}");
    let take_id = job["take_id"].as_str().expect("take id").to_owned();
    let listed = send(&mut daemon, IpcCommand::ListIntakeSuggestions);
    assert!(listed.payload["suggestions"][0].get("audio_path").is_none());

    seed_sample(&mut daemon, "seed-88", 1, "mic-a", 88.0);
    let approve = IpcCommand::ApproveIntakeSuggestion {
        id: take_id.clone(),
        name: None,
    };
    let refused = send(&mut daemon, approve.clone());
    assert_eq!(error_code(&refused), ENROLL_BUDGET_EXCEEDED);
    assert_eq!(
        send(&mut daemon, IpcCommand::ListIntakeSuggestions).payload["count"],
        1,
        "the take stays pending"
    );

    // Deleting audio frees the budget; the approval then moves the take into the gallery.
    send(
        &mut daemon,
        IpcCommand::DeleteVoiceSample {
            id: "seed-88".to_owned(),
        },
    );
    let ok = send(&mut daemon, approve);
    assert_eq!(ok.status, IpcStatus::Ok, "{ok:?}");
    assert_eq!(ok.payload["sample_id"], take_id.as_str());
    assert_eq!(
        send(&mut daemon, IpcCommand::ListIntakeSuggestions).payload["count"],
        0
    );
    let item = &list(&mut daemon)["samples"][0];
    assert_eq!(item["id"], take_id.as_str());
    assert_eq!(item["needs_reenroll"], false);
    assert!(
        temp.path()
            .join("samples")
            .join(format!("{take_id}.wav"))
            .is_file()
    );
}

#[test]
fn discard_take_never_deletes_a_file_outside_samples() {
    let temp = TempDir::new("enroll-discard-sentinel");
    let mut daemon = daemon(&temp.path().join("profiles"));
    let sentinel = temp.path().join("sentinel.txt");
    std::fs::write(&sentinel, b"do not delete").expect("sentinel");
    let take = IntakeTake::new(
        "legacy-take",
        "1",
        4.0,
        10.0,
        Some(sentinel.to_string_lossy().into_owned()),
        vec![0.1; 192],
    )
    .expect("take");
    daemon.voice_intake_mut().add_take(take).expect("seed take");
    let resp = send(
        &mut daemon,
        IpcCommand::DiscardIntakeSuggestion {
            id: "legacy-take".to_owned(),
        },
    );
    assert_eq!(resp.payload["discarded"], true);
    assert!(
        sentinel.is_file(),
        "a file outside samples/ survives a discard"
    );

    // A recorded take's WAV (inside samples/) is deleted with it.
    let recorded = send(&mut daemon, take_cmd(&speech_pcm(4.0, 0.3), "mic-a"));
    let job = wait_job(&mut daemon, &job_id(&recorded));
    let take_id = job["take_id"].as_str().expect("take id").to_owned();
    let wav = temp
        .path()
        .join("profiles")
        .join("samples")
        .join(format!("{take_id}.wav"));
    assert!(wav.is_file());
    send(
        &mut daemon,
        IpcCommand::DiscardIntakeSuggestion { id: take_id },
    );
    assert!(!wav.exists());
}

#[test]
fn legacy_take_json_is_rejected_without_echo() {
    let temp = TempDir::new("enroll-legacy-take-json");
    let mut daemon = daemon(temp.path());
    let take = IntakeTake::new("SECRET_TAKE", "1", 4.0, 1.0, None, vec![0.1; 192]).expect("take");
    let resp = send(
        &mut daemon,
        IpcCommand::AddIntakeSuggestion {
            take_json: serde_json::to_string(&take).expect("json"),
        },
    );
    assert_eq!(resp.status, IpcStatus::InvalidCommand);
    assert!(!resp.to_json().expect("json").contains("SECRET_TAKE"));
}

// ---------------------------------------------------------------- build

fn build(daemon: &mut ServiceDaemon) -> IpcResponse {
    send(
        daemon,
        IpcCommand::BuildVoiceProfile {
            name: "Joao".to_owned(),
        },
    )
}

#[test]
fn build_profile_applies_it_through_the_transaction() {
    let temp = TempDir::new("enroll-build");
    let mut daemon = daemon(temp.path());
    for _ in 0..3 {
        assert_eq!(add_sample(&mut daemon, 4.0, "mic-a")["state"], "done");
    }
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "done", "{job}");
    let profile_id = job["profile_id"].as_str().expect("profile id").to_owned();
    assert!(profile_id.starts_with("p-"));

    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["is_voice_profile_active"], true);
    assert_eq!(
        status.payload["active_voice_profile_id"],
        profile_id.as_str()
    );
    assert_eq!(
        status.payload["stored_voice_profile_id"],
        profile_id.as_str()
    );
    assert_eq!(status.payload["has_voice_profile"], true);
    assert!(status.payload.get("embedding").is_none());
    let stored = temp.path().join(ACTIVE_PROFILE_FILE_NAME);
    assert!(stored.is_file());
    #[cfg(unix)]
    assert_eq!(mode_of(&stored), 0o600);
    assert_eq!(list(&mut daemon)["has_profile"], true);
}

#[test]
fn build_without_the_development_model_fails_closed() {
    let temp = TempDir::new("enroll-no-model");
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            model_delay: None,
            ..Setup::default()
        },
    );
    let previous = test_profile("previous");
    let set = send(
        &mut daemon,
        IpcCommand::SetVoiceProfile {
            profile_json: previous.to_json().expect("json"),
        },
    );
    assert_eq!(set.status, IpcStatus::Ok);
    for _ in 0..2 {
        add_sample(&mut daemon, 4.0, "mic-a");
    }
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], ENROLL_MODEL_NOT_CONFIGURED);
    assert!(job["profile_id"].is_null());
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], "previous");
    assert_eq!(status.payload["stored_voice_profile_id"], "previous");
}

#[test]
fn samples_of_other_microphones_are_excluded_from_the_profile() {
    let temp = TempDir::new("enroll-other-mic");
    let mut daemon = daemon(temp.path());
    add_sample(&mut daemon, 4.0, "mic-a");
    add_sample(&mut daemon, 4.0, "mic-a");
    std::thread::sleep(Duration::from_millis(3));
    add_sample(&mut daemon, 4.0, "mic-b");
    let resp = build(&mut daemon);
    assert_eq!(error_code(&resp), ENROLL_TOO_LITTLE_SPEECH, "{resp:?}");

    let listed = list(&mut daemon);
    let items = listed["samples"].as_array().expect("samples");
    for item in items {
        let is_b = item["device_label"] == "Mic mic-b";
        assert_eq!(item["other_microphone"], !is_b, "{item}");
        assert_eq!(item["used_in_profile"], is_b, "{item}");
    }
    let used = listed["budget"]["used_seconds"].as_f64().expect("used");
    assert!(
        used > 3.5 && used < 4.5,
        "budget of the selected group only: {used}"
    );
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert!(status.payload["active_voice_profile_id"].is_null());
}

#[test]
fn a_single_four_second_sample_is_too_little_speech() {
    let temp = TempDir::new("enroll-single");
    let mut daemon = daemon(temp.path());
    add_sample(&mut daemon, 4.0, "mic-a");
    assert_eq!(error_code(&build(&mut daemon)), ENROLL_TOO_LITTLE_SPEECH);
    // An empty gallery too.
    let temp2 = TempDir::new("enroll-empty");
    let mut empty = daemon_with(temp2.path(), Setup::default()).0;
    assert_eq!(error_code(&build(&mut empty)), ENROLL_TOO_LITTLE_SPEECH);
}

#[test]
fn job_result_is_applied_on_the_next_request_and_the_loop_is_not_blocked() {
    let temp = TempDir::new("enroll-nonblocking");
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            model_delay: Some(Duration::from_millis(300)),
            ..Setup::default()
        },
    );
    add_sample(&mut daemon, 4.0, "mic-a");
    add_sample(&mut daemon, 4.0, "mic-a");
    let start = Instant::now();
    let id = job_id(&build(&mut daemon));
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert!(
        start.elapsed() < Duration::from_millis(250),
        "build and status must not wait for the model: {:?}",
        start.elapsed()
    );
    assert_eq!(status.status, IpcStatus::Ok);
    assert!(status.payload["active_voice_profile_id"].is_null());
    let running = send(
        &mut daemon,
        IpcCommand::GetEnrollmentJob { job_id: id.clone() },
    );
    assert_eq!(running.payload["state"], "running");
    let done = wait_job(&mut daemon, &id);
    assert_eq!(done["state"], "done", "{done}");
    // Applied by the drain that ran before the request that observed `done`.
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(
        status.payload["active_voice_profile_id"],
        done["profile_id"]
    );
}

#[test]
fn leveling_keeps_very_different_samples_under_the_ceiling() {
    let temp = TempDir::new("enroll-leveling");
    let (mut daemon, seen_peak) = daemon_with(temp.path(), Setup::default());
    for scale in [0.95, 0.95, 0.12] {
        // Raw peaks stay under 0.99; the stored levels differ by about 18 dB.
        let resp = send(&mut daemon, add_cmd(&speech_pcm(4.0, scale), "mic-a"));
        assert_eq!(wait_job(&mut daemon, &job_id(&resp))["state"], "done");
    }
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "done", "{job}");
    let peak = seen_peak.lock().unwrap().expect("model ran");
    assert!(peak <= 0.99, "joined peak {peak}");
}

// ---------------------------------------------------------------- privacy and migration

#[test]
fn no_raw_audio_is_written_anywhere() {
    let temp = TempDir::new("enroll-no-raw");
    let mut daemon = daemon(temp.path());
    let raw = speech_pcm(3.0, 0.3);
    let resp = send(&mut daemon, add_cmd(&raw, "mic-a"));
    let job = wait_job(&mut daemon, &job_id(&resp));
    let sample_id = job["sample_id"].as_str().expect("sample id").to_owned();

    let raw_bytes: Vec<u8> = raw[1_000..1_016]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let mut audio_files = Vec::new();
    for path in files_under(temp.path()) {
        let bytes = std::fs::read(&path).expect("read");
        assert!(
            !bytes
                .windows(raw_bytes.len())
                .any(|w| w == raw_bytes.as_slice()),
            "raw PCM bytes found in {}",
            path.display()
        );
        if bytes.starts_with(b"RIFF") || path.extension().is_some_and(|e| e == "wav") {
            audio_files.push(path);
        }
    }
    let expected = temp.path().join("samples").join(format!("{sample_id}.wav"));
    assert_eq!(audio_files, vec![expected.clone()]);
    let (stored, rate) =
        decode_wav_pcm16_mono(&std::fs::read(&expected).expect("wav")).expect("decode");
    assert_eq!(rate, 48_000);
    assert_eq!(stored.len(), raw.len());
    let tolerance = 1.0 / 32_768.0 + 1e-6;
    assert!(
        stored
            .iter()
            .zip(&raw)
            .all(|(s, r)| (s - r * 0.5).abs() <= tolerance)
    );
    assert!(stored.iter().zip(&raw).any(|(s, r)| (s - r).abs() > 0.01));
}

fn legacy_dir_with_two(root: &Path) -> PathBuf {
    let legacy = root.join("legacy");
    let mut manager = VoiceSampleManager::new(&legacy);
    for id in ["old-1", "old-2"] {
        let mut sample = VoiceSample::new(id, "1", id, Some("blob:x".to_owned()), vec![0.5; 192])
            .expect("legacy sample");
        sample.speech_seconds = 3.0;
        manager.add_sample(sample).expect("legacy add");
    }
    legacy
}

#[test]
fn legacy_migration_runs_once_and_a_deleted_sample_stays_deleted() {
    let temp = TempDir::new("enroll-migration");
    let legacy = legacy_dir_with_two(temp.path());
    let store = temp.path().join("store");
    let setup = || Setup {
        legacy: Some(legacy.clone()),
        ..Setup::default()
    };
    let (mut daemon, _) = daemon_with(&store, setup());
    let listed = list(&mut daemon);
    assert_eq!(listed["total_count"], 2);
    for item in listed["samples"].as_array().expect("samples") {
        assert_eq!(item["needs_reenroll"], true);
        assert_eq!(item["used_in_profile"], false);
    }
    assert_eq!(listed["budget"]["used_seconds"], 0.0);
    let deleted = send(
        &mut daemon,
        IpcCommand::DeleteVoiceSample {
            id: "old-1".to_owned(),
        },
    );
    assert_eq!(deleted.payload["deleted"], true);
    drop(daemon);

    let (mut restarted, _) = daemon_with(&store, setup());
    let listed = list(&mut restarted);
    assert_eq!(listed["total_count"], 1);
    assert_eq!(listed["samples"][0]["id"], "old-2");
    assert!(
        legacy.join("voice_samples.json").is_file(),
        "legacy files are kept"
    );
}

#[test]
fn unreadable_legacy_data_never_stops_the_daemon() {
    let temp = TempDir::new("enroll-migration-bad");
    let legacy = temp.path().join("legacy");
    std::fs::create_dir_all(&legacy).expect("legacy");
    std::fs::write(legacy.join("voice_samples.json"), b"{not json").expect("bad legacy");
    let (mut daemon, _) = daemon_with(
        &temp.path().join("store"),
        Setup {
            legacy: Some(legacy),
            ..Setup::default()
        },
    );
    assert_eq!(list(&mut daemon)["total_count"], 0);
    assert_eq!(
        send(&mut daemon, IpcCommand::GetStatus).status,
        IpcStatus::Ok
    );
}

#[test]
fn listing_exposes_the_selected_device_group() {
    let temp = TempDir::new("enroll-selected-group");
    let mut daemon = daemon(temp.path());
    let empty = list(&mut daemon);
    assert!(empty["selected_device_id_hash"].is_null());
    assert!(empty["selected_device_label"].is_null());

    add_sample(&mut daemon, 3.0, "hash-a");
    add_sample(&mut daemon, 3.0, "hash-a");
    std::thread::sleep(Duration::from_millis(3));
    add_sample(&mut daemon, 3.0, "hash-b");
    let listed = list(&mut daemon);
    assert_eq!(listed["selected_device_id_hash"], "hash-b");
    assert_eq!(listed["selected_device_label"], "Mic hash-b");
    let items = listed["samples"].as_array().expect("samples");
    let hashes: Vec<&str> = items
        .iter()
        .map(|i| i["device_id_hash"].as_str().expect("hash"))
        .collect();
    assert_eq!(hashes, vec!["hash-a", "hash-a", "hash-b"]);
}

#[test]
fn without_a_usable_denoiser_samples_and_takes_fail_synchronously() {
    let temp = TempDir::new("enroll-no-denoiser");
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            denoise_gain: None,
            ..Setup::default()
        },
    );
    for cmd in [
        add_cmd(&speech_pcm(2.0, 0.3), "mic-a"),
        take_cmd(&speech_pcm(2.0, 0.3), "mic-a"),
        // Checked before the base64 is decoded.
        IpcCommand::AddVoiceSample {
            name: "n".into(),
            pcm_f32_le_b64: "not-base64!".into(),
            sample_rate: 48_000,
            device_label: "Mic".into(),
            device_id_hash: "h".into(),
        },
    ] {
        let resp = send(&mut daemon, cmd);
        assert_eq!(error_code(&resp), ENROLL_MODEL_NOT_CONFIGURED, "{resp:?}");
        assert!(resp.payload.get("job_id").is_none());
    }
    let none = send(
        &mut daemon,
        IpcCommand::GetEnrollmentJob {
            job_id: "sample-job-1".to_owned(),
        },
    );
    assert_eq!(
        error_code(&none),
        ENROLL_JOB_NOT_FOUND,
        "no job was created"
    );
    assert_eq!(list(&mut daemon)["total_count"], 0);
    assert!(!temp.path().join("samples").exists());
}

// ---------------------------------------------------------------- stale build results

fn slow_build_daemon(label: &str) -> (TempDir, ServiceDaemon) {
    let temp = TempDir::new(label);
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            model_delay: Some(Duration::from_millis(300)),
            ..Setup::default()
        },
    );
    add_sample(&mut daemon, 4.0, "mic-a");
    add_sample(&mut daemon, 4.0, "mic-a");
    (temp, daemon)
}

#[test]
fn clearing_the_profile_during_a_build_discards_its_result() {
    let (_temp, mut daemon) = slow_build_daemon("enroll-stale-clear");
    let id = job_id(&build(&mut daemon));
    assert_eq!(
        send(&mut daemon, IpcCommand::ClearVoiceProfile).status,
        IpcStatus::Ok
    );
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], "ENROLL_FAILED");
    assert_eq!(job["stage"], "apply");
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert!(status.payload["active_voice_profile_id"].is_null());
    assert!(status.payload["stored_voice_profile_id"].is_null());
}

#[test]
fn deleting_a_group_sample_during_a_build_discards_its_result() {
    let (_temp, mut daemon) = slow_build_daemon("enroll-stale-delete");
    let first = list(&mut daemon)["samples"][0]["id"]
        .as_str()
        .expect("id")
        .to_owned();
    let id = job_id(&build(&mut daemon));
    let deleted = send(&mut daemon, IpcCommand::DeleteVoiceSample { id: first });
    assert_eq!(deleted.payload["deleted"], true);
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], "ENROLL_FAILED");
    // "apply" when the WAVs were read before the delete (stale result), "trim" when the
    // worker found the WAV already gone; either way nothing is applied.
    assert!(job["stage"] == "apply" || job["stage"] == "trim", "{job}");
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert!(status.payload["active_voice_profile_id"].is_null());
}

#[test]
fn only_one_build_runs_at_a_time() {
    let (_temp, mut daemon) = slow_build_daemon("enroll-one-build");
    let first = job_id(&build(&mut daemon));
    let second = build(&mut daemon);
    assert_eq!(error_code(&second), ENROLL_BUSY, "{second:?}");
    let job = wait_job(&mut daemon, &first);
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(build(&mut daemon).status, IpcStatus::Ok, "free again");
}

#[test]
fn a_profile_the_backend_rejects_fails_the_job_and_keeps_the_previous_one() {
    let temp = TempDir::new("enroll-apply-fails");
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            reject_built: true,
            ..Setup::default()
        },
    );
    let set = send(
        &mut daemon,
        IpcCommand::SetVoiceProfile {
            profile_json: test_profile("previous").to_json().expect("json"),
        },
    );
    assert_eq!(set.status, IpcStatus::Ok);
    add_sample(&mut daemon, 4.0, "mic-a");
    add_sample(&mut daemon, 4.0, "mic-a");
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], "ENROLL_FAILED");
    assert_eq!(job["stage"], "apply");
    let status = send(&mut daemon, IpcCommand::GetStatus);
    assert_eq!(status.payload["active_voice_profile_id"], "previous");
    assert_eq!(status.payload["stored_voice_profile_id"], "previous");
    let stored = ProfileStore::new(temp.path()).load_active().expect("load");
    assert_eq!(stored.expect("stored").id, "previous");
}

// ---------------------------------------------------------------- peaks after decimation

/// `seconds` of 300 ms bursts separated by 100 ms of silence, normalized to `peak`.
fn bursts(seconds: f32, peak: f32, mut source: impl FnMut(usize) -> f32) -> Vec<f32> {
    let n = (seconds * 48_000.0) as usize;
    let mut out: Vec<f32> = (0..n)
        .map(|i| if i % 19_200 < 14_400 { source(i) } else { 0.0 })
        .collect();
    let max = out.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    for v in &mut out {
        *v *= peak / max;
    }
    out
}

fn square_bursts(seconds: f32, peak: f32) -> Vec<f32> {
    bursts(seconds, peak, |i| {
        if (i / 24) % 2 == 0 { 1.0 } else { -1.0 } // 1 kHz square at 48 kHz
    })
}

fn pink_bursts(seconds: f32, peak: f32) -> Vec<f32> {
    let mut seed = 0x1234_5678_u32;
    let mut b = [0.0_f32; 3];
    bursts(seconds, peak, move |_| {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let white = (seed >> 8) as f32 / 8_388_608.0 - 1.0;
        b[0] = 0.997_65f32.mul_add(b[0], white * 0.099_046);
        b[1] = 0.963f32.mul_add(b[1], white * 0.296_516_4);
        b[2] = 0.57f32.mul_add(b[2], white * 1.052_691_3);
        b[0] + b[1] + b[2] + white * 0.184_8
    })
}

#[test]
fn broadband_transients_stay_under_the_ceiling_after_decimation() {
    let temp = TempDir::new("enroll-transients");
    let (mut daemon, seen_peak) = daemon_with(
        temp.path(),
        Setup {
            denoise_gain: Some(1.0),
            ..Setup::default()
        },
    );
    // The square bursts set the median level, so they keep their 0.985 peak through the level
    // matching; their odd harmonics ring after the 48 -> 16 kHz low-pass (Gibbs overshoot).
    for pcm in [
        square_bursts(4.0, 0.985),
        pink_bursts(4.0, 0.985),
        square_bursts(4.0, 0.985),
        speech_pcm(4.0, 0.05),
        square_bursts(4.0, 0.985),
    ] {
        let resp = send(&mut daemon, add_cmd(&pcm, "mic-a"));
        let job = wait_job(&mut daemon, &job_id(&resp));
        assert_eq!(job["state"], "done", "{job}");
    }
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "done", "{job}");
    let peak = seen_peak.lock().unwrap().expect("model ran");
    assert!(peak <= 0.99, "16 kHz peak seen by the model: {peak}");
}

// ---------------------------------------------------------------- persistence failures

#[cfg(unix)]
#[test]
fn a_failed_manifest_write_leaves_no_phantom_sample_or_take() {
    use std::os::unix::fs::PermissionsExt;
    let temp = TempDir::new("enroll-persist-fails");
    let mut daemon = daemon(temp.path());
    assert_eq!(add_sample(&mut daemon, 3.0, "mic-a")["state"], "done");
    let used_before = list(&mut daemon)["budget"]["used_seconds"].clone();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o500)).expect("ro");

    let job = add_sample(&mut daemon, 3.0, "mic-a");
    let take = send(&mut daemon, take_cmd(&speech_pcm(3.0, 0.3), "mic-a"));
    let take_job = wait_job(&mut daemon, &job_id(&take));
    let listed = list(&mut daemon);
    let pending = send(&mut daemon, IpcCommand::ListIntakeSuggestions);
    let wavs = files_under(&temp.path().join("samples")).len();
    std::fs::set_permissions(temp.path(), std::fs::Permissions::from_mode(0o700)).expect("rw");

    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], "ENROLL_FAILED");
    assert_eq!(take_job["state"], "failed", "{take_job}");
    assert_eq!(listed["total_count"], 1, "no phantom sample in memory");
    assert_eq!(listed["budget"]["used_seconds"], used_before);
    assert_eq!(pending.payload["count"], 0, "no phantom take in memory");
    assert_eq!(wavs, 1, "the freshly written WAVs were removed");
}

#[test]
fn an_empty_device_hash_is_refused_synchronously() {
    let temp = TempDir::new("enroll-empty-hash");
    let mut daemon = daemon(temp.path());
    for cmd in [
        add_cmd(&speech_pcm(2.0, 0.3), ""),
        take_cmd(&speech_pcm(2.0, 0.3), ""),
        add_cmd(&speech_pcm(2.0, 0.3), "   "),
    ] {
        let resp = send(&mut daemon, cmd);
        assert_eq!(error_code(&resp), ENROLL_INVALID_AUDIO, "{resp:?}");
        assert!(resp.payload.get("job_id").is_none());
    }
    assert_eq!(list(&mut daemon)["total_count"], 0);
}

// ---------------------------------------------------------------- reviewer coverage

/// Stores a real WAV at `samples/<id>.wav` and records the sample with `audio_path`.
fn seed_wav_sample(
    daemon: &mut ServiceDaemon,
    id: &str,
    ts: u64,
    seconds: f32,
    audio_path: Option<String>,
) -> PathBuf {
    let wav = realtime_noise_model::wav::encode_wav_pcm16_mono(&speech_pcm(seconds, 0.15), 48_000);
    let path = daemon
        .voice_samples()
        .write_sample_wav(id, &wav)
        .expect("write wav");
    let sample = VoiceSample {
        id: id.to_owned(),
        timestamp: ts.to_string(),
        name: "seed".to_owned(),
        audio_path: Some(audio_path.unwrap_or_else(|| path.to_string_lossy().into_owned())),
        embedding: Vec::new(),
        is_active: true,
        device_label: "Mic mic-a".to_owned(),
        device_id_hash: "mic-a".to_owned(),
        capture_sample_rate: 48_000,
        speech_seconds: seconds,
    };
    daemon.voice_samples_mut().add_sample(sample).expect("seed");
    path
}

#[test]
fn concurrent_ingestions_that_fit_alone_but_not_together_store_exactly_one() {
    let temp = TempDir::new("enroll-concurrent-budget");
    let (mut daemon, _) = daemon_with(
        temp.path(),
        Setup {
            denoise_delay: Duration::from_millis(150),
            ..Setup::default()
        },
    );
    seed_sample(&mut daemon, "seed-80", 1, "mic-a", 80.0);
    let pcm = speech_pcm(6.0, 0.3);
    let a = job_id(&send(&mut daemon, add_cmd(&pcm, "mic-a")));
    let b = job_id(&send(&mut daemon, add_cmd(&pcm, "mic-a")));
    let (ja, jb) = (wait_job(&mut daemon, &a), wait_job(&mut daemon, &b));
    let states = [ja["state"].clone(), jb["state"].clone()];
    assert!(
        states.contains(&json!("done")) && states.contains(&json!("failed")),
        "{ja} {jb}"
    );
    let failed = if ja["state"] == "failed" { &ja } else { &jb };
    assert_eq!(failed["error_code"], ENROLL_BUDGET_EXCEEDED);
    let listed = list(&mut daemon);
    assert_eq!(listed["total_count"], 2);
    assert!(listed["budget"]["used_seconds"].as_f64().expect("used") <= 90.0 + 1e-3);
}

#[test]
fn a_migrated_group_over_ninety_seconds_fails_without_dropping_audio() {
    let temp = TempDir::new("enroll-over-ninety");
    let mut daemon = daemon(temp.path());
    for (i, id) in ["m-1", "m-2", "m-3"].iter().enumerate() {
        seed_wav_sample(&mut daemon, id, i as u64 + 1, 31.0, None);
    }
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "failed", "{job}");
    assert_eq!(job["error_code"], ENROLL_BUDGET_EXCEEDED);
    assert_eq!(
        list(&mut daemon)["total_count"],
        3,
        "no sample was discarded"
    );
    assert_eq!(files_under(&temp.path().join("samples")).len(), 3);
    assert!(send(&mut daemon, IpcCommand::GetStatus).payload["active_voice_profile_id"].is_null());
}

#[test]
fn deleting_an_ingested_sample_removes_its_wav() {
    let temp = TempDir::new("enroll-delete-wav");
    let mut daemon = daemon(temp.path());
    let job = add_sample(&mut daemon, 3.0, "mic-a");
    let sample_id = job["sample_id"].as_str().expect("id").to_owned();
    let wav = temp.path().join("samples").join(format!("{sample_id}.wav"));
    assert!(wav.is_file());
    let resp = send(&mut daemon, IpcCommand::DeleteVoiceSample { id: sample_id });
    assert_eq!(resp.payload["deleted"], true);
    assert!(!wav.exists());
}

#[test]
fn the_build_reads_audio_by_id_and_ignores_a_stored_outside_path() {
    let temp = TempDir::new("enroll-outside-path");
    let mut daemon = daemon(&temp.path().join("profiles"));
    let outside = temp.path().join("not-a-wav.bin");
    std::fs::write(&outside, b"garbage that would fail to decode").expect("outside");
    seed_wav_sample(
        &mut daemon,
        "s-by-id",
        1,
        8.0,
        Some(outside.to_string_lossy().into_owned()),
    );
    let id = job_id(&build(&mut daemon));
    let job = wait_job(&mut daemon, &id);
    assert_eq!(job["state"], "done", "{job}");
    assert_eq!(
        std::fs::read(&outside).expect("outside"),
        b"garbage that would fail to decode"
    );
}

#[cfg(unix)]
#[test]
fn symlinks_inside_samples_never_delete_their_targets() {
    let temp = TempDir::new("enroll-symlinks");
    let dir = temp.path().join("profiles");
    let mut daemon = daemon(&dir);
    let sentinel = temp.path().join("sentinel.wav");
    std::fs::write(&sentinel, b"keep").expect("sentinel");

    // A sample whose samples/<id>.wav is a symlink to a file outside.
    seed_wav_sample(&mut daemon, "s-link", 1, 3.0, None);
    let link = dir.join("samples").join("s-link.wav");
    std::fs::remove_file(&link).expect("rm");
    std::os::unix::fs::symlink(&sentinel, &link).expect("symlink");
    let resp = send(
        &mut daemon,
        IpcCommand::DeleteVoiceSample {
            id: "s-link".to_owned(),
        },
    );
    assert_eq!(resp.payload["deleted"], true);
    assert_eq!(std::fs::read(&sentinel).expect("sentinel"), b"keep");

    // A take whose audio path is a symlink inside samples/ pointing outside.
    let take_link = dir.join("samples").join("t-link.wav");
    std::os::unix::fs::symlink(&sentinel, &take_link).expect("symlink");
    let take = IntakeTake::new(
        "t-link",
        "1",
        3.0,
        0.0,
        Some(take_link.to_string_lossy().into_owned()),
        Vec::new(),
    )
    .expect("take");
    daemon.voice_intake_mut().add_take(take).expect("seed take");
    let resp = send(
        &mut daemon,
        IpcCommand::DiscardIntakeSuggestion {
            id: "t-link".to_owned(),
        },
    );
    assert_eq!(resp.payload["discarded"], true);
    assert_eq!(std::fs::read(&sentinel).expect("sentinel"), b"keep");
}
