use std::{fmt, fs, io::Write, path::Path};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::InferenceError;

pub const PROFILE_SCHEMA_VERSION: u32 = 1;
pub const FILM_HIDDEN_DIM: usize = 256;
pub const NUM_ERB_BANDS: usize = 32;
pub const MIN_EQ_GAIN_DB: f32 = -6.0;
pub const MAX_EQ_GAIN_DB: f32 = 12.0;

const MIN_GAMMA: f32 = 0.001;
const MAX_GAMMA: f32 = 100.0;
const MIN_BETA: f32 = -50.0;
const MAX_BETA: f32 = 50.0;

#[derive(Debug)]
pub enum VoiceProfileError {
    DimensionMismatch {
        field: &'static str,
        expected: usize,
        actual: usize,
    },
    NonFiniteValue {
        field: &'static str,
        index: usize,
    },
    ValueOutOfRange {
        field: &'static str,
        value: f32,
        min: f32,
        max: f32,
    },
    IntegrityMismatch {
        expected: String,
        actual: String,
    },
    UnsupportedVersion(u32),
    Serialization(String),
    Deserialization(String),
    InsecurePermissions(String),
    Io(std::io::Error),
}

impl fmt::Display for VoiceProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionMismatch {
                field,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "dimension mismatch for {field}: expected {expected}, got {actual}"
                )
            }
            Self::NonFiniteValue { field, index } => {
                write!(f, "non-finite value in {field} at index {index}")
            }
            Self::ValueOutOfRange {
                field,
                value,
                min,
                max,
            } => {
                write!(
                    f,
                    "{field} value {value} out of allowed range [{min}, {max}]"
                )
            }
            Self::IntegrityMismatch { expected, actual } => {
                write!(
                    f,
                    "voice profile integrity hash mismatch: expected {expected}, calculated {actual}"
                )
            }
            Self::UnsupportedVersion(v) => write!(f, "unsupported profile schema version {v}"),
            Self::Serialization(err) => write!(f, "failed to serialize profile: {err}"),
            Self::Deserialization(err) => write!(f, "failed to deserialize profile: {err}"),
            Self::InsecurePermissions(msg) => write!(f, "insecure file permissions: {msg}"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for VoiceProfileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

impl From<std::io::Error> for VoiceProfileError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err)
    }
}

impl From<VoiceProfileError> for InferenceError {
    fn from(err: VoiceProfileError) -> Self {
        Self::InputContract(err.to_string())
    }
}

/// `FiLM` conditioning vectors applied to `DeepFilterNet3` encoder and DF decoder GRUs.
///
/// In identity mode, `gamma = 1.0` and `beta = 0.0` for all 256 hidden channels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FiLMVectors {
    pub gamma_enc: Vec<f32>,
    pub beta_enc: Vec<f32>,
    pub gamma_df: Vec<f32>,
    pub beta_df: Vec<f32>,
}

impl FiLMVectors {
    /// Creates a neutral (identity) set of `FiLM` vectors.
    ///
    /// Evaluates mathematically identical to the unconditioned base model.
    pub fn identity() -> Self {
        Self {
            gamma_enc: vec![1.0; FILM_HIDDEN_DIM],
            beta_enc: vec![0.0; FILM_HIDDEN_DIM],
            gamma_df: vec![1.0; FILM_HIDDEN_DIM],
            beta_df: vec![0.0; FILM_HIDDEN_DIM],
        }
    }

    /// Checks if all vectors are strictly at their identity values (gamma == 1.0, beta == 0.0).
    pub fn is_identity(&self) -> bool {
        self.gamma_enc
            .iter()
            .all(|&v| (v - 1.0).abs() < f32::EPSILON)
            && self.beta_enc.iter().all(|&v| v.abs() < f32::EPSILON)
            && self
                .gamma_df
                .iter()
                .all(|&v| (v - 1.0).abs() < f32::EPSILON)
            && self.beta_df.iter().all(|&v| v.abs() < f32::EPSILON)
    }

    /// Creates and validates a new set of `FiLM` vectors.
    pub fn new(
        gamma_enc: Vec<f32>,
        beta_enc: Vec<f32>,
        gamma_df: Vec<f32>,
        beta_df: Vec<f32>,
    ) -> Result<Self, VoiceProfileError> {
        let instance = Self {
            gamma_enc,
            beta_enc,
            gamma_df,
            beta_df,
        };
        instance.validate()?;
        Ok(instance)
    }

    /// Validates dimensions and bounds of all conditioning vectors.
    pub fn validate(&self) -> Result<(), VoiceProfileError> {
        Self::validate_vector("gamma_enc", &self.gamma_enc, MIN_GAMMA, MAX_GAMMA)?;
        Self::validate_vector("beta_enc", &self.beta_enc, MIN_BETA, MAX_BETA)?;
        Self::validate_vector("gamma_df", &self.gamma_df, MIN_GAMMA, MAX_GAMMA)?;
        Self::validate_vector("beta_df", &self.beta_df, MIN_BETA, MAX_BETA)?;
        Ok(())
    }

