//! Voice Intake Engine.
//!
//! Intake curation for voice takes detected during calls. Pending suggestions carry the take's
//! denoised WAV (stored by the service in `samples/`), its speech duration and capture device; the
//! user approves one (it becomes a gallery sample) or discards it (its WAV is deleted).
//!
//! Confinement: only an `audio_path` that resolves inside the private `samples/` directory is
//! ever deleted. Legacy takes whose path points elsewhere keep their file untouched.

use crate::voice_samples::{
    SAMPLES_DIR_NAME, VOICE_EMBEDDING_DIM, VoiceSample, VoiceSampleError, VoiceSampleManager,
};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const INTAKE_FILE_NAME: &str = "intake_suggestions.json";

#[derive(Debug)]
pub enum VoiceIntakeError {
    InvalidDimension { expected: usize, actual: usize },
    NonFiniteValue(&'static str),
    EmptyId,
    InvalidDuration(f32),
    TakeNotFound(String),
    Serialization(String),
    Deserialization(String),
    Io(std::io::Error),
    SampleError(VoiceSampleError),
}

impl fmt::Display for VoiceIntakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimension { expected, actual } => {
                write!(
                    f,
                    "intake take embedding dimension mismatch: expected {expected}, got {actual}"
                )
            }
            Self::NonFiniteValue(field) => {
                write!(f, "non-finite value in intake take field '{field}'")
            }
            Self::EmptyId => write!(f, "intake take id cannot be empty"),
            Self::InvalidDuration(d) => {
                write!(
                    f,
                    "invalid intake take duration {d}s: must be positive and finite"
                )
            }
            Self::TakeNotFound(id) => write!(f, "intake suggestion '{id}' not found"),
            Self::Serialization(err) => write!(f, "failed to serialize intake suggestions: {err}"),
            Self::Deserialization(err) => {
                write!(f, "failed to deserialize intake suggestions: {err}")
            }
            Self::Io(err) => write!(f, "I/O error in intake engine: {err}"),
            Self::SampleError(err) => {
                write!(f, "failed to add approved take to sample gallery: {err}")
            }
        }
    }
}

impl std::error::Error for VoiceIntakeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::SampleError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VoiceIntakeError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<VoiceSampleError> for VoiceIntakeError {
    fn from(err: VoiceSampleError) -> Self {
        Self::SampleError(err)
    }
}

/// A detected candidate voice take from a call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntakeTake {
    pub id: String,
    pub timestamp: String,
    pub duration_secs: f32,
    pub snr: f32,
    pub audio_path: Option<String>,
    /// Legacy takes carried a 192d embedding; audio takes carry none (empty).
    #[serde(default)]
    pub embedding: Vec<f32>,
    #[serde(default)]
    pub device_label: String,
    #[serde(default)]
    pub device_id_hash: String,
    /// Active speech after trimming, measured on the denoised take (spec 4.4).
    #[serde(default)]
    pub speech_seconds: f32,
    /// Name proposed by the producer of the take, if any.
    #[serde(default)]
    pub name: Option<String>,
}

/// True when `path` resolves (canonicalized) inside `root`.
fn is_inside(path: &Path, root: &Path) -> bool {
    match (fs::canonicalize(path), fs::canonicalize(root)) {
        (Ok(file), Ok(root)) => file.starts_with(root),
        _ => false,
    }
}

impl IntakeTake {
    pub fn new(
        id: impl Into<String>,
        timestamp: impl Into<String>,
        duration_secs: f32,
        snr: f32,
        audio_path: Option<String>,
        embedding: Vec<f32>,
    ) -> Result<Self, VoiceIntakeError> {
        let take = Self {
            id: id.into(),
            timestamp: timestamp.into(),
            duration_secs,
            snr,
            audio_path,
            embedding,
            device_label: String::new(),
            device_id_hash: String::new(),
            speech_seconds: 0.0,
            name: None,
        };
        take.validate()?;
        Ok(take)
    }

