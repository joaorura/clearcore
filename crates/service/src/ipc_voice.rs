//! Voice enrollment IPC handlers and their background jobs (task S6, spec 4.1-4.4 and 5).
//!
//! Heavy work (denoise, trim/join, EQ, enrollment) runs on job threads; results are drained and
//! applied on the daemon thread at the start of every request, so the IPC loop never blocks and
//! every write to the gallery, the intake queue and the active profile happens on one thread.
//!
//! Privacy: raw PCM lives only in memory ([`Pcm`] zeroes it on drop). Responses and logs carry
//! fixed codes and messages only; never audio, base64, names, sample paths or serde/IO text.

use crate::enrollment_config::EnrollmentConfig;
use crate::enrollment_error::EnrollError;
use crate::enrollment_ingest::{Denoiser, IngestResult, ingest_sample};
use crate::enrollment_jobs::{JobFailure, JobInfo, JobState, JobTable};
use crate::voice_budget::{budget_for, fits_manual, fits_take, selected_device_hash};
use crate::voice_intake::{IntakeTake, VoiceIntakeError};
use crate::voice_samples::{VoiceSample, is_valid_sample_id};
use crate::{ApplyError, ServiceDaemon};
use base64::Engine as _;
use realtime_noise_ipc::enrollment_codes as codes;
use realtime_noise_ipc::{IpcResponse, IpcStatus};
use realtime_noise_model::VoiceProfile;
use realtime_noise_model::enrollment::{
    EnrollmentError, ProfileMetadata, RawFilmVectors, SpeakerEmbeddingModel,
    SpeakerEnrollmentEngine,
};
use realtime_noise_model::enrollment_builder::{BuildError, build_profile};
use realtime_noise_model::resample::decimate_48k_to_16k;
use realtime_noise_model::speech_trim::{
    active_rms_dbfs, apply_gain_limited, join_crossfade, trim_speech,
};
use realtime_noise_model::wav::decode_wav_pcm16_mono;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Capture rate the service accepts (spec 4.1).
pub const CAPTURE_SAMPLE_RATE: u32 = 48_000;
/// Longest raw take accepted in one request.
pub const MAX_PCM_SECONDS: u32 = 90;
const MAX_PCM_BYTES: usize = (MAX_PCM_SECONDS as usize) * (CAPTURE_SAMPLE_RATE as usize) * 4;
/// Base64 length of `MAX_PCM_BYTES` (padded), checked before decoding anything.
const MAX_PCM_B64_LEN: usize = MAX_PCM_BYTES.div_ceil(3) * 4;
/// Longest joined speech the builder hands to the model (spec 4.4).
const MAX_JOINED_SAMPLES: usize = (MAX_PCM_SECONDS as usize) * (CAPTURE_SAMPLE_RATE as usize);
/// A group with less stored speech than this cannot become a profile (spec 4.2 step 0).
pub const MIN_GROUP_SPEECH_SECONDS: f32 = 6.0;
/// Upper bound of names, labels and hashes kept in the manifests.
const MAX_METADATA_BYTES: usize = 256;
/// Upper bound of one stored sample WAV read back at build time (90 s at 16 bit + header).
const MAX_SAMPLE_WAV_BYTES: u64 = 16 * 1024 * 1024;
/// Upper bound of the development enrollment archive read from disk.
const MAX_DEV_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
/// Level matching: largest gain change applied to one sample (spec 4.2 step 1).
const MAX_LEVEL_GAIN_DB: f32 = 12.0;
/// Fixed service-log reasons of a built profile that was not applied (diagnostics only; they
/// never carry the profile, its name or the backend error text).
const BUILT_PROFILE_NOT_APPLICABLE_LOG: &str =
    "Built voice profile not applied: the active backend does not accept voice profiles";
const BUILT_PROFILE_NO_STORE_LOG: &str =
    "Built voice profile not applied: no profile store is attached";
const BUILT_PROFILE_PERSIST_FAILED_LOG: &str =
    "Built voice profile not applied: it could not be persisted; the previous profile was kept";
/// Peak ceiling of the leveled, joined 48 kHz audio. It sits below the validator's 0.99 because
/// the 48 -> 16 kHz low-pass rings on broadband transients (a full-scale square wave overshoots
/// by about 9 %, Gibbs); 0.9 keeps the 16 kHz signal the model sees under 0.99.
const LEVELING_PEAK_CEILING: f32 = 0.9;
/// Ceiling of the 16 kHz signal the model will see. 0.9 is not enough for every transient (a
/// truncated square wave rings to about 1.18x), so the joined take is also decimated once to
/// measure it and scaled down when it would exceed this (never up).
const DECIMATED_PEAK_CEILING: f32 = 0.97;
/// Safety margin under the headroom limit (absorbs float rounding of the dB round trip).
const HEADROOM_MARGIN_DB: f32 = 0.01;
const JOIN_CROSSFADE_MS: u32 = 20;

/// A job still running after this is failed (`stage: "timeout"`) and its slot freed; the
/// worker thread itself cannot be stopped and its late result is discarded.
const JOB_WATCHDOG: std::time::Duration = std::time::Duration::from_secs(10 * 60);

const SAMPLE_JOB_PREFIX: &str = "sample-";
const PROFILE_JOB_PREFIX: &str = "profile-";

/// Builds a fresh denoiser for one sample (test seam; production is the base `DFNet3`).
pub type DenoiserFactory = Box<dyn Fn() -> Box<dyn Denoiser> + Send + Sync>;
/// Loads the enrollment model on a job thread (test seam; production is the dev archive).
pub type ModelFactory =
    Box<dyn Fn() -> Result<Box<dyn SpeakerEmbeddingModel + Send>, EnrollError> + Send + Sync>;
type SharedModelFactory =
    Arc<dyn Fn() -> Result<Box<dyn SpeakerEmbeddingModel + Send>, EnrollError> + Send + Sync>;

/// Test seams for [`ServiceDaemon::with_enrollment_hooks`]. A daemon built with hooks is
/// hermetic: it never reads the enrollment environment variables nor `$HOME`.
#[doc(hidden)]
#[derive(Default)]
pub struct EnrollmentHooks {
    /// `None` keeps the production denoiser (base `DFNet3` through tract, needs the repo root).
    pub denoiser_factory: Option<DenoiserFactory>,
    /// `None` keeps the production loader driven by `dev_enrollment_asset`.
    pub model_factory: Option<ModelFactory>,
    /// Archive path and expected SHA-256 for the production loader; `None` = not configured.
    pub dev_enrollment_asset: Option<(PathBuf, String)>,
    /// Legacy samples directory migrated once by `attach_profile_store`; `None` = no migration.
    pub legacy_samples_dir: Option<PathBuf>,
}

/// Zeroes the PCM when dropped: raw audio is wiped on every path, including a rejected spawn.
struct Pcm(Vec<f32>);

impl Drop for Pcm {
    fn drop(&mut self) {
        self.0.fill(0.0);
        std::hint::black_box(&self.0);
    }
}