    fn validate_vector(
        field: &'static str,
        vec: &[f32],
        min: f32,
        max: f32,
    ) -> Result<(), VoiceProfileError> {
        if vec.len() != FILM_HIDDEN_DIM {
            return Err(VoiceProfileError::DimensionMismatch {
                field,
                expected: FILM_HIDDEN_DIM,
                actual: vec.len(),
            });
        }
        for (index, &val) in vec.iter().enumerate() {
            if !val.is_finite() {
                return Err(VoiceProfileError::NonFiniteValue { field, index });
            }
            if !(min..=max).contains(&val) {
                return Err(VoiceProfileError::ValueOutOfRange {
                    field,
                    value: val,
                    min,
                    max,
                });
            }
        }
        Ok(())
    }
}

/// Equalization gains per ERB frequency band (in dB), constrained to `[-6.0, +12.0]` dB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandGains {
    pub gains_db: [f32; NUM_ERB_BANDS],
}

impl BandGains {
    /// Creates a neutral (flat 0 dB) equalization filter.
    pub const fn neutral() -> Self {
        Self {
            gains_db: [0.0; NUM_ERB_BANDS],
        }
    }

    /// Checks if all band gains are 0 dB (flat).
    pub fn is_neutral(&self) -> bool {
        self.gains_db.iter().all(|&g| g.abs() < f32::EPSILON)
    }

    /// Creates and validates `BandGains` from a fixed array of decibel gains.
    pub fn from_array(gains_db: [f32; NUM_ERB_BANDS]) -> Result<Self, VoiceProfileError> {
        let instance = Self { gains_db };
        instance.validate()?;
        Ok(instance)
    }

    /// Creates and validates `BandGains` from a slice of decibel gains.
    pub fn from_slice(slice: &[f32]) -> Result<Self, VoiceProfileError> {
        if slice.len() != NUM_ERB_BANDS {
            return Err(VoiceProfileError::DimensionMismatch {
                field: "gains_db",
                expected: NUM_ERB_BANDS,
                actual: slice.len(),
            });
        }
        let mut gains_db = [0.0; NUM_ERB_BANDS];
        gains_db.copy_from_slice(slice);
        Self::from_array(gains_db)
    }

    /// Creates `BandGains` clamping any out-of-range values into `[-6.0, +12.0]` dB.
    pub fn clamped(slice: &[f32]) -> Result<Self, VoiceProfileError> {
        if slice.len() != NUM_ERB_BANDS {
            return Err(VoiceProfileError::DimensionMismatch {
                field: "gains_db",
                expected: NUM_ERB_BANDS,
                actual: slice.len(),
            });
        }
        let mut gains_db = [0.0; NUM_ERB_BANDS];
        for (i, &val) in slice.iter().enumerate() {
            if !val.is_finite() {
                return Err(VoiceProfileError::NonFiniteValue {
                    field: "gains_db",
                    index: i,
                });
            }
            gains_db[i] = val.clamp(MIN_EQ_GAIN_DB, MAX_EQ_GAIN_DB);
        }
        Ok(Self { gains_db })
    }

    /// Converts decibel gains to linear multipliers (`10^(gain_db / 20)`).
    pub fn linear_factors(&self) -> [f32; NUM_ERB_BANDS] {
        let mut factors = [1.0; NUM_ERB_BANDS];
        for (i, &db) in self.gains_db.iter().enumerate() {
            factors[i] = 10.0f32.powf(db / 20.0);
        }
        factors
    }

    /// Validates that all band gains are finite and within `[-6.0, +12.0]` dB.
    pub fn validate(&self) -> Result<(), VoiceProfileError> {
        for (index, &gain) in self.gains_db.iter().enumerate() {
            if !gain.is_finite() {
                return Err(VoiceProfileError::NonFiniteValue {
                    field: "gains_db",
                    index,
                });
            }
            if !(MIN_EQ_GAIN_DB..=MAX_EQ_GAIN_DB).contains(&gain) {
                return Err(VoiceProfileError::ValueOutOfRange {
                    field: "gains_db",
                    value: gain,
                    min: MIN_EQ_GAIN_DB,
                    max: MAX_EQ_GAIN_DB,
                });
            }
        }
        Ok(())
    }
}

/// A registered speaker profile containing `FiLM` conditioning vectors and optional Neural EQ gains.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceProfile {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub created_at_utc: String,
    pub film: FiLMVectors,
    pub eq: Option<BandGains>,
    pub integrity_hash: String,
}

