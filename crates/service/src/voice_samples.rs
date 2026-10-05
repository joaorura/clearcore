//! Voice Sample Gallery and Profile Embedding Manager.
//!
//! Stores user voice samples locally with secure file permissions (0700 dir, 0600 files),
//! and computes the normalized L2 vector average of all active samples in < 1 ms:
//!
//! ```text
//! e_perfil = sum(e_i) / ||sum(e_i)||_2
//! ```
//!
//! Produces the 768-byte binary voice profile (`profile.bin`) for `pDFNet3`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const VOICE_EMBEDDING_DIM: usize = 192;
pub const PROFILE_BIN_BYTES: usize = 768; // 192 * 4 bytes (IEEE-754 Float32 little endian)
pub const SAMPLES_FILE_NAME: &str = "voice_samples.json";
pub const PROFILE_BIN_FILE_NAME: &str = "profile.bin";
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug)]
pub enum VoiceSampleError {
    InvalidDimension { expected: usize, actual: usize },
    NonFiniteValue { index: usize },
    EmptyId,
    EmptyName,
    Serialization(String),
    Deserialization(String),
    InsecurePermissions(String),
    Io(std::io::Error),
    SampleNotFound(String),
}

impl fmt::Display for VoiceSampleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDimension { expected, actual } => {
                write!(
                    f,
                    "voice sample embedding dimension mismatch: expected {expected}, got {actual}"
                )
            }
            Self::NonFiniteValue { index } => {
                write!(f, "non-finite value in sample embedding at index {index}")
            }
            Self::EmptyId => write!(f, "voice sample id cannot be empty"),
            Self::EmptyName => write!(f, "voice sample name cannot be empty"),
            Self::Serialization(err) => write!(f, "failed to serialize voice samples: {err}"),
            Self::Deserialization(err) => write!(f, "failed to deserialize voice samples: {err}"),
            Self::InsecurePermissions(err) => write!(f, "insecure storage permissions: {err}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
            Self::SampleNotFound(id) => write!(f, "voice sample with id '{id}' not found"),
        }
    }
}

impl std::error::Error for VoiceSampleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VoiceSampleError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

const fn default_active() -> bool {
    true
}

/// Metadata and 192d embedding vector for a single voice sample.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceSample {
    pub id: String,
    pub timestamp: String,
    pub name: String,
    pub audio_path: Option<String>,
    pub embedding: Vec<f32>,
    #[serde(default = "default_active")]
    pub is_active: bool,
}

impl VoiceSample {
    pub fn new(
        id: impl Into<String>,
        timestamp: impl Into<String>,
        name: impl Into<String>,
        audio_path: Option<String>,
        embedding: Vec<f32>,
    ) -> Result<Self, VoiceSampleError> {
        let sample = Self {
            id: id.into(),
            timestamp: timestamp.into(),
            name: name.into(),
            audio_path,
            embedding,
            is_active: true,
        };
        sample.validate()?;
        Ok(sample)
    }

    pub fn validate(&self) -> Result<(), VoiceSampleError> {
        if self.id.trim().is_empty() {
            return Err(VoiceSampleError::EmptyId);
        }
        if self.name.trim().is_empty() {
            return Err(VoiceSampleError::EmptyName);
        }
        if self.embedding.len() != VOICE_EMBEDDING_DIM {
            return Err(VoiceSampleError::InvalidDimension {
                expected: VOICE_EMBEDDING_DIM,
                actual: self.embedding.len(),
            });
        }
        for (index, &val) in self.embedding.iter().enumerate() {
            if !val.is_finite() {
                return Err(VoiceSampleError::NonFiniteValue { index });
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoiceSamplesManifest {
    pub version: u32,
    pub updated_at_utc: String,
    pub samples: Vec<VoiceSample>,
}

/// Manages storage and dynamic vector aggregation for voice samples.
#[derive(Debug, Clone)]
pub struct VoiceSampleManager {
    dir: PathBuf,
    samples: Vec<VoiceSample>,
}

impl VoiceSampleManager {
    #[must_use]
    pub fn default_dir() -> PathBuf {
        std::env::var_os("HOME").map_or_else(
            || {
                std::env::var_os("APPDATA").map_or_else(
                    || PathBuf::from(".clearcore").join("profiles"),
                    |appdata| PathBuf::from(appdata).join("clearcore").join("profiles"),
                )
            },
            |home| PathBuf::from(home).join(".clearcore").join("profiles"),
        )
    }

    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            samples: Vec::new(),
        }
    }

    /// Creates and loads samples from `dir`, creating directory if needed.
    pub fn load(dir: impl Into<PathBuf>) -> Result<Self, VoiceSampleError> {
        let mut manager = Self::new(dir);
        manager.load_from_disk()?;
        Ok(manager)
    }

    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    #[must_use]
    pub fn samples_file_path(&self) -> PathBuf {
        self.dir.join(SAMPLES_FILE_NAME)
    }

    #[must_use]
    pub fn profile_bin_path(&self) -> PathBuf {
        self.dir.join(PROFILE_BIN_FILE_NAME)
    }

    fn refuse_symlinked_dir(&self) -> Result<(), VoiceSampleError> {
        match fs::symlink_metadata(&self.dir) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                Err(VoiceSampleError::InsecurePermissions(format!(
                    "profile directory {} is a symlink",
                    self.dir.display()
                )))
            }
            Ok(_) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn ensure_dir(&self) -> Result<(), VoiceSampleError> {
        self.refuse_symlinked_dir()?;
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&self.dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&self.dir)?.permissions().mode();
            if mode & 0o077 != 0 {
                fs::set_permissions(&self.dir, fs::Permissions::from_mode(0o700))?;
            }
        }
        Ok(())
    }

