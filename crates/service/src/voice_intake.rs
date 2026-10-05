//! Voice Intake Engine.
//!
//! Realtime monitoring and intake curation module for detected voice takes during calls.
//! Maintains a queue of pending suggestions (with duration, SNR, audio file path, and 192d embedding),
//! allowing the user to review, listen, approve (moving the take to the permanent sample gallery
//! and recalculating the profile in < 1 ms), or discard (deleting the take and its audio file).

use crate::voice_samples::{
    VOICE_EMBEDDING_DIM, VoiceSample, VoiceSampleError, VoiceSampleManager,
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
    pub embedding: Vec<f32>,
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
        if self.embedding.len() != VOICE_EMBEDDING_DIM {
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
    pub fn add_take(&mut self, take: IntakeTake) -> Result<(), VoiceIntakeError> {
        take.validate()?;
        if let Some(pos) = self.pending_takes.iter().position(|t| t.id == take.id) {
            self.pending_takes[pos] = take;
        } else {
            self.pending_takes.push(take);
        }
        self.persist()?;
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

    /// Approves a pending take, moving it to the sample gallery and recalculating the voice profile.
    pub fn approve_take(
        &mut self,
        id: &str,
        sample_manager: &mut VoiceSampleManager,
        name_tag: Option<&str>,
    ) -> Result<VoiceSample, VoiceIntakeError> {
        let pos = self
            .pending_takes
            .iter()
            .position(|t| t.id == id)
            .ok_or_else(|| VoiceIntakeError::TakeNotFound(id.to_string()))?;

        let take = self.pending_takes.remove(pos);
        self.persist()?;

        let sample_name = name_tag.map_or_else(
            || format!("Sugestão Chamada {}", take.timestamp),
            str::to_string,
        );

        let sample = VoiceSample::new(
            take.id,
            take.timestamp,
            sample_name,
            take.audio_path,
            take.embedding,
        )?;

        sample_manager.add_sample(sample.clone())?;
        Ok(sample)
    }

    /// Discards a pending take, removing it from the queue and deleting any audio file on disk.
    pub fn discard_take(&mut self, id: &str) -> Result<bool, VoiceIntakeError> {
        if let Some(pos) = self.pending_takes.iter().position(|t| t.id == id) {
            let take = self.pending_takes.remove(pos);
            if let Some(ref path_str) = take.audio_path {
                let audio_path = PathBuf::from(path_str);
                if audio_path.is_file() {
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
        let audio_file = temp.path().join("discard_me.wav");
        fs::write(&audio_file, b"audio data").expect("write audio");
        assert!(audio_file.is_file());

        let mut engine = VoiceIntakeEngine::new(Some(temp.path().join("intake")));
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
}