/// Feeds a boxed model to `SpeakerEnrollmentEngine`, which is generic over the model.
struct BoxedModel(Box<dyn SpeakerEmbeddingModel + Send>);

impl SpeakerEmbeddingModel for BoxedModel {
    fn extract(&mut self, samples_16khz: &[f32]) -> Result<RawFilmVectors, EnrollmentError> {
        self.0.extract(samples_16khz)
    }
}

#[derive(Debug, Clone)]
enum IngestKind {
    Manual {
        name: String,
        device_label: String,
        device_id_hash: String,
    },
    Take {
        name: Option<String>,
        device_label: String,
        device_id_hash: String,
    },
}

/// Payload of a finished ingest job, applied on the daemon thread.
pub struct IngestOutcome {
    kind: IngestKind,
    result: IngestResult,
}

impl Drop for IngestOutcome {
    fn drop(&mut self) {
        self.result.wav_bytes.fill(0);
    }
}

#[derive(Debug, Clone, Copy)]
struct Quality {
    peak: f32,
    rms_dbfs: f32,
    active_fraction: f32,
    speech_seconds: f32,
}

/// What a job produced beyond its state, reported by `GetEnrollmentJob`.
#[derive(Debug, Clone, Default)]
struct JobExtra {
    sample_id: Option<String>,
    profile_id: Option<String>,
    take_id: Option<String>,
    quality: Option<Quality>,
    recorded: Option<bool>,
    reason: Option<&'static str>,
}

/// Enrollment state owned by the daemon thread.
pub struct EnrollmentState {
    sample_jobs: JobTable<IngestOutcome>,
    /// Build results carry the `profile_generation` captured at spawn.
    profile_jobs: JobTable<(u64, VoiceProfile)>,
    extras: HashMap<String, JobExtra>,
    denoiser_factory: Option<DenoiserFactory>,
    model_factory: SharedModelFactory,
    pub(crate) legacy_samples_dir: Option<PathBuf>,
    id_counter: u64,
    /// Bumped by every change that makes a running build stale (profile set/cleared, a sample
    /// added, deleted or approved, a new build). A build result whose captured generation differs
    /// is discarded, so a profile is never built from deleted audio, never revives a cleared
    /// profile and never overrides a newer one.
    profile_generation: u64,
    /// The single build allowed to run at a time.
    running_build: Option<String>,
}

impl EnrollmentState {
    /// Production state: development model from the environment, given legacy directory.
    pub(crate) fn from_env(legacy_samples_dir: Option<PathBuf>) -> Self {
        Self::with(EnrollmentConfig::from_env(), None, None, legacy_samples_dir)
    }

    fn with(
        config: EnrollmentConfig,
        denoiser_factory: Option<DenoiserFactory>,
        model_factory: Option<ModelFactory>,
        legacy_samples_dir: Option<PathBuf>,
    ) -> Self {
        let model_factory: SharedModelFactory = match model_factory {
            Some(factory) => Arc::from(factory),
            None => Arc::new(move || load_dev_model(&config)),
        };
        Self {
            sample_jobs: JobTable::new(),
            profile_jobs: JobTable::new(),
            extras: HashMap::new(),
            denoiser_factory,
            model_factory,
            legacy_samples_dir,
            id_counter: 0,
            profile_generation: 0,
            running_build: None,
        }
    }

    pub(crate) const fn bump_generation(&mut self) {
        self.profile_generation = self.profile_generation.wrapping_add(1);
    }

    fn build_running(&self) -> bool {
        self.running_build
            .as_deref()
            .and_then(|id| self.profile_jobs.info(id))
            .is_some_and(|info| info.state == JobState::Running)
    }

    fn next_id(&mut self, prefix: &str) -> String {
        self.id_counter = self.id_counter.wrapping_add(1);
        format!("{prefix}-{}-{}", epoch_millis(), self.id_counter)
    }
}

fn load_dev_model(
    config: &EnrollmentConfig,
) -> Result<Box<dyn SpeakerEmbeddingModel + Send>, EnrollError> {
    use realtime_noise_model::enrollment::OnnxEnrollmentModel;
    if !config.is_configured() {
        return Err(EnrollError::ModelNotConfigured);
    }
    let Some(ref path) = config.archive_path else {
        return Err(EnrollError::ModelNotConfigured);
    };

    // Direct ONNX model support (embedded repository model)
    if path.extension().is_some_and(|ext| ext == "onnx") {
        let model = OnnxEnrollmentModel::from_file(path)
            .map_err(|_| EnrollError::ModelNotConfigured)?;
        return Ok(Box::new(model));
    }

    let Some(ref sha) = config.expected_sha256 else {
        return Err(EnrollError::ModelNotConfigured);
    };
    // A missing, oversized, tampered or unreadable archive is a configuration problem: the
    // configured asset cannot be used, so the build fails closed with the same code.
    let mut bytes =
        read_capped(path, MAX_DEV_ARCHIVE_BYTES).map_err(|()| EnrollError::ModelNotConfigured)?;
    let model = OnnxEnrollmentModel::from_dev_archive(&bytes, sha)
        .map_err(|_| EnrollError::ModelNotConfigured);
    bytes.fill(0);
    Ok(Box::new(model?))
}

/// Reads a regular file of at most `max` bytes; symlinks and anything else are refused.
fn read_capped(path: &Path, max: u64) -> Result<Vec<u8>, ()> {
    let meta = std::fs::symlink_metadata(path).map_err(|_| ())?;
    if !meta.is_file() || meta.len() > max {
        return Err(());
    }
    let file = std::fs::File::open(path).map_err(|_| ())?;
    let mut bytes = Vec::new();
    file.take(max.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > max {
        bytes.fill(0);
        return Err(());
    }
    Ok(bytes)
}

fn epoch_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

/// `YYYY-MM-DDTHH:MM:SSZ` for the current time (no date crate in the workspace).
fn utc_now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    rfc3339_from_epoch(secs)
}

/// Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
fn rfc3339_from_epoch(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

fn valid_metadata(text: &str, allow_empty: bool) -> bool {
    text.len() <= MAX_METADATA_BYTES && (allow_empty || !text.trim().is_empty())
}

/// Decodes `pcm_f32_le_b64` into 48 kHz mono samples; the decoded bytes are zeroed.
fn decode_pcm(b64: &str, sample_rate: u32) -> Result<Pcm, EnrollError> {
    if sample_rate != CAPTURE_SAMPLE_RATE {
        return Err(EnrollError::InvalidAudio);
    }
    if b64.len() > MAX_PCM_B64_LEN {
        return Err(EnrollError::PayloadTooLarge);
    }
    let mut bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| EnrollError::InvalidAudio)?;
    let checked = if bytes.len() > MAX_PCM_BYTES {
        Err(EnrollError::PayloadTooLarge)
    } else if bytes.is_empty() || bytes.len() % 4 != 0 {
        Err(EnrollError::InvalidAudio)
    } else {
        Ok(())
    };
    let pcm = checked.map(|()| {
        Pcm(bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect())
    });
    bytes.fill(0);
    std::hint::black_box(&bytes);
    pcm
}