#[derive(Serialize)]
struct CanonicalProfilePayload<'a> {
    version: u32,
    id: &'a str,
    name: &'a str,
    created_at_utc: &'a str,
    film: &'a FiLMVectors,
    eq: &'a Option<BandGains>,
}

impl VoiceProfile {
    /// Creates a neutral (identity) speaker profile with default identity `FiLM` vectors.
    pub fn identity(
        id: impl Into<String>,
        name: impl Into<String>,
        created_at_utc: impl Into<String>,
    ) -> Result<Self, VoiceProfileError> {
        Self::new(
            id,
            name,
            created_at_utc,
            FiLMVectors::identity(),
            Some(BandGains::neutral()),
        )
    }

    /// Creates and validates a new `VoiceProfile`, computing its integrity hash.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        created_at_utc: impl Into<String>,
        film: FiLMVectors,
        eq: Option<BandGains>,
    ) -> Result<Self, VoiceProfileError> {
        film.validate()?;
        if let Some(ref band_gains) = eq {
            band_gains.validate()?;
        }

        let mut profile = Self {
            version: PROFILE_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            created_at_utc: created_at_utc.into(),
            film,
            eq,
            integrity_hash: String::new(),
        };

        profile.integrity_hash = profile.compute_integrity_hash()?;
        Ok(profile)
    }

    /// Checks if this profile represents a neutral identity operation.
    pub fn is_neutral(&self) -> bool {
        self.film.is_identity() && self.eq.as_ref().is_none_or(BandGains::is_neutral)
    }

    /// Computes canonical SHA-256 integrity hash of the profile contents.
    pub fn compute_integrity_hash(&self) -> Result<String, VoiceProfileError> {
        let payload = CanonicalProfilePayload {
            version: self.version,
            id: &self.id,
            name: &self.name,
            created_at_utc: &self.created_at_utc,
            film: &self.film,
            eq: &self.eq,
        };

        let json_bytes = serde_json::to_vec(&payload)
            .map_err(|e| VoiceProfileError::Serialization(e.to_string()))?;
        let hash = Sha256::digest(&json_bytes);
        Ok(format!("{hash:x}"))
    }

    /// Verifies the profile against its declared integrity hash and schema version.
    pub fn verify_integrity(&self) -> Result<(), VoiceProfileError> {
        if self.version != PROFILE_SCHEMA_VERSION {
            return Err(VoiceProfileError::UnsupportedVersion(self.version));
        }
        self.film.validate()?;
        if let Some(ref band_gains) = self.eq {
            band_gains.validate()?;
        }
        let calculated = self.compute_integrity_hash()?;
        if calculated != self.integrity_hash {
            return Err(VoiceProfileError::IntegrityMismatch {
                expected: self.integrity_hash.clone(),
                actual: calculated,
            });
        }
        Ok(())
    }

    /// Serializes profile to JSON string.
    pub fn to_json(&self) -> Result<String, VoiceProfileError> {
        self.verify_integrity()?;
        serde_json::to_string_pretty(self)
            .map_err(|e| VoiceProfileError::Serialization(e.to_string()))
    }

    /// Deserializes and validates a profile from JSON string.
    pub fn from_json(json_str: &str) -> Result<Self, VoiceProfileError> {
        let profile: Self = serde_json::from_str(json_str)
            .map_err(|e| VoiceProfileError::Deserialization(e.to_string()))?;
        profile.verify_integrity()?;
        Ok(profile)
    }

    /// Atomically saves the profile to disk with strict 0600 file permissions (POSIX).
    pub fn save_to_file_secure(&self, path: &Path) -> Result<(), VoiceProfileError> {
        self.verify_integrity()?;
        let json_str = self.to_json()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension(format!(
            "tmp.{}.{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));

        // The file is created with mode 0600 from the first byte: there is no window in which
        // biometric data is readable by group/others (unlike write-then-chmod).
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let write_result = options.open(&tmp_path).and_then(|mut file| {
            file.write_all(json_str.as_bytes())?;
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

    /// Loads and validates a profile from disk, verifying 0600 permissions on Unix.
    pub fn load_from_file_secure(path: &Path) -> Result<Self, VoiceProfileError> {
        let metadata = fs::symlink_metadata(path)?;
        if !metadata.file_type().is_file() {
            return Err(VoiceProfileError::InsecurePermissions(
                "profile path is not a regular file".to_owned(),
            ));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode();
            if mode & 0o077 != 0 {
                return Err(VoiceProfileError::InsecurePermissions(format!(
                    "file mode {mode:#o} has group/world permissions (expected 0600)"
                )));
            }
        }

        let bytes = fs::read(path)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|e| VoiceProfileError::Deserialization(e.to_string()))?;
        Self::from_json(text)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn test_film_identity_properties() {
        let film = FiLMVectors::identity();
        assert!(film.is_identity());
        assert_eq!(film.gamma_enc.len(), FILM_HIDDEN_DIM);
        assert_eq!(film.beta_enc.len(), FILM_HIDDEN_DIM);
        assert_eq!(film.gamma_df.len(), FILM_HIDDEN_DIM);
        assert_eq!(film.beta_df.len(), FILM_HIDDEN_DIM);
        assert!(film.validate().is_ok());

        assert!((film.gamma_enc[0] - 1.0).abs() < f32::EPSILON);
        assert!(film.beta_enc[0].abs() < f32::EPSILON);
    }

    #[test]
    fn test_film_dimension_mismatch() {
        let mut film = FiLMVectors::identity();
        film.gamma_enc.pop();
        assert!(matches!(
            film.validate(),
            Err(VoiceProfileError::DimensionMismatch {
                field: "gamma_enc",
                ..
            })
        ));
    }

    #[test]
    fn test_film_non_finite_rejection() {
        let mut film = FiLMVectors::identity();
        film.beta_enc[10] = f32::NAN;
        assert!(matches!(
            film.validate(),
            Err(VoiceProfileError::NonFiniteValue {
                field: "beta_enc",
                index: 10
            })
        ));
    }

    #[test]
    fn test_film_out_of_range_rejection() {
        let mut film = FiLMVectors::identity();
        film.gamma_df[5] = -0.5; // strictly positive required
        assert!(matches!(
            film.validate(),
            Err(VoiceProfileError::ValueOutOfRange {
                field: "gamma_df",
                ..
            })
        ));
    }

    #[test]
    fn test_band_gains_neutral() {
        let eq = BandGains::neutral();
        assert!(eq.is_neutral());
        assert_eq!(eq.gains_db.len(), NUM_ERB_BANDS);
        assert!(eq.validate().is_ok());

        let linear = eq.linear_factors();
        for factor in linear {
            assert!((factor - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn test_band_gains_clamped() {
        let mut raw = [0.0; NUM_ERB_BANDS];
        raw[0] = -10.0; // below -6 dB
        raw[1] = 20.0; // above +12 dB
        raw[2] = 3.0;

        let eq = BandGains::clamped(&raw).expect("clamped should succeed");
        assert!((eq.gains_db[0] - MIN_EQ_GAIN_DB).abs() < f32::EPSILON);
        assert!((eq.gains_db[1] - MAX_EQ_GAIN_DB).abs() < f32::EPSILON);
        assert!((eq.gains_db[2] - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_band_gains_out_of_range_rejection() {
        let mut raw = [0.0; NUM_ERB_BANDS];
        raw[5] = 12.1;
        assert!(matches!(
            BandGains::from_array(raw),
            Err(VoiceProfileError::ValueOutOfRange {
                field: "gains_db",
                ..
            })
        ));
    }

    #[test]
    fn test_voice_profile_identity_roundtrip() {
        let profile = VoiceProfile::identity("spk-001", "Primary Speaker", "2026-10-02T12:00:00Z")
            .expect("identity creation should succeed");

        assert!(profile.is_neutral());
        assert!(!profile.integrity_hash.is_empty());

        let json = profile.to_json().expect("serialization should succeed");
        let decoded = VoiceProfile::from_json(&json).expect("deserialization should succeed");

        assert_eq!(profile, decoded);
        assert!(decoded.is_neutral());
    }

    #[test]
    fn test_voice_profile_tamper_detection() {
        let profile = VoiceProfile::identity("spk-001", "Primary Speaker", "2026-10-02T12:00:00Z")
            .expect("identity creation should succeed");

        let mut json = profile.to_json().expect("serialization should succeed");
        // Tamper with name
        json = json.replace("Primary Speaker", "Imposter");

        assert!(matches!(
            VoiceProfile::from_json(&json),
            Err(VoiceProfileError::IntegrityMismatch { .. })
        ));
    }

    #[test]
    fn test_secure_file_storage_roundtrip() {
        let temp_dir = tempfile::tempdir().expect("tempdir creation");
        let profile_path = temp_dir.path().join("secure_profile.json");

        let profile =
            VoiceProfile::identity("spk-secure", "Secure Speaker", "2026-10-02T12:00:00Z")
                .expect("profile creation");

        profile
            .save_to_file_secure(&profile_path)
            .expect("save_to_file_secure should succeed");

        let loaded = VoiceProfile::load_from_file_secure(&profile_path)
            .expect("load_from_file_secure should succeed");

        assert_eq!(profile, loaded);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&profile_path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600, "profile must be stored with mode 0600");
            // Deliberately set insecure permissions (0644) and verify rejection
            let insecure = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&profile_path, insecure).expect("set_permissions");

            assert!(matches!(
                VoiceProfile::load_from_file_secure(&profile_path),
                Err(VoiceProfileError::InsecurePermissions(_))
            ));
        }
    }
}