    /// Loads samples from disk if `voice_samples.json` exists.
    pub fn load_from_disk(&mut self) -> Result<(), VoiceSampleError> {
        self.refuse_symlinked_dir()?;
        let path = self.samples_file_path();
        if !path.exists() {
            self.samples.clear();
            return Ok(());
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let metadata = fs::symlink_metadata(&path)?;
            let mode = metadata.permissions().mode();
            if mode & 0o077 != 0 {
                return Err(VoiceSampleError::InsecurePermissions(format!(
                    "file mode {mode:#o} has group/world permissions (expected 0600)"
                )));
            }
        }

        let bytes = fs::read(&path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| VoiceSampleError::Deserialization(e.to_string()))?;

        // Support both direct array `[VoiceSample, ...]` and `VoiceSamplesManifest`.
        if let Ok(manifest) = serde_json::from_str::<VoiceSamplesManifest>(text) {
            for sample in &manifest.samples {
                sample.validate()?;
            }
            self.samples = manifest.samples;
        } else {
            let samples: Vec<VoiceSample> = serde_json::from_str(text)
                .map_err(|e| VoiceSampleError::Deserialization(e.to_string()))?;
            for sample in &samples {
                sample.validate()?;
            }
            self.samples = samples;
        }

        Ok(())
    }

    /// Atomically persists current samples and updates/removes `profile.bin`.
    pub fn persist(&self) -> Result<(), VoiceSampleError> {
        self.ensure_dir()?;
        let now_utc = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs())
            .to_string();

        let manifest = VoiceSamplesManifest {
            version: MANIFEST_VERSION,
            updated_at_utc: now_utc,
            samples: self.samples.clone(),
        };