const fn status_for(error: &EnrollError) -> IpcStatus {
    match error {
        EnrollError::InvalidAudio
        | EnrollError::PayloadTooLarge
        | EnrollError::JobNotFound
        | EnrollError::BudgetExceeded
        | EnrollError::TooLittleSpeech => IpcStatus::InvalidCommand,
        _ => IpcStatus::InternalError,
    }
}

const fn message_for(error: &EnrollError) -> &'static str {
    match error {
        EnrollError::Clipping => "the recording is clipped",
        EnrollError::TooQuiet => "the recording is too quiet",
        EnrollError::TooLittleSpeech => "not enough speech from this microphone",
        EnrollError::ModelNotConfigured => {
            "the development enrollment model or the base denoiser is not available"
        }
        EnrollError::BudgetExceeded => "the speech budget is full; delete audio first",
        EnrollError::InvalidAudio => "invalid audio payload",
        EnrollError::PayloadTooLarge => "audio payload too large",
        EnrollError::JobNotFound => "enrollment job not found",
        EnrollError::Busy => "enrollment is busy; retry later",
        EnrollError::Failed => "voice enrollment failed",
        EnrollError::BackendUnsupported => {
            "the active isolation model does not accept a voice profile"
        }
    }
}

fn enroll_error(request_id: &str, error: &EnrollError) -> IpcResponse {
    IpcResponse::error(
        request_id,
        status_for(error),
        error.code(),
        message_for(error),
    )
}

fn busy_or_failed(request_id: &str, failure: JobFailure) -> IpcResponse {
    let error = if failure.error_code == codes::ENROLL_BUSY {
        EnrollError::Busy
    } else {
        EnrollError::Failed
    };
    enroll_error(request_id, &error)
}

const fn failure(error_code: &'static str, stage: &'static str) -> JobFailure {
    JobFailure {
        error_code,
        stage,
        remaining_seconds: None,
    }
}

fn map_build_error(error: &BuildError) -> JobFailure {
    match error {
        BuildError::TooMuchSpeech => failure(codes::ENROLL_BUDGET_EXCEEDED, "trim"),
        BuildError::Eq(_) => failure(codes::ENROLL_FAILED, "eq"),
        BuildError::Enroll(inner) => failure(
            match inner {
                EnrollmentError::TooLong { .. } => codes::ENROLL_BUDGET_EXCEEDED,
                EnrollmentError::TooShort { .. } | EnrollmentError::InsufficientSpeech { .. } => {
                    codes::ENROLL_TOO_LITTLE_SPEECH
                }
                EnrollmentError::Clipping { .. } => codes::ENROLL_CLIPPING,
                EnrollmentError::TooQuiet { .. } => codes::ENROLL_TOO_QUIET,
                _ => codes::ENROLL_FAILED,
            },
            "enroll",
        ),
    }
}

/// Gain (dB) that moves a sample toward the group level without ever exceeding the peak
/// ceiling: `desired` is clamped to +-12 dB and then to the headroom `0.9 / peak`.
fn leveling_gain_db(desired_db: f32, peak: f32) -> f32 {
    let wanted = if desired_db.is_finite() {
        desired_db.clamp(-MAX_LEVEL_GAIN_DB, MAX_LEVEL_GAIN_DB)
    } else {
        0.0
    };
    if peak > 0.0 && peak.is_finite() {
        let headroom_db =
            20.0_f32.mul_add((LEVELING_PEAK_CEILING / peak).log10(), -HEADROOM_MARGIN_DB);
        wanted.min(headroom_db)
    } else {
        wanted
    }
}

fn peak_of(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, v| m.max(v.abs()))
}

fn median(values: &mut [f32]) -> f32 {
    values.sort_by(f32::total_cmp);
    let n = values.len();
    match n {
        0 => 0.0,
        _ if n % 2 == 1 => values[n / 2],
        _ => f32::midpoint(values[n / 2 - 1], values[n / 2]),
    }
}

/// Matches every trimmed part to the median active-speech level (gain limited to +-12 dB and by
/// the peak headroom), then joins them with a 20 ms crossfade. `levels[i]` is the active RMS
/// (dBFS) of `parts[i]`. Each crossfade mixes two parts under the ceiling; if the joined peak
/// still exceeds `LEVELING_PEAK_CEILING` the whole take is scaled down to it (never up).
/// Intermediate copies are zeroed.
fn level_and_join(parts: &mut [Vec<f32>], levels: &[f32]) -> Vec<f32> {
    let mut sorted = levels.to_vec();
    let target = median(&mut sorted);
    for (part, level) in parts.iter_mut().zip(levels) {
        let gain = leveling_gain_db(target - level, peak_of(part));
        let leveled = apply_gain_limited(part, gain, MAX_LEVEL_GAIN_DB);
        part.fill(0.0);
        *part = leveled;
    }
    let refs: Vec<&[f32]> = parts.iter().map(Vec::as_slice).collect();
    let mut joined = join_crossfade(&refs, CAPTURE_SAMPLE_RATE, JOIN_CROSSFADE_MS);
    let peak48 = peak_of(&joined);
    scale_down_to(&mut joined, peak48, LEVELING_PEAK_CEILING);
    let mut decimated = decimate_48k_to_16k(&joined);
    let peak16 = peak_of(&decimated);
    decimated.fill(0.0);
    scale_down_to(&mut joined, peak16, DECIMATED_PEAK_CEILING);
    joined
}

/// Scales `samples` so that `measured_peak` becomes `ceiling`, only when it is above it.
fn scale_down_to(samples: &mut [f32], measured_peak: f32, ceiling: f32) {
    if measured_peak > ceiling && measured_peak.is_finite() {
        let scale = ceiling / measured_peak;
        for v in samples {
            *v *= scale;
        }
    }
}

/// Reads, trims and level-matches the group's WAVs (chronological order) into one take.
fn load_group(paths: &[PathBuf]) -> Result<Vec<f32>, JobFailure> {
    let mut parts: Vec<Vec<f32>> = Vec::with_capacity(paths.len());
    let mut levels = Vec::with_capacity(paths.len());
    let result = (|| {
        for path in paths {
            let mut bytes = read_capped(path, MAX_SAMPLE_WAV_BYTES)
                .map_err(|()| failure(codes::ENROLL_FAILED, "trim"))?;
            let decoded = decode_wav_pcm16_mono(&bytes);
            bytes.fill(0);
            let (mut pcm, rate) = decoded.map_err(|_| failure(codes::ENROLL_FAILED, "trim"))?;
            if rate != CAPTURE_SAMPLE_RATE {
                pcm.fill(0.0);
                return Err(failure(codes::ENROLL_FAILED, "trim"));
            }
            let level = active_rms_dbfs(&pcm, CAPTURE_SAMPLE_RATE);
            let trimmed = trim_speech(&pcm, CAPTURE_SAMPLE_RATE);
            pcm.fill(0.0);
            // A sample with no active frame adds nothing (it is silence, not discarded speech).
            if !trimmed.samples.is_empty() {
                parts.push(trimmed.samples);
                levels.push(level);
            }
        }
        Ok(level_and_join(&mut parts, &levels))
    })();
    for part in &mut parts {
        part.fill(0.0);
    }
    result
}