    pub fn validate(&self) -> Result<(), VoiceIntakeError> {
        if self.id.trim().is_empty() {
            return Err(VoiceIntakeError::EmptyId);
        }
        if !self.duration_secs.is_finite() || self.duration_secs <= 0.0 {
            return Err(VoiceIntakeError::InvalidDuration(self.duration_secs));
        }
        if !self.snr.is_finite() {
            return Err(VoiceIntakeError::NonFiniteValue("snr"));
        }
        if !self.speech_seconds.is_finite() || self.speech_seconds < 0.0 {
            return Err(VoiceIntakeError::NonFiniteValue("speech_seconds"));
        }
        if !self.embedding.is_empty() && self.embedding.len() != VOICE_EMBEDDING_DIM {
            return Err(VoiceIntakeError::InvalidDimension {
                expected: VOICE_EMBEDDING_DIM,
                actual: self.embedding.len(),
            });
        }
        for &val in &self.embedding {
            if !val.is_finite() {
                return Err(VoiceIntakeError::NonFiniteValue("embedding"));
            }
        }
        Ok(())
    }
}

/// Realtime monitoring and curation engine for voice intake suggestions.
#[derive(Debug, Clone)]
pub struct VoiceIntakeEngine {
    dir: Option<PathBuf>,
    pending_takes: Vec<IntakeTake>,
}

impl VoiceIntakeEngine {
    #[must_use]
    pub fn new(storage_dir: Option<PathBuf>) -> Self {
        let mut engine = Self {
            dir: storage_dir,
            pending_takes: Vec::new(),
        };
        let _ = engine.load_from_disk();
        engine
    }