        let json_bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|e| VoiceSampleError::Serialization(e.to_string()))?;

        Self::write_secure_atomic(&self.samples_file_path(), &json_bytes)?;

        // Update profile.bin
        if let Some(profile_bytes) = self.compute_profile_bin() {
            Self::write_secure_atomic(&self.profile_bin_path(), &profile_bytes)?;
        } else {
            let bin_path = self.profile_bin_path();
            if bin_path.exists() {
                let _ = fs::remove_file(bin_path);
            }
        }

        Ok(())
    }

    fn write_secure_atomic(path: &Path, content: &[u8]) -> Result<(), VoiceSampleError> {
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
            file.write_all(content)?;
            file.sync_all()
        });
        if let Err(error) = write_result {
            let _ = fs::remove_file(&tmp_path);
            return Err(error.into());
        }

        if let Err(error) = fs::rename(&tmp_path, path) {
            let _ = fs::remove_file(&tmp_path);
            return Err(error.into());
        }
        Ok(())
    }

    /// Adds or replaces a sample and persists the updated gallery and profile.
    pub fn add_sample(&mut self, sample: VoiceSample) -> Result<(), VoiceSampleError> {
        sample.validate()?;
        if let Some(existing) = self.samples.iter_mut().find(|s| s.id == sample.id) {
            *existing = sample;
        } else {
            self.samples.push(sample);
        }
        self.persist()
    }

    /// Lists all stored voice samples.
    #[must_use]
    pub fn list_samples(&self) -> &[VoiceSample] {
        &self.samples
    }

    /// Gets a single voice sample by id.
    #[must_use]
    pub fn get_sample(&self, id: &str) -> Option<&VoiceSample> {
        self.samples.iter().find(|s| s.id == id)
    }

    /// Deletes a voice sample by id, optionally removing the associated audio file.
    pub fn delete_sample(
        &mut self,
        id: &str,
        delete_audio_file: bool,
    ) -> Result<bool, VoiceSampleError> {
        if let Some(pos) = self.samples.iter().position(|s| s.id == id) {
            let removed = self.samples.remove(pos);
            if delete_audio_file && let Some(ref audio_path_str) = removed.audio_path {
                let audio_path = PathBuf::from(audio_path_str);
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

    /// Toggles active status of a sample and updates profile.bin.
    pub fn set_sample_active(
        &mut self,
        id: &str,
        is_active: bool,
    ) -> Result<bool, VoiceSampleError> {
        if let Some(sample) = self.samples.iter_mut().find(|s| s.id == id) {
            sample.is_active = is_active;
            self.persist()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Computes the normalized L2 vector average of all active samples:
    ///
    /// ```text
    /// e_perfil = sum(e_i) / ||sum(e_i)||_2
    /// ```
    ///
    /// Executes in < 1 ms (< 10 microseconds for typical counts).
    #[must_use]
    pub fn compute_profile_embedding(&self) -> Option<[f32; VOICE_EMBEDDING_DIM]> {
        let active_samples: Vec<&VoiceSample> =
            self.samples.iter().filter(|s| s.is_active).collect();
        if active_samples.is_empty() {
            return None;
        }

        let mut sum = [0.0f64; VOICE_EMBEDDING_DIM];
        for sample in active_samples {
            for (i, &val) in sample.embedding.iter().enumerate() {
                sum[i] += f64::from(val);
            }
        }

        let mut norm_sq = 0.0f64;
        for &val in &sum {
            norm_sq += val * val;
        }

        let norm = norm_sq.sqrt();
        if norm <= 1e-12 {
            return None;
        }

        let mut normalized = [0.0f32; VOICE_EMBEDDING_DIM];
        for (i, &val) in sum.iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            {
                normalized[i] = (val / norm) as f32;
            }
        }

        Some(normalized)
    }

    /// Computes the 768-byte binary representation of the normalized profile embedding.
    #[must_use]
    pub fn compute_profile_bin(&self) -> Option<[u8; PROFILE_BIN_BYTES]> {
        let embedding = self.compute_profile_embedding()?;
        let mut bytes = [0u8; PROFILE_BIN_BYTES];
        for (i, &val) in embedding.iter().enumerate() {
            let le = val.to_le_bytes();
            bytes[i * 4..(i + 1) * 4].copy_from_slice(&le);
        }
        Some(bytes)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn make_unit_vector(active_idx: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; VOICE_EMBEDDING_DIM];
        v[active_idx % VOICE_EMBEDDING_DIM] = 1.0;
        v
    }

    #[test]
    fn test_sample_validation() {
        let valid_vec = make_unit_vector(0);
        let sample = VoiceSample::new(
            "s1",
            "2026-10-05T00:00:00Z",
            "Sample 1",
            None,
            valid_vec.clone(),
        );
        assert!(sample.is_ok());

        // Empty ID
        let empty_id = VoiceSample::new(
            "",
            "2026-10-05T00:00:00Z",
            "Sample 1",
            None,
            valid_vec.clone(),
        );
        assert!(matches!(empty_id, Err(VoiceSampleError::EmptyId)));

        // Empty Name
        let empty_name =
            VoiceSample::new("s1", "2026-10-05T00:00:00Z", "  ", None, valid_vec.clone());
        assert!(matches!(empty_name, Err(VoiceSampleError::EmptyName)));

        // Wrong dimension (too short or too long)
        let mut short_vec = valid_vec.clone();
        short_vec.pop();
        let short_sample =
            VoiceSample::new("s1", "2026-10-05T00:00:00Z", "Sample 1", None, short_vec);
        assert!(matches!(
            short_sample,
            Err(VoiceSampleError::InvalidDimension {
                expected: 192,
                actual: 191
            })
        ));

        // Non-finite value
        let mut bad_vec = valid_vec;
        bad_vec[42] = f32::NAN;
        let nan_sample = VoiceSample::new("s1", "2026-10-05T00:00:00Z", "Sample 1", None, bad_vec);
        assert!(matches!(
            nan_sample,
            Err(VoiceSampleError::NonFiniteValue { index: 42 })
        ));
    }

    #[test]
    fn test_l2_recalculation_single_sample_speed() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut mgr = VoiceSampleManager::new(temp.path().join("profiles"));

        let vec1 = make_unit_vector(10);
        let sample =
            VoiceSample::new("s1", "2026-10-05T00:00:00Z", "Sample 1", None, vec1).expect("sample");
        mgr.add_sample(sample).expect("add");

        let start = Instant::now();
        let profile = mgr.compute_profile_embedding().expect("profile exists");
        let elapsed = start.elapsed();

        // Must run in < 1 ms (typically < 10 microseconds)
        assert!(
            elapsed < std::time::Duration::from_millis(1),
            "Recalculation took {elapsed:?}"
        );
        assert!((profile[10] - 1.0).abs() < 1e-6);
        assert!(profile[0].abs() < 1e-6);
    }

    #[test]
    fn test_l2_recalculation_orthogonal_vectors() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut mgr = VoiceSampleManager::new(temp.path().join("profiles"));

        // Two orthogonal unit vectors: index 0 and index 1
        let v1 = make_unit_vector(0);
        let v2 = make_unit_vector(1);

        mgr.add_sample(VoiceSample::new("s1", "2026-10-05T00:00:00Z", "S1", None, v1).expect("s1"))
            .expect("add");
        mgr.add_sample(VoiceSample::new("s2", "2026-10-05T00:00:00Z", "S2", None, v2).expect("s2"))
            .expect("add");

        let profile = mgr.compute_profile_embedding().expect("profile");

        // (e1 + e2) / sqrt(2) => e1/sqrt(2) approx 0.70710678
        let expected = 1.0f32 / 2.0f32.sqrt();
        assert!((profile[0] - expected).abs() < 1e-5);
        assert!((profile[1] - expected).abs() < 1e-5);
        assert!(profile[2].abs() < 1e-6);

        // Verify total L2 norm is strictly 1.0
        let norm_sq: f32 = profile.iter().map(|&x| x * x).sum();
        assert!((norm_sq.sqrt() - 1.0).abs() < 1e-6);

        // Verify binary output length
        let bin = mgr.compute_profile_bin().expect("bin");
        assert_eq!(bin.len(), PROFILE_BIN_BYTES);
    }

    #[test]
    fn test_sample_lifecycle_and_persistence() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("profiles");
        {
            let mut mgr = VoiceSampleManager::new(&dir);
            assert!(mgr.list_samples().is_empty());
            assert!(mgr.compute_profile_embedding().is_none());

            mgr.add_sample(
                VoiceSample::new(
                    "s1",
                    "2026-10-05T00:00:00Z",
                    "Sample 1",
                    None,
                    make_unit_vector(5),
                )
                .expect("s1"),
            )
            .expect("add 1");
            mgr.add_sample(
                VoiceSample::new(
                    "s2",
                    "2026-10-05T00:00:01Z",
                    "Sample 2",
                    None,
                    make_unit_vector(6),
                )
                .expect("s2"),
            )
            .expect("add 2");

            assert_eq!(mgr.list_samples().len(), 2);
            assert!(dir.join(SAMPLES_FILE_NAME).is_file());
            assert!(dir.join(PROFILE_BIN_FILE_NAME).is_file());
        }

        // Reload from disk
        let mut reloaded = VoiceSampleManager::load(&dir).expect("load");
        assert_eq!(reloaded.list_samples().len(), 2);
        assert!(reloaded.get_sample("s1").is_some());
        assert!(reloaded.get_sample("s2").is_some());

        // Delete sample 1
        assert!(reloaded.delete_sample("s1", false).expect("delete"));
        assert_eq!(reloaded.list_samples().len(), 1);
        assert!(reloaded.get_sample("s1").is_none());
        assert!(reloaded.get_sample("s2").is_some());

        // Deleting non-existent returns false
        assert!(!reloaded.delete_sample("s1", false).expect("delete missing"));

        // Toggle active
        reloaded
            .set_sample_active("s2", false)
            .expect("set active false");
        assert!(reloaded.compute_profile_embedding().is_none());

        reloaded
            .set_sample_active("s2", true)
            .expect("set active true");
        assert!(reloaded.compute_profile_embedding().is_some());
    }

    #[test]
    fn test_delete_sample_removes_audio_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir = temp.path().join("profiles");
        let audio_path = temp.path().join("sample1.wav");
        fs::write(&audio_path, b"RIFF dummy wav audio data").expect("write audio");
        assert!(audio_path.is_file());

        let mut mgr = VoiceSampleManager::new(&dir);
        let sample = VoiceSample::new(
            "s_audio",
            "2026-10-05T00:00:00Z",
            "Audio Sample",
            Some(audio_path.to_str().expect("path").to_string()),
            make_unit_vector(3),
        )
        .expect("sample");
        mgr.add_sample(sample).expect("add");

        assert!(audio_path.is_file());
        mgr.delete_sample("s_audio", true).expect("delete");
        assert!(!audio_path.exists(), "Audio file must be deleted");
    }
}