/// Worker of `BuildVoiceProfile`: load the model, trim/level/join, enforce the 90 s cap, build.
fn run_build(
    paths: &[PathBuf],
    model_factory: &SharedModelFactory,
    metadata: &ProfileMetadata,
) -> Result<VoiceProfile, JobFailure> {
    let model = model_factory().map_err(|e| failure(e.code(), "enroll"))?;
    let mut joined = load_group(paths)?;
    if joined.len() > MAX_JOINED_SAMPLES {
        // Never drop audio silently: the user decides what to delete (spec 4.2 step 2).
        joined.fill(0.0);
        return Err(failure(codes::ENROLL_BUDGET_EXCEEDED, "trim"));
    }
    let mut engine = SpeakerEnrollmentEngine::new(BoxedModel(model));
    let built = build_profile(&mut engine, &joined, metadata);
    joined.fill(0.0);
    built.map_err(|e| map_build_error(&e))
}

/// `take_json` of `AddIntakeSuggestion`: the call take's audio and capture device.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TakeRequest {
    pcm_f32_le_b64: String,
    sample_rate: u32,
    #[serde(default)]
    device_label: String,
    #[serde(default)]
    device_id_hash: String,
    #[serde(default)]
    name: Option<String>,
}

impl Drop for TakeRequest {
    fn drop(&mut self) {
        let mut bytes = std::mem::take(&mut self.pcm_f32_le_b64).into_bytes();
        bytes.fill(0);
        std::hint::black_box(&bytes);
    }
}

fn state_name(state: JobState) -> &'static str {
    match state {
        JobState::Running => "running",
        JobState::Done => "done",
        JobState::Failed => "failed",
    }
}

/// Known limitations of the job report (documented, not implemented):
/// - `stage` of a Running job is the stage it was spawned with (`denoise` or `trim`); progress
///   through eq/enroll is not tracked, only a failure sets the stage where it happened.
/// - Quality failures (`ENROLL_CLIPPING`, `ENROLL_TOO_QUIET`, ...) carry the fixed code only, not
///   the measured peak or level; `quality` is filled for successful ingestions.
fn job_json(prefix: &str, info: &JobInfo, extra: Option<&JobExtra>) -> Value {
    let extra = extra.cloned().unwrap_or_default();
    json!({
        "job_id": format!("{prefix}{}", info.job_id),
        "state": state_name(info.state),
        "stage": info.stage,
        "error_code": info.error_code,
        "remaining_seconds": info.remaining_seconds,
        "sample_id": extra.sample_id,
        "profile_id": extra.profile_id,
        "take_id": extra.take_id,
        "recorded": extra.recorded,
        "reason": extra.reason,
        "quality": extra.quality.map(|q| json!({
            "peak": q.peak,
            "rms_dbfs": q.rms_dbfs,
            "active_fraction": q.active_fraction,
            "speech_seconds": q.speech_seconds,
        })),
    })
}

fn debug_voice_file_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/clearcore-voice-debug.log")
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = writeln!(f, "[{now}] [DAEMON] {msg}");
    }
}

impl ServiceDaemon {
    /// Replaces the enrollment seams (test-only): fake denoiser/model, explicit development
    /// asset and legacy directory. Call before `attach_profile_store` for the migration hook.
    #[doc(hidden)]
    #[must_use]
    pub fn with_enrollment_hooks(mut self, hooks: EnrollmentHooks) -> Self {
        let config = hooks
            .dev_enrollment_asset
            .map(|(path, sha)| {
                EnrollmentConfig::from_lookup(|key| match key {
                    crate::enrollment_config::ENV_ASSET => {
                        Some(path.to_string_lossy().into_owned())
                    }
                    crate::enrollment_config::ENV_SHA256 => Some(sha.clone()),
                    _ => None,
                })
            })
            .unwrap_or_default();
        self.enrollment = EnrollmentState::with(
            config,
            hooks.denoiser_factory,
            hooks.model_factory,
            hooks.legacy_samples_dir,
        );
        self
    }

    /// A fresh denoiser for one sample, or `None` when no usable denoiser exists (no test hook
    /// and an unknown repository root, so the approved base `DFNet3` asset cannot be located).
    /// The real denoiser is always compiled in: the service depends on the model crate with its
    /// `tract` feature (see `crates/service/Cargo.toml`).
    pub(crate) fn make_denoiser(&self) -> Option<Box<dyn Denoiser>> {
        if let Some(factory) = &self.enrollment.denoiser_factory {
            return Some(factory());
        }
        let root = self.repo_root.as_deref()?;
        Some(Box::new(crate::enrollment_ingest::TractDenoiser::new(
            root.to_path_buf(),
        )))
    }

    /// Applies every finished job on the daemon thread (called before each request).
    pub(crate) fn drain_enrollment_jobs(&mut self) {
        self.enrollment.sample_jobs.expire_older_than(JOB_WATCHDOG);
        self.enrollment.profile_jobs.expire_older_than(JOB_WATCHDOG);
        for (job_id, outcome) in self.enrollment.sample_jobs.drain() {
            self.store_ingested(&job_id, &outcome);
        }
        for (job_id, (generation, profile)) in self.enrollment.profile_jobs.drain() {
            if generation == self.enrollment.profile_generation {
                self.apply_built_profile(&job_id, &profile);
            } else {
                // Stale: the gallery or the profile changed while it was being built.
                self.enrollment
                    .profile_jobs
                    .mark_failed(&job_id, failure(codes::ENROLL_FAILED, "apply"));
            }
        }
        let state = &mut self.enrollment;
        state.extras.retain(|external, _| {
            if let Some(inner) = external.strip_prefix(SAMPLE_JOB_PREFIX) {
                state.sample_jobs.info(inner).is_some()
            } else if let Some(inner) = external.strip_prefix(PROFILE_JOB_PREFIX) {
                state.profile_jobs.info(inner).is_some()
            } else {
                false
            }
        });
    }