    #[must_use]
    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    #[must_use]
    pub fn intake_file_path(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(INTAKE_FILE_NAME))
    }

    /// Loads pending intake suggestions from disk if storage directory is configured.
    pub fn load_from_disk(&mut self) -> Result<(), VoiceIntakeError> {
        let Some(path) = self.intake_file_path() else {
            return Ok(());
        };
        if !path.exists() {
            self.pending_takes.clear();
            return Ok(());
        }

        let bytes = fs::read(&path)?;
        let takes: Vec<IntakeTake> = serde_json::from_slice(&bytes)
            .map_err(|e| VoiceIntakeError::Deserialization(e.to_string()))?;

        for take in &takes {
            take.validate()?;
        }
        self.pending_takes = takes;
        Ok(())
    }

    /// Persists pending takes to disk atomically if storage is configured.
    pub fn persist(&self) -> Result<(), VoiceIntakeError> {
        let Some(path) = self.intake_file_path() else {
            return Ok(());
        };

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json_bytes = serde_json::to_vec_pretty(&self.pending_takes)
            .map_err(|e| VoiceIntakeError::Serialization(e.to_string()))?;

        let tmp_path = path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));

        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }

        let write_result = options.open(&tmp_path).and_then(|mut file| {
            file.write_all(&json_bytes)?;
            file.sync_all()
        });
        if let Err(error) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(error.into());
        }

        if let Err(error) = fs::rename(&tmp_path, &path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(error.into());
        }

        Ok(())
    }

    /// Adds a new detected take to the pending queue.
    /// If persisting fails the in-memory queue is restored (no phantom take).
    pub fn add_take(&mut self, take: IntakeTake) -> Result<(), VoiceIntakeError> {
        take.validate()?;
        let previous = if let Some(pos) = self.pending_takes.iter().position(|t| t.id == take.id) {
            Some((pos, std::mem::replace(&mut self.pending_takes[pos], take)))
        } else {
            self.pending_takes.push(take);
            None
        };
        if let Err(error) = self.persist() {
            match previous {
                Some((pos, old)) => self.pending_takes[pos] = old,
                None => {
                    self.pending_takes.pop();
                }
            }
            return Err(error);
        }
        Ok(())
    }

    /// Returns all pending takes awaiting user curation.
    #[must_use]
    pub fn list_pending(&self) -> &[IntakeTake] {
        &self.pending_takes
    }

    /// Finds a pending take by id.
    #[must_use]
    pub fn get_pending(&self, id: &str) -> Option<&IntakeTake> {
        self.pending_takes.iter().find(|t| t.id == id)
    }

    /// Private audio directory of this engine (`<dir>/samples`), if storage is configured.
    #[must_use]
    pub fn samples_dir(&self) -> Option<PathBuf> {
        self.dir.as_ref().map(|d| d.join(SAMPLES_DIR_NAME))
    }

    /// Approves a pending take: the gallery sample is added FIRST and the take is removed only
    /// after that succeeded, so a failed add leaves the take pending. The caller rechecks the
    /// speech budget before calling this. The sample keeps the take's WAV only when its path
    /// resolves inside the gallery's `samples/`; otherwise it is stored without audio
    /// (`needs_reenroll`).
    pub fn approve_take(
        &mut self,
        id: &str,
        sample_manager: &mut VoiceSampleManager,
        name_tag: Option<&str>,
    ) -> Result<VoiceSample, VoiceIntakeError> {
        let take = self
            .get_pending(id)
            .cloned()
            .ok_or_else(|| VoiceIntakeError::TakeNotFound(id.to_string()))?;

        let confined_audio = take
            .audio_path
            .as_ref()
            .filter(|p| is_inside(Path::new(p), &sample_manager.samples_dir()))
            .cloned();
        let has_audio = confined_audio.is_some();
        let sample_name = name_tag
            .map(str::to_string)
            .or(take.name)
            .unwrap_or_else(|| format!("Sugestão Chamada {}", take.timestamp));
        let sample = VoiceSample {
            id: take.id,
            timestamp: take.timestamp,
            name: sample_name,
            audio_path: confined_audio,
            embedding: take.embedding,
            is_active: true,
            device_label: take.device_label,
            device_id_hash: take.device_id_hash,
            capture_sample_rate: if has_audio { 48_000 } else { 0 },
            speech_seconds: if has_audio { take.speech_seconds } else { 0.0 },
        };
        sample.validate()?;
        sample_manager.add_sample(sample.clone())?;

        self.pending_takes.retain(|t| t.id != id);
        self.persist()?;
        Ok(sample)
    }

    /// Discards a pending take, removing it from the queue and deleting its audio file when (and
    /// only when) that file resolves inside `samples/`.
    pub fn discard_take(&mut self, id: &str) -> Result<bool, VoiceIntakeError> {
        if let Some(pos) = self.pending_takes.iter().position(|t| t.id == id) {
            let take = self.pending_takes.remove(pos);
            if let (Some(path_str), Some(root)) = (take.audio_path.as_ref(), self.samples_dir()) {
                let audio_path = PathBuf::from(path_str);
                if audio_path.is_file() && is_inside(&audio_path, &root) {
                    let _ = fs::remove_file(audio_path);
                }
            }
            self.persist()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn make_unit_vector(idx: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; VOICE_EMBEDDING_DIM];
        v[idx % VOICE_EMBEDDING_DIM] = 1.0;
        v
    }

    #[test]
    fn test_intake_take_validation() {
        let vec = make_unit_vector(0);
        let take = IntakeTake::new(
            "take-1",
            "2026-10-05T01:00:00Z",
            4.5,
            22.0,
            None,
            vec.clone(),
        );
        assert!(take.is_ok());

        // Empty ID
        let empty_id = IntakeTake::new("", "2026-10-05T01:00:00Z", 4.5, 22.0, None, vec.clone());
        assert!(matches!(empty_id, Err(VoiceIntakeError::EmptyId)));

        // Non-positive or non-finite duration
        let neg_dur = IntakeTake::new("t1", "2026-10-05T01:00:00Z", -1.0, 22.0, None, vec.clone());
        assert!(matches!(neg_dur, Err(VoiceIntakeError::InvalidDuration(_))));

        let nan_dur = IntakeTake::new(
            "t1",
            "2026-10-05T01:00:00Z",
            f32::NAN,
            22.0,
            None,
            vec.clone(),
        );
        assert!(matches!(nan_dur, Err(VoiceIntakeError::InvalidDuration(_))));

        // Non-finite SNR
        let nan_snr = IntakeTake::new(
            "t1",
            "2026-10-05T01:00:00Z",
            4.5,
            f32::NAN,
            None,
            vec.clone(),
        );
        assert!(matches!(
            nan_snr,
            Err(VoiceIntakeError::NonFiniteValue("snr"))
        ));

        // Wrong dimension
        let mut short_vec = vec;
        short_vec.pop();
        let bad_dim = IntakeTake::new("t1", "2026-10-05T01:00:00Z", 4.5, 22.0, None, short_vec);
        assert!(matches!(
            bad_dim,
            Err(VoiceIntakeError::InvalidDimension {
                expected: 192,
                actual: 191
            })
        ));
    }

    #[test]
    fn test_intake_add_and_list() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut engine = VoiceIntakeEngine::new(Some(temp.path().join("intake")));

        assert!(engine.list_pending().is_empty());

        let t1 = IntakeTake::new(
            "t1",
            "2026-10-05T01:00:00Z",
            5.0,
            18.5,
            None,
            make_unit_vector(1),
        )
        .expect("t1");
        let t2 = IntakeTake::new(
            "t2",
            "2026-10-05T01:01:00Z",
            4.2,
            24.1,
            None,
            make_unit_vector(2),
        )
        .expect("t2");

        engine.add_take(t1).expect("add t1");
        engine.add_take(t2).expect("add t2");

        assert_eq!(engine.list_pending().len(), 2);
        assert!((engine.get_pending("t1").expect("t1").duration_secs - 5.0).abs() < 1e-6);
        assert!((engine.get_pending("t2").expect("t2").snr - 24.1).abs() < 1e-6);
    }

    #[test]
    fn test_intake_approval_moves_to_samples_and_recalculates_profile() {
        let temp = tempfile::tempdir().expect("tempdir");
        let samples_dir = temp.path().join("profiles");
        let intake_dir = temp.path().join("intake");

        let mut samples_mgr = VoiceSampleManager::new(&samples_dir);
        let mut intake_engine = VoiceIntakeEngine::new(Some(intake_dir));

        assert!(samples_mgr.list_samples().is_empty());
        assert!(samples_mgr.compute_profile_embedding().is_none());

        let take_vec = make_unit_vector(7);
        let take = IntakeTake::new(
            "call-take-42",
            "2026-10-05T02:00:00Z",
            4.8,
            19.3,
            None,
            take_vec,
        )
        .expect("take");
        intake_engine.add_take(take).expect("add take");

        assert_eq!(intake_engine.list_pending().len(), 1);

        // Approve take
        let approved_sample = intake_engine
            .approve_take("call-take-42", &mut samples_mgr, Some("Chamada de Sexta"))
            .expect("approve");

        assert_eq!(approved_sample.id, "call-take-42");
        assert_eq!(approved_sample.name, "Chamada de Sexta");

        // Pending queue must be empty
        assert!(intake_engine.list_pending().is_empty());

        // Sample gallery must have the sample
        assert_eq!(samples_mgr.list_samples().len(), 1);
        let sample = samples_mgr
            .get_sample("call-take-42")
            .expect("sample in gallery");
        assert_eq!(sample.name, "Chamada de Sexta");

        // Profile must be recalculated and active
        let profile = samples_mgr
            .compute_profile_embedding()
            .expect("active profile");
        assert!((profile[7] - 1.0).abs() < 1e-6);
        assert!(profile[0].abs() < 1e-6);
    }

    #[test]
    fn test_intake_discard_removes_take_and_deletes_audio_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let intake_dir = temp.path().join("intake");
        fs::create_dir_all(intake_dir.join(SAMPLES_DIR_NAME)).expect("samples dir");
        let audio_file = intake_dir.join(SAMPLES_DIR_NAME).join("discard_me.wav");
        fs::write(&audio_file, b"audio data").expect("write audio");
        assert!(audio_file.is_file());

        let mut engine = VoiceIntakeEngine::new(Some(intake_dir));
        let take = IntakeTake::new(
            "junk-take",
            "2026-10-05T03:00:00Z",
            4.0,
            12.0,
            Some(audio_file.to_str().expect("str").to_string()),
            make_unit_vector(9),
        )
        .expect("take");
        engine.add_take(take).expect("add");

        assert_eq!(engine.list_pending().len(), 1);

        // Discard
        let discarded = engine.discard_take("junk-take").expect("discard");
        assert!(discarded);
        assert!(engine.list_pending().is_empty());
        assert!(
            !audio_file.exists(),
            "Audio file must be deleted upon discard"
        );

        // Discarding non-existent
        assert!(!engine.discard_take("junk-take").expect("discard again"));
    }

    fn take_with_audio(id: &str, path: &Path) -> IntakeTake {
        IntakeTake::new(
            id,
            "1",
            4.0,
            0.0,
            Some(path.to_str().expect("str").to_string()),
            Vec::new(),
        )
        .expect("take")
    }

    #[test]
    fn discard_never_deletes_a_file_outside_samples() {
        let temp = tempfile::tempdir().expect("tempdir");
        let intake_dir = temp.path().join("intake");
        let sentinel = temp.path().join("sentinel.txt");
        fs::write(&sentinel, b"keep me").expect("write");
        // A path that only looks like it is inside: `samples/../../sentinel.txt`.
        fs::create_dir_all(intake_dir.join(SAMPLES_DIR_NAME)).expect("samples dir");
        let sneaky = intake_dir
            .join(SAMPLES_DIR_NAME)
            .join("..")
            .join("..")
            .join("sentinel.txt");

        let mut engine = VoiceIntakeEngine::new(Some(intake_dir));
        engine
            .add_take(take_with_audio("a", &sentinel))
            .expect("add a");
        engine
            .add_take(take_with_audio("b", &sneaky))
            .expect("add b");
        assert!(engine.discard_take("a").expect("discard a"));
        assert!(engine.discard_take("b").expect("discard b"));
        assert!(sentinel.is_file(), "a file outside samples/ must survive");
        assert!(engine.list_pending().is_empty());
    }

    #[test]
    fn audio_take_without_embedding_is_valid_and_old_json_loads() {
        let take = IntakeTake::new("t", "1", 4.0, 0.0, None, Vec::new()).expect("take");
        assert!(take.validate().is_ok());
        let old = r#"{"id":"x","timestamp":"1","duration_secs":4.0,"snr":1.0,"audio_path":null}"#;
        let parsed: IntakeTake = serde_json::from_str(old).expect("old json");
        assert!(parsed.embedding.is_empty() && parsed.device_id_hash.is_empty());
    }

    #[test]
    fn approve_adds_the_sample_before_removing_the_take() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("profiles");
        let mut samples = VoiceSampleManager::new(&dir);
        let wav = samples.write_sample_wav("t-1", b"RIFF").expect("wav");
        let mut engine = VoiceIntakeEngine::new(Some(dir.clone()));
        let mut take = take_with_audio("t-1", &wav);
        take.speech_seconds = 4.0;
        take.device_id_hash = "dev".into();
        engine.add_take(take).expect("add");

        // Make the gallery unwritable through a symlinked profile dir: the add fails.
        let blocked_dir = temp.path().join("blocked");
        std::os::unix::fs::symlink(&dir, &blocked_dir).expect("symlink");
        let mut blocked = VoiceSampleManager::new(&blocked_dir);
        assert!(engine.approve_take("t-1", &mut blocked, None).is_err());
        assert_eq!(
            engine.list_pending().len(),
            1,
            "a failed approve keeps the take"
        );

        let sample = engine
            .approve_take("t-1", &mut samples, Some("Nome"))
            .expect("approve");
        assert_eq!(sample.audio_path.as_deref(), wav.to_str());
        assert!((sample.speech_seconds - 4.0).abs() < 1e-6);
        assert_eq!(sample.device_id_hash, "dev");
        assert!(engine.list_pending().is_empty());
    }
}