    fn spawn_ingest(
        &mut self,
        request_id: &str,
        mut denoiser: Box<dyn Denoiser>,
        pcm: Pcm,
        kind: IngestKind,
    ) -> IpcResponse {
        let spawned = self.enrollment.sample_jobs.spawn("denoise", move || {
            let mut pcm = pcm;
            match ingest_sample(denoiser.as_mut(), &mut pcm.0) {
                Ok(result) => {
                    debug_voice_file_log(&format!(
                        "ingest_sample SUCCESS: speech_secs={}, peak={}, rms_dbfs={}",
                        result.speech_seconds, result.peak, result.rms_dbfs
                    ));
                    Ok(IngestOutcome { kind, result })
                }
                Err(e) => {
                    debug_voice_file_log(&format!(
                        "ingest_sample FAILED: error={e:?} (code={})",
                        e.code()
                    ));
                    Err(failure(e.code(), "denoise"))
                }
            }
        });
        match spawned {
            Ok(job_id) => {
                debug_voice_file_log(&format!(
                    "spawn_ingest: spawned job_id={SAMPLE_JOB_PREFIX}{job_id}"
                ));
                IpcResponse::success(
                    request_id,
                    json!({"success": true, "job_id": format!("{SAMPLE_JOB_PREFIX}{job_id}")}),
                )
            }
            Err(f) => {
                debug_voice_file_log(&format!(
                    "spawn_ingest: FAILED spawning job (busy or error)"
                ));
                busy_or_failed(request_id, f)
            }
        }
    }

    /// `AddVoiceSample`: validates and decodes the PCM, then denoises it on a job thread.
    pub(crate) fn add_voice_sample(
        &mut self,
        name: &str,
        pcm_f32_le_b64: &str,
        sample_rate: u32,
        device_label: &str,
        device_id_hash: &str,
    ) -> IpcResponse {
        const REQUEST_ID: &str = "add-voice-sample-resp";
        debug_voice_file_log(&format!(
            "add_voice_sample received: name={name}, rate={sample_rate}, label={device_label}, hash={device_id_hash}, b64_len={}",
            pcm_f32_le_b64.len()
        ));
        if !valid_metadata(name, false)
            || !valid_metadata(device_label, true)
            || !valid_metadata(device_id_hash, true)
        {
            debug_voice_file_log("add_voice_sample: FAILED invalid_metadata");
            return IpcResponse::invalid_command(REQUEST_ID, "invalid sample metadata");
        }
        // An empty hash would form a group "" mixing every unidentified microphone (D7).
        if device_id_hash.trim().is_empty() {
            debug_voice_file_log("add_voice_sample: FAILED empty device_id_hash");
            return enroll_error(REQUEST_ID, &EnrollError::InvalidAudio);
        }
        // Fail closed BEFORE decoding anything: without the base denoiser nothing is stored.
        let Some(denoiser) = self.make_denoiser() else {
            debug_voice_file_log(
                "add_voice_sample: FAILED make_denoiser is None (ModelNotConfigured)",
            );
            return enroll_error(REQUEST_ID, &EnrollError::ModelNotConfigured);
        };
        let pcm = match decode_pcm(pcm_f32_le_b64, sample_rate) {
            Ok(pcm) => pcm,
            Err(e) => {
                debug_voice_file_log(&format!("add_voice_sample: FAILED decode_pcm error: {e:?}"));
                return enroll_error(REQUEST_ID, &e);
            }
        };
        debug_voice_file_log("add_voice_sample: PCM decoded, spawning ingest job...");
        self.spawn_ingest(
            REQUEST_ID,
            denoiser,
            pcm,
            IngestKind::Manual {
                name: name.to_owned(),
                device_label: device_label.to_owned(),
                device_id_hash: device_id_hash.to_owned(),
            },
        )
    }

    /// `AddIntakeSuggestion`: same ingestion as a manual sample; the budget decides on drain.
    pub(crate) fn add_intake_suggestion(&mut self, take_json: &str) -> IpcResponse {
        const REQUEST_ID: &str = "add-intake-suggestion-resp";
        let Ok(request) = serde_json::from_str::<TakeRequest>(take_json) else {
            return IpcResponse::invalid_command(REQUEST_ID, "Invalid intake suggestion payload");
        };
        if !valid_metadata(&request.device_label, true)
            || !valid_metadata(&request.device_id_hash, true)
            || request
                .name
                .as_deref()
                .is_some_and(|n| !valid_metadata(n, false))
        {
            return IpcResponse::invalid_command(REQUEST_ID, "Invalid intake suggestion payload");
        }
        if request.device_id_hash.trim().is_empty() {
            return enroll_error(REQUEST_ID, &EnrollError::InvalidAudio);
        }
        let Some(denoiser) = self.make_denoiser() else {
            return enroll_error(REQUEST_ID, &EnrollError::ModelNotConfigured);
        };
        let pcm = match decode_pcm(&request.pcm_f32_le_b64, request.sample_rate) {
            Ok(pcm) => pcm,
            Err(e) => return enroll_error(REQUEST_ID, &e),
        };
        let kind = IngestKind::Take {
            name: request.name.clone(),
            device_label: request.device_label.clone(),
            device_id_hash: request.device_id_hash.clone(),
        };
        drop(request);
        self.spawn_ingest(REQUEST_ID, denoiser, pcm, kind)
    }

    fn store_ingested(&mut self, job_id: &str, outcome: &IngestOutcome) {
        let external = format!("{SAMPLE_JOB_PREFIX}{job_id}");
        let r = &outcome.result;
        debug_voice_file_log(&format!(
            "store_ingested called: external={external}, speech_secs={}, peak={}, rms={}",
            r.speech_seconds, r.peak, r.rms_dbfs
        ));
        let quality = Quality {
            peak: r.peak,
            rms_dbfs: r.rms_dbfs,
            active_fraction: r.active_fraction,
            speech_seconds: r.speech_seconds,
        };
        match &outcome.kind {
            IngestKind::Manual {
                name,
                device_label,
                device_id_hash,
            } => {
                let budget = budget_for(self.voice_samples.list_samples(), device_id_hash);
                if !fits_manual(&budget, r.speech_seconds) {
                    debug_voice_file_log(&format!(
                        "store_ingested: budget exceeded (used={}, max={}, needed={})",
                        budget.used_seconds, budget.max_seconds, r.speech_seconds
                    ));
                    self.enrollment.sample_jobs.mark_failed(
                        job_id,
                        JobFailure {
                            error_code: codes::ENROLL_BUDGET_EXCEEDED,
                            stage: "apply",
                            remaining_seconds: Some(budget.remaining_seconds),
                        },
                    );
                    return;
                }
                let sample_id = self.enrollment.next_id("s");
                let Ok(path) = self
                    .voice_samples
                    .write_sample_wav(&sample_id, &r.wav_bytes)
                else {
                    debug_voice_file_log("store_ingested: write_sample_wav FAILED");
                    self.fail_sample_job(job_id);
                    return;
                };
                let sample = VoiceSample {
                    id: sample_id.clone(),
                    // Epoch milliseconds: `selected_device_hash` orders samples numerically.
                    timestamp: epoch_millis().to_string(),
                    name: name.clone(),
                    audio_path: Some(path.to_string_lossy().into_owned()),
                    embedding: Vec::new(),
                    is_active: true,
                    device_label: device_label.clone(),
                    device_id_hash: device_id_hash.clone(),
                    capture_sample_rate: CAPTURE_SAMPLE_RATE,
                    speech_seconds: r.speech_seconds,
                };
                if self.voice_samples.add_sample(sample).is_err() {
                    debug_voice_file_log("store_ingested: add_sample to db FAILED");
                    let _ = std::fs::remove_file(&path);
                    self.fail_sample_job(job_id);
                    return;
                }
                debug_voice_file_log(&format!(
                    "store_ingested: sample {sample_id} successfully saved to disk!"
                ));
                self.enrollment.bump_generation();
                self.enrollment.extras.insert(
                    external,
                    JobExtra {
                        sample_id: Some(sample_id),
                        quality: Some(quality),
                        ..JobExtra::default()
                    },
                );
            }
            IngestKind::Take {
                name,
                device_label,
                device_id_hash,
            } => {
                let budget = budget_for(self.voice_samples.list_samples(), device_id_hash);
                if !fits_take(&budget, r.speech_seconds) {
                    // Not recorded and not an error (spec 4.4): nothing is written.
                    self.enrollment.extras.insert(
                        external,
                        JobExtra {
                            quality: Some(quality),
                            recorded: Some(false),
                            reason: Some("budget"),
                            ..JobExtra::default()
                        },
                    );
                    return;
                }
                let take_id = self.enrollment.next_id("t");
                let Ok(path) = self.voice_samples.write_sample_wav(&take_id, &r.wav_bytes) else {
                    self.fail_sample_job(job_id);
                    return;
                };
                let take = IntakeTake {
                    id: take_id.clone(),
                    timestamp: epoch_millis().to_string(),
                    duration_secs: r.speech_seconds,
                    snr: 0.0,
                    audio_path: Some(path.to_string_lossy().into_owned()),
                    embedding: Vec::new(),
                    device_label: device_label.clone(),
                    device_id_hash: device_id_hash.clone(),
                    speech_seconds: r.speech_seconds,
                    name: name.clone(),
                };
                if self.voice_intake.add_take(take).is_err() {
                    let _ = std::fs::remove_file(&path);
                    self.fail_sample_job(job_id);
                    return;
                }
                self.enrollment.extras.insert(
                    external,
                    JobExtra {
                        take_id: Some(take_id),
                        quality: Some(quality),
                        recorded: Some(true),
                        ..JobExtra::default()
                    },
                );
            }
        }
    }

    fn fail_sample_job(&mut self, job_id: &str) {
        self.enrollment
            .sample_jobs
            .mark_failed(job_id, failure(codes::ENROLL_FAILED, "apply"));
    }

    fn apply_built_profile(&mut self, job_id: &str, profile: &VoiceProfile) {
        match self.apply_profile(profile) {
            Ok(()) => {
                self.enrollment.extras.insert(
                    format!("{PROFILE_JOB_PREFIX}{job_id}"),
                    JobExtra {
                        profile_id: Some(profile.id.clone()),
                        ..JobExtra::default()
                    },
                );
            }
            // The transaction already kept (or restored) the previous profile. The service log
            // gets a fixed reason per branch: never the profile, its name or the backend text.
            Err(ApplyError::NotApplicable) => {
                eprintln!("{BUILT_PROFILE_NOT_APPLICABLE_LOG}");
                self.enrollment
                    .profile_jobs
                    .mark_failed(job_id, failure(codes::ENROLL_BACKEND_UNSUPPORTED, "apply"));
            }
            Err(ApplyError::NoStore) => {
                eprintln!("{BUILT_PROFILE_NO_STORE_LOG}");
                self.enrollment
                    .profile_jobs
                    .mark_failed(job_id, failure(codes::ENROLL_FAILED, "apply"));
            }
            Err(ApplyError::PersistFailed) => {
                eprintln!("{BUILT_PROFILE_PERSIST_FAILED_LOG}");
                self.enrollment
                    .profile_jobs
                    .mark_failed(job_id, failure(codes::ENROLL_FAILED, "apply"));
            }
        }
    }

    /// `BuildVoiceProfile`: selects the device group and builds the profile on a job thread.
    pub(crate) fn build_voice_profile(&mut self, name: &str) -> IpcResponse {
        const REQUEST_ID: &str = "build-voice-profile-resp";
        if !valid_metadata(name, false) {
            return IpcResponse::invalid_command(REQUEST_ID, "invalid profile name");
        }
        // Refused up front: a backend that cannot apply a profile would only fail the build at
        // the `apply` stage after the whole trim/EQ/embedding run.
        if !self.supervisor.supports_voice_profile() {
            return enroll_error(REQUEST_ID, &EnrollError::BackendUnsupported);
        }
        if self.enrollment.build_running() {
            return enroll_error(REQUEST_ID, &EnrollError::Busy);
        }
        let samples = self.voice_samples.list_samples();
        let Some(device) = selected_device_hash(samples) else {
            return enroll_error(REQUEST_ID, &EnrollError::TooLittleSpeech);
        };
        let selected_label = samples
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_active && s.audio_path.is_some() && s.device_id_hash == device)
            .max_by_key(|(i, s)| (s.timestamp.parse::<u64>().unwrap_or(0), *i))
            .map(|(_, s)| s.device_label.trim().to_ascii_lowercase());
        let mut group: Vec<(u64, usize, &VoiceSample)> = samples
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.is_active
                    && s.audio_path.is_some()
                    && (s.device_id_hash == device
                        || match selected_label.as_ref() {
                            Some(lbl) if !lbl.is_empty() => {
                                s.device_label.trim().to_ascii_lowercase() == *lbl
                            }
                            _ => false,
                        })
            })
            .map(|(i, s)| (s.timestamp.parse::<u64>().unwrap_or(0), i, s))
            .collect();
        group.sort_by_key(|(ts, i, _)| (*ts, *i));
        let speech: f32 = group
            .iter()
            .map(|(_, _, s)| s.speech_seconds)
            .filter(|v| v.is_finite() && *v > 0.0)
            .sum();
        if speech < MIN_GROUP_SPEECH_SECONDS {
            return enroll_error(REQUEST_ID, &EnrollError::TooLittleSpeech);
        }
        // Read back only the confined `samples/<id>.wav`, never a stored or client path.
        let samples_dir = self.voice_samples.samples_dir();
        let paths: Vec<PathBuf> = group
            .iter()
            .filter(|(_, _, s)| is_valid_sample_id(&s.id))
            .map(|(_, _, s)| samples_dir.join(format!("{}.wav", s.id)))
            .collect();
        let metadata = ProfileMetadata {
            id: self.enrollment.next_id("p"),
            name: name.to_owned(),
            created_at_utc: utc_now_rfc3339(),
        };
        let factory = Arc::clone(&self.enrollment.model_factory);
        self.enrollment.bump_generation();
        let generation = self.enrollment.profile_generation;
        let spawned = self.enrollment.profile_jobs.spawn("trim", move || {
            run_build(&paths, &factory, &metadata).map(|profile| (generation, profile))
        });
        match spawned {
            Ok(job_id) => {
                self.enrollment.running_build = Some(job_id.clone());
                IpcResponse::success(
                    REQUEST_ID,
                    json!({"success": true, "job_id": format!("{PROFILE_JOB_PREFIX}{job_id}")}),
                )
            }
            Err(f) => busy_or_failed(REQUEST_ID, f),
        }
    }

    /// `GetEnrollmentJob`: looks the id up in both job tables.
    pub(crate) fn get_enrollment_job(&self, job_id: &str) -> IpcResponse {
        const REQUEST_ID: &str = "get-enrollment-job-resp";
        let found = job_id
            .strip_prefix(SAMPLE_JOB_PREFIX)
            .and_then(|inner| self.enrollment.sample_jobs.info(inner))
            .map(|info| (SAMPLE_JOB_PREFIX, info))
            .or_else(|| {
                job_id
                    .strip_prefix(PROFILE_JOB_PREFIX)
                    .and_then(|inner| self.enrollment.profile_jobs.info(inner))
                    .map(|info| (PROFILE_JOB_PREFIX, info))
            });
        found.map_or_else(
            || enroll_error(REQUEST_ID, &EnrollError::JobNotFound),
            |(prefix, info)| {
                IpcResponse::success(
                    REQUEST_ID,
                    job_json(prefix, info, self.enrollment.extras.get(job_id)),
                )
            },
        )
    }

    /// `ListVoiceSamples`: gallery with the device group flags and the group's speech budget.
    /// The stored audio path is never exposed.
    pub(crate) fn list_voice_samples(&self) -> IpcResponse {
        let samples = self.voice_samples.list_samples();
        let selected = selected_device_hash(samples);
        // Label of the most recent eligible sample of the selected group (same ordering as
        // `selected_device_hash`), so the UI can warn before another microphone switches it.
        let selected_label = selected.as_deref().and_then(|device| {
            samples
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.is_active && s.audio_path.is_some() && s.device_id_hash == device
                })
                .max_by_key(|(i, s)| (s.timestamp.parse::<u64>().unwrap_or(0), *i))
                .map(|(_, s)| s.device_label.clone())
        });
        let target_label_norm = selected_label
            .as_ref()
            .map(|l| l.trim().to_ascii_lowercase());
        let items: Vec<Value> = samples
            .iter()
            .map(|s| {
                let in_group = match (selected.as_deref(), target_label_norm.as_ref()) {
                    (Some(sel_hash), Some(lbl)) if !lbl.is_empty() => {
                        s.device_id_hash == sel_hash
                            || s.device_label.trim().to_ascii_lowercase() == *lbl
                    }
                    (Some(sel_hash), _) => s.device_id_hash == sel_hash,
                    _ => false,
                };
                json!({
                    "id": s.id,
                    "name": s.name,
                    "timestamp": s.timestamp,
                    "speech_seconds": s.speech_seconds,
                    "device_label": s.device_label,
                    "device_id_hash": s.device_id_hash,
                    "is_active": s.is_active,
                    "needs_reenroll": s.audio_path.is_none(),
                    "used_in_profile": in_group && s.is_active && s.audio_path.is_some(),
                    "other_microphone": selected.is_some() && !in_group,
                })
            })
            .collect();
        let budget = budget_for(samples, selected.as_deref().unwrap_or_default());
        IpcResponse::success(
            "list-voice-samples-resp",
            json!({
                "samples": items,
                "total_count": samples.len(),
                "has_profile": self.stored_voice_profile_id.is_some(),
                "selected_device_id_hash": selected,
                "selected_device_label": selected_label,
                "budget": {
                    "used_seconds": budget.used_seconds,
                    "max_seconds": budget.max_seconds,
                    "remaining_seconds": budget.remaining_seconds,
                },
            }),
        )
    }

    /// `ApproveIntakeSuggestion`: rechecks the budget BEFORE touching the take; a take that no
    /// longer fits stays pending and the call fails with `ENROLL_BUDGET_EXCEEDED`.
    pub(crate) fn approve_intake_suggestion(
        &mut self,
        id: &str,
        name: Option<&str>,
    ) -> IpcResponse {
        const REQUEST_ID: &str = "approve-intake-suggestion-resp";
        if name.is_some_and(|n| !valid_metadata(n, false)) {
            return IpcResponse::invalid_command(REQUEST_ID, "invalid sample name");
        }
        let Some(take) = self.voice_intake.get_pending(id) else {
            return IpcResponse::error(
                REQUEST_ID,
                IpcStatus::InvalidCommand,
                "TAKE_NOT_FOUND",
                "Intake suggestion not found",
            );
        };
        let budget = budget_for(self.voice_samples.list_samples(), &take.device_id_hash);
        if !fits_manual(&budget, take.speech_seconds) {
            return enroll_error(REQUEST_ID, &EnrollError::BudgetExceeded);
        }
        match self
            .voice_intake
            .approve_take(id, &mut self.voice_samples, name)
        {
            Ok(sample) => {
                self.enrollment.bump_generation();
                IpcResponse::success(
                    REQUEST_ID,
                    json!({
                        "success": true,
                        "approved": true,
                        "sample_id": sample.id,
                        "has_profile": self.stored_voice_profile_id.is_some(),
                    }),
                )
            }
            Err(VoiceIntakeError::TakeNotFound(_)) => IpcResponse::error(
                REQUEST_ID,
                IpcStatus::InvalidCommand,
                "TAKE_NOT_FOUND",
                "Intake suggestion not found",
            ),
            Err(_) => {
                IpcResponse::internal_error(REQUEST_ID, "Failed to approve intake suggestion")
            }
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::suboptimal_flops
)]
mod tests {
    use super::*;

    fn speech(seconds: f32, scale: f32) -> Vec<f32> {
        (0..(seconds * 48_000.0) as usize)
            .map(|n| {
                let t = n as f32 / 48_000.0;
                let envelope = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
                let tau = 2.0 * std::f32::consts::PI * 150.0 * t;
                scale
                    * envelope
                    * (0.5 * tau.sin() + 0.3 * (2.0 * tau).sin() + 0.2 * (3.0 * tau).sin())
            })
            .collect()
    }

    #[test]
    fn leveling_never_clips_samples_of_very_different_levels() {
        // A loud take, a quiet take and a peaky take (sparse spikes over quiet speech) whose
        // RMS asks for a large boost that the headroom limit must cap.
        let loud = speech(3.0, 0.9);
        let quiet = speech(3.0, 0.03);
        let mut peaky = speech(3.0, 0.05);
        for v in peaky.iter_mut().step_by(4_800) {
            *v = 0.95;
        }
        let mut parts = vec![loud.clone(), loud, quiet, peaky];
        let levels: Vec<f32> = parts
            .iter()
            .map(|p| active_rms_dbfs(p, CAPTURE_SAMPLE_RATE))
            .collect();
        let joined = level_and_join(&mut parts, &levels);
        let peak = peak_of(&joined);
        assert!(peak <= LEVELING_PEAK_CEILING, "joined peak {peak}");
        assert!(joined.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn leveling_gain_respects_the_limit_and_the_headroom() {
        assert!((leveling_gain_db(30.0, 0.01) - 12.0).abs() < 1e-6);
        assert!((leveling_gain_db(-30.0, 0.5) + 12.0).abs() < 1e-6);
        let g = leveling_gain_db(12.0, 0.5);
        assert!(0.5 * 10f32.powf(g / 20.0) <= LEVELING_PEAK_CEILING);
        assert!(leveling_gain_db(f32::NAN, 0.5).abs() < 1e-6);
        // A peak above the ceiling attenuates even when a boost was asked for.
        assert!(leveling_gain_db(6.0, 1.0) < 0.0);
    }

    #[test]
    fn level_matching_never_exceeds_twelve_decibels() {
        // 40 dB apart: the median sits 20 dB from each, but each moves only 12 dB.
        let loud = speech(1.0, 0.3);
        let quiet = speech(1.0, 0.003);
        let mut parts = vec![loud.clone(), quiet.clone()];
        let levels: Vec<f32> = parts
            .iter()
            .map(|p| active_rms_dbfs(p, CAPTURE_SAMPLE_RATE))
            .collect();
        let joined = level_and_join(&mut parts, &levels);
        let fade = 960;
        let head_gain = peak_of(&joined[..loud.len() - fade]) / peak_of(&loud);
        let tail_gain = peak_of(&joined[loud.len()..]) / peak_of(&quiet);
        let db = |g: f32| 20.0 * g.log10();
        assert!(
            (db(head_gain) + 12.0).abs() < 0.05,
            "loud moved {} dB",
            db(head_gain)
        );
        assert!(
            (db(tail_gain) - 12.0).abs() < 0.05,
            "quiet moved {} dB",
            db(tail_gain)
        );
    }

    #[test]
    fn a_joined_peak_over_the_ceiling_is_scaled_down_not_up() {
        let mut parts = vec![vec![0.95_f32; 4_800]];
        let joined = level_and_join(&mut parts, &[-1.0]);
        let peak = peak_of(&joined);
        assert!(peak <= LEVELING_PEAK_CEILING && peak > 0.89, "{peak}");
        // A full-scale square wave rings after decimation: the 16 kHz peak is capped too.
        let square: Vec<f32> = (0..48_000)
            .map(|i| if (i / 24) % 2 == 0 { 0.95 } else { -0.95 })
            .collect();
        let mut parts = vec![square];
        let joined = level_and_join(&mut parts, &[0.0]);
        let peak16 = peak_of(&decimate_48k_to_16k(&joined));
        assert!(peak16 <= DECIMATED_PEAK_CEILING + 1e-4, "{peak16}");
        let mut quiet = vec![vec![0.1_f32; 4_800]];
        let same = level_and_join(&mut quiet, &[-20.0]);
        assert!(
            (peak_of(&same) - 0.1).abs() < 1e-6,
            "never amplified globally"
        );
    }

    #[test]
    fn median_handles_even_and_odd_counts() {
        assert!((median(&mut [3.0, 1.0, 2.0]) - 2.0).abs() < 1e-6);
        assert!((median(&mut [4.0, 1.0, 2.0, 3.0]) - 2.5).abs() < 1e-6);
        assert!(median(&mut []).abs() < 1e-6);
    }

    #[test]
    fn build_errors_map_to_fixed_codes() {
        let cases = [
            (BuildError::TooMuchSpeech, codes::ENROLL_BUDGET_EXCEEDED),
            (
                BuildError::Enroll(EnrollmentError::TooLong { samples: 1 }),
                codes::ENROLL_BUDGET_EXCEEDED,
            ),
            (
                BuildError::Enroll(EnrollmentError::TooShort { samples: 1 }),
                codes::ENROLL_TOO_LITTLE_SPEECH,
            ),
            (
                BuildError::Enroll(EnrollmentError::InsufficientSpeech {
                    active_fraction: 0.1,
                }),
                codes::ENROLL_TOO_LITTLE_SPEECH,
            ),
            (
                BuildError::Enroll(EnrollmentError::Clipping { peak: 1.0 }),
                codes::ENROLL_CLIPPING,
            ),
            (
                BuildError::Enroll(EnrollmentError::TooQuiet { rms_dbfs: -60.0 }),
                codes::ENROLL_TOO_QUIET,
            ),
            (
                BuildError::Enroll(EnrollmentError::Model("secret".into())),
                codes::ENROLL_FAILED,
            ),
        ];
        for (error, code) in cases {
            assert_eq!(map_build_error(&error).error_code, code);
        }
    }

    #[test]
    fn pcm_decoding_checks_rate_alignment_and_size() {
        let b64 = |bytes: &[u8]| base64::engine::general_purpose::STANDARD.encode(bytes);
        let one = 0.25_f32.to_le_bytes();
        let ok = decode_pcm(&b64(&one), 48_000).unwrap();
        assert_eq!(ok.0, vec![0.25]);
        assert_eq!(
            decode_pcm(&b64(&one), 44_100).err(),
            Some(EnrollError::InvalidAudio)
        );
        assert_eq!(
            decode_pcm(&b64(&one[..3]), 48_000).err(),
            Some(EnrollError::InvalidAudio)
        );
        assert_eq!(
            decode_pcm("not-base64!", 48_000).err(),
            Some(EnrollError::InvalidAudio)
        );
        assert_eq!(
            decode_pcm("", 48_000).err(),
            Some(EnrollError::InvalidAudio)
        );
        let too_long = "A".repeat(MAX_PCM_B64_LEN + 4);
        assert_eq!(
            decode_pcm(&too_long, 48_000).err(),
            Some(EnrollError::PayloadTooLarge)
        );
    }

    #[test]
    fn rfc3339_formats_known_instants() {
        assert_eq!(rfc3339_from_epoch(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_from_epoch(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(rfc3339_from_epoch(1_791_201_845), "2026-10-05T12:04:05Z");
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    #[test]
    fn default_build_has_a_real_denoiser() {
        let mut daemon =
            ServiceDaemon::with_supervisor(realtime_noise_supervisor::EngineSupervisor::default())
                .with_enrollment_hooks(EnrollmentHooks::default());
        assert!(daemon.make_denoiser().is_none(), "no repo root, no hook");
        daemon.set_repo_root(repo_root());
        assert!(daemon.make_denoiser().is_some());
    }

    #[test]
    #[ignore = "runs the approved DFNet3 asset; run locally"]
    fn default_build_denoiser_is_the_base_dfnet3() {
        let mut daemon =
            ServiceDaemon::with_supervisor(realtime_noise_supervisor::EngineSupervisor::default())
                .with_enrollment_hooks(EnrollmentHooks::default());
        daemon.set_repo_root(repo_root());
        let input = speech(0.5, 0.3);
        let out = daemon.make_denoiser().unwrap().denoise(&input).unwrap();
        assert_eq!(out.len(), input.len());
        assert!(out.iter().zip(&input).any(|(o, i)| (o - i).abs() > 1e-3));
    }
}
