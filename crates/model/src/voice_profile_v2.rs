//! Voice profile schema v2: the pDFNet3 Pro v3 speaker conditioning.
//!
//! The v1 profile carried `FiLM` `gamma`/`beta` vectors ([`crate::voice_profile::VoiceProfile`]).
//! The v3 model is conditioned by one L2-normalized ECAPA speaker embedding (`f32`, 192 values)
//! produced by the v3 `enrollment.onnx` (output `speaker_embedding`, `[1, 192]`). The profile
//! also keeps the calibrated microphone EQ.
//!
//! # On-disk format (`active_profile.json`, mode 0600)
//!
//! ```json
//! {
//!   "version": 2,
//!   "id": "p-1", "name": "Voice", "created_at_utc": "2026-11-09T12:00:00Z",
//!   "model_id": "pdfnet3-pro-v3-full", "model_version": 3,
//!   "embedding_dim": 192,
//!   "speaker_embedding_b64": "<192 x f32 little-endian, standard base64 with padding>",
//!   "eq": { "gains_db": [32 floats] } | null,
//!   "integrity_hash": "<sha256 hex of the canonical JSON of every field above>"
//! }
//! ```
//!
//! A v1 file is never applied: [`StoredProfile::from_json`] reports it as
//! [`StoredProfile::Outdated`] so the service can ask the user to regenerate it from the samples
//! already stored (the samples stay valid).

use std::path::Path;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::voice_profile::{
    BandGains, VoiceProfile, VoiceProfileError, probe_schema_version, read_secure_utf8,
    write_secure_atomic,
};

pub const PROFILE_V2_SCHEMA_VERSION: u32 = 2;
/// Model the embedding belongs to; a profile of another model id is not applicable.
pub const PDFNET3_PRO_V3_MODEL_ID: &str = "pdfnet3-pro-v3-full";
/// Human-readable name shown in the UI and the status.
pub const PDFNET3_PRO_V3_MODEL_LABEL: &str = "pDFNet3 Pro v3";
pub const PDFNET3_PRO_V3_MODEL_VERSION: u32 = 3;
/// Length of the ECAPA speaker embedding.
pub const SPEAKER_EMBEDDING_DIM: usize = 192;
/// Largest deviation of the embedding L2 norm from 1.0 accepted in a stored profile.
pub const EMBEDDING_NORM_TOLERANCE: f32 = 1e-3;

/// ECAPA speaker embedding: 192 finite floats with unit L2 norm.
#[derive(Clone, PartialEq)]
pub struct SpeakerEmbedding(Box<[f32; SPEAKER_EMBEDDING_DIM]>);

impl std::fmt::Debug for SpeakerEmbedding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Biometric data: never printed.
        f.write_str("SpeakerEmbedding(..)")
    }
}

fn l2_norm(values: &[f32]) -> f32 {
    let sum: f64 = values.iter().map(|v| f64::from(*v) * f64::from(*v)).sum();
    #[allow(clippy::cast_possible_truncation)]
    let norm = sum.sqrt() as f32;
    norm
}

impl SpeakerEmbedding {
    /// Validates dimension, finiteness and unit norm (within [`EMBEDDING_NORM_TOLERANCE`]).
    pub fn new(values: &[f32]) -> Result<Self, VoiceProfileError> {
        let array = Self::check_shape_and_finite(values)?;
        let norm = l2_norm(&array[..]);
        if (norm - 1.0).abs() > EMBEDDING_NORM_TOLERANCE {
            return Err(VoiceProfileError::ValueOutOfRange {
                field: "speaker_embedding_norm",
                value: norm,
                min: 1.0 - EMBEDDING_NORM_TOLERANCE,
                max: 1.0 + EMBEDDING_NORM_TOLERANCE,
            });
        }
        Ok(Self(array))
    }

    /// Takes a raw model output (any positive norm) and L2-normalizes it.
    pub fn from_raw_normalizing(values: &[f32]) -> Result<Self, VoiceProfileError> {
        let mut array = Self::check_shape_and_finite(values)?;
        let norm = l2_norm(&array[..]);
        if !(norm.is_finite() && norm > 1e-6) {
            return Err(VoiceProfileError::ValueOutOfRange {
                field: "speaker_embedding_norm",
                value: norm,
                min: 1e-6,
                max: f32::MAX,
            });
        }
        for value in array.iter_mut() {
            *value /= norm;
        }
        Ok(Self(array))
    }

    fn check_shape_and_finite(
        values: &[f32],
    ) -> Result<Box<[f32; SPEAKER_EMBEDDING_DIM]>, VoiceProfileError> {
        if values.len() != SPEAKER_EMBEDDING_DIM {
            return Err(VoiceProfileError::DimensionMismatch {
                field: "speaker_embedding",
                expected: SPEAKER_EMBEDDING_DIM,
                actual: values.len(),
            });
        }
        if let Some(index) = values.iter().position(|v| !v.is_finite()) {
            return Err(VoiceProfileError::NonFiniteValue {
                field: "speaker_embedding",
                index,
            });
        }
        let mut array = Box::new([0.0_f32; SPEAKER_EMBEDDING_DIM]);
        array.copy_from_slice(values);
        Ok(array)
    }

    #[must_use]
    pub fn as_slice(&self) -> &[f32; SPEAKER_EMBEDDING_DIM] {
        &self.0
    }

    /// Standard base64 (with padding) of the 192 values as `f32` little-endian (768 bytes).
    #[must_use]
    pub fn to_base64(&self) -> String {
        let mut bytes = Vec::with_capacity(SPEAKER_EMBEDDING_DIM * 4);
        for value in self.0.iter() {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    /// Inverse of [`Self::to_base64`]; validates like [`Self::new`].
    pub fn from_base64(text: &str) -> Result<Self, VoiceProfileError> {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(text.trim())
            .map_err(|e| VoiceProfileError::Deserialization(e.to_string()))?;
        if bytes.len() != SPEAKER_EMBEDDING_DIM * 4 {
            return Err(VoiceProfileError::DimensionMismatch {
                field: "speaker_embedding_b64",
                expected: SPEAKER_EMBEDDING_DIM * 4,
                actual: bytes.len(),
            });
        }
        let values: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Self::new(&values)
    }

    /// Scales the embedding contribution by a voice-isolation intensity (0-100 %): the vector
    /// returned is `alpha * e` (not renormalized), `alpha = intensity / 100`. Provided for
    /// consumers that blend toward "no conditioning"; the stored embedding is never changed.
    #[must_use]
    pub fn scaled(&self, intensity_percent: u8) -> Vec<f32> {
        let alpha = f32::from(intensity_percent.min(100)) / 100.0;
        self.0.iter().map(|v| v * alpha).collect()
    }
}

/// A registered speaker profile for the pDFNet3 Pro v3: embedding plus optional microphone EQ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceProfileV2 {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub created_at_utc: String,
    pub model_id: String,
    pub model_version: u32,
    pub embedding_dim: u32,
    pub speaker_embedding_b64: String,
    pub eq: Option<BandGains>,
    pub integrity_hash: String,
}

#[derive(Serialize)]
struct CanonicalV2<'a> {
    version: u32,
    id: &'a str,
    name: &'a str,
    created_at_utc: &'a str,
    model_id: &'a str,
    model_version: u32,
    embedding_dim: u32,
    speaker_embedding_b64: &'a str,
    eq: &'a Option<BandGains>,
}

impl VoiceProfileV2 {
    /// Creates and seals a profile for the pDFNet3 Pro v3.
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        created_at_utc: impl Into<String>,
        embedding: &SpeakerEmbedding,
        eq: Option<BandGains>,
    ) -> Result<Self, VoiceProfileError> {
        if let Some(gains) = &eq {
            gains.validate()?;
        }
        let mut profile = Self {
            version: PROFILE_V2_SCHEMA_VERSION,
            id: id.into(),
            name: name.into(),
            created_at_utc: created_at_utc.into(),
            model_id: PDFNET3_PRO_V3_MODEL_ID.to_owned(),
            model_version: PDFNET3_PRO_V3_MODEL_VERSION,
            embedding_dim: u32::try_from(SPEAKER_EMBEDDING_DIM).unwrap_or(u32::MAX),
            speaker_embedding_b64: embedding.to_base64(),
            eq,
            integrity_hash: String::new(),
        };
        profile.integrity_hash = profile.compute_integrity_hash()?;
        Ok(profile)
    }

    /// The decoded, validated embedding.
    pub fn speaker_embedding(&self) -> Result<SpeakerEmbedding, VoiceProfileError> {
        SpeakerEmbedding::from_base64(&self.speaker_embedding_b64)
    }

    pub fn compute_integrity_hash(&self) -> Result<String, VoiceProfileError> {
        let payload = CanonicalV2 {
            version: self.version,
            id: &self.id,
            name: &self.name,
            created_at_utc: &self.created_at_utc,
            model_id: &self.model_id,
            model_version: self.model_version,
            embedding_dim: self.embedding_dim,
            speaker_embedding_b64: &self.speaker_embedding_b64,
            eq: &self.eq,
        };
        let bytes = serde_json::to_vec(&payload)
            .map_err(|e| VoiceProfileError::Serialization(e.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    /// Schema version, model id, embedding and EQ validity, then the integrity hash.
    pub fn verify_integrity(&self) -> Result<(), VoiceProfileError> {
        if self.version != PROFILE_V2_SCHEMA_VERSION {
            return Err(VoiceProfileError::UnsupportedVersion(self.version));
        }
        if self.model_id != PDFNET3_PRO_V3_MODEL_ID
            || self.model_version != PDFNET3_PRO_V3_MODEL_VERSION
        {
            return Err(VoiceProfileError::Deserialization(
                "profile belongs to another model".to_owned(),
            ));
        }
        if usize::try_from(self.embedding_dim) != Ok(SPEAKER_EMBEDDING_DIM) {
            return Err(VoiceProfileError::DimensionMismatch {
                field: "embedding_dim",
                expected: SPEAKER_EMBEDDING_DIM,
                actual: usize::try_from(self.embedding_dim).unwrap_or(usize::MAX),
            });
        }
        self.speaker_embedding()?;
        if let Some(gains) = &self.eq {
            gains.validate()?;
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

    pub fn to_json(&self) -> Result<String, VoiceProfileError> {
        self.verify_integrity()?;
        serde_json::to_string_pretty(self)
            .map_err(|e| VoiceProfileError::Serialization(e.to_string()))
    }

    /// Parses and verifies a v2 profile. A v1 file is `UnsupportedVersion(1)`; use
    /// [`StoredProfile::from_json`] to classify any stored profile.
    pub fn from_json(text: &str) -> Result<Self, VoiceProfileError> {
        if let Some(version) = probe_schema_version(text)
            && version != PROFILE_V2_SCHEMA_VERSION
        {
            return Err(VoiceProfileError::UnsupportedVersion(version));
        }
        let profile: Self = serde_json::from_str(text)
            .map_err(|e| VoiceProfileError::Deserialization(e.to_string()))?;
        profile.verify_integrity()?;
        Ok(profile)
    }

    pub fn save_to_file_secure(&self, path: &Path) -> Result<(), VoiceProfileError> {
        let text = self.to_json()?;
        write_secure_atomic(path, &text)
    }

    pub fn load_from_file_secure(path: &Path) -> Result<Self, VoiceProfileError> {
        Self::from_json(&read_secure_utf8(path)?)
    }
}

/// Identity of a stored v1 (`FiLM`) profile that can no longer be applied. No conditioning data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutdatedProfile {
    pub id: String,
    pub name: String,
    pub created_at_utc: String,
    /// Schema version found on disk (1).
    pub version: u32,
}

/// What `active_profile.json` holds.
#[derive(Debug, Clone, PartialEq)]
pub enum StoredProfile {
    /// A valid v2 profile for the pDFNet3 Pro v3.
    Current(Box<VoiceProfileV2>),
    /// A valid v1 (gamma/beta) profile: it must be regenerated from the stored samples.
    Outdated(OutdatedProfile),
}

impl StoredProfile {
    /// Classifies by the `version` field. A v1 file is checked with the v1 integrity rules (a
    /// tampered or broken one is an error, not "outdated"); any other version is
    /// `UnsupportedVersion`.
    pub fn from_json(text: &str) -> Result<Self, VoiceProfileError> {
        let value: serde_json::Value = serde_json::from_str(text)
            .map_err(|e| VoiceProfileError::Deserialization(e.to_string()))?;
        let version = value
            .get("version")
            .and_then(serde_json::Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| VoiceProfileError::Deserialization("missing version".to_owned()))?;
        match version {
            PROFILE_V2_SCHEMA_VERSION => {
                VoiceProfileV2::from_json(text).map(|p| Self::Current(Box::new(p)))
            }
            1 => {
                let legacy = VoiceProfile::from_json(text)?;
                Ok(Self::Outdated(OutdatedProfile {
                    id: legacy.id,
                    name: legacy.name,
                    created_at_utc: legacy.created_at_utc,
                    version,
                }))
            }
            other => Err(VoiceProfileError::UnsupportedVersion(other)),
        }
    }

    pub fn load_from_file_secure(path: &Path) -> Result<Self, VoiceProfileError> {
        Self::from_json(&read_secure_utf8(path)?)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::voice_profile::NUM_ERB_BANDS;

    pub fn unit_embedding(seed: f32) -> SpeakerEmbedding {
        let raw: Vec<f32> = (1_u16..)
            .take(SPEAKER_EMBEDDING_DIM)
            .map(|n| (f32::from(n) * seed).sin())
            .collect();
        SpeakerEmbedding::from_raw_normalizing(&raw).expect("embedding")
    }

    fn profile() -> VoiceProfileV2 {
        let mut gains = [0.0_f32; NUM_ERB_BANDS];
        gains[3] = 2.5;
        VoiceProfileV2::new(
            "p-1",
            "Voz",
            "2026-11-09T12:00:00Z",
            &unit_embedding(0.37),
            Some(BandGains::from_array(gains).expect("gains")),
        )
        .expect("profile")
    }

    #[test]
    fn embedding_base64_is_f32_le_and_roundtrips_exactly() {
        let e = unit_embedding(0.5);
        let b64 = e.to_base64();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .expect("b64");
        assert_eq!(bytes.len(), 768);
        assert_eq!(&bytes[..4], &e.as_slice()[0].to_le_bytes());
        assert_eq!(SpeakerEmbedding::from_base64(&b64).expect("decode"), e);
    }

    #[test]
    fn embedding_rejects_wrong_size_non_finite_and_non_unit_norm() {
        assert!(matches!(
            SpeakerEmbedding::new(&[0.1; 191]),
            Err(VoiceProfileError::DimensionMismatch { .. })
        ));
        let mut v = vec![0.0_f32; SPEAKER_EMBEDDING_DIM];
        v[7] = f32::NAN;
        assert!(matches!(
            SpeakerEmbedding::new(&v),
            Err(VoiceProfileError::NonFiniteValue { index: 7, .. })
        ));
        assert!(matches!(
            SpeakerEmbedding::new(&[0.5; SPEAKER_EMBEDDING_DIM]),
            Err(VoiceProfileError::ValueOutOfRange { .. })
        ));
        assert!(SpeakerEmbedding::from_raw_normalizing(&[0.0; SPEAKER_EMBEDDING_DIM]).is_err());
    }

    #[test]
    fn v2_profile_roundtrips_through_json_and_file() {
        let p = profile();
        assert_eq!(p.model_id, "pdfnet3-pro-v3-full");
        assert_eq!(p.embedding_dim, 192);
        let json = p.to_json().expect("json");
        assert!(json.contains("\"version\": 2"));
        assert!(json.contains("speaker_embedding_b64"));
        assert_eq!(VoiceProfileV2::from_json(&json).expect("back"), p);

        let dir = tempfile::tempdir().expect("tmp");
        let path = dir.path().join("active_profile.json");
        p.save_to_file_secure(&path).expect("save");
        assert_eq!(
            VoiceProfileV2::load_from_file_secure(&path).expect("load"),
            p
        );
        assert!(matches!(
            StoredProfile::load_from_file_secure(&path).expect("any"),
            StoredProfile::Current(c) if *c == p
        ));
    }

    #[test]
    fn tampering_is_detected() {
        let json = profile().to_json().expect("json");
        let tampered = json.replace("\"Voz\"", "\"Outra\"");
        assert!(matches!(
            VoiceProfileV2::from_json(&tampered),
            Err(VoiceProfileError::IntegrityMismatch { .. })
        ));
        let other_model = json.replace("pdfnet3-pro-v3-full", "pdfnet3-pro");
        assert!(VoiceProfileV2::from_json(&other_model).is_err());
    }

    #[test]
    fn v1_file_is_outdated_not_loaded() {
        let v1 = VoiceProfile::identity("old-1", "Antiga", "2026-10-02T12:00:00Z")
            .expect("v1")
            .to_json()
            .expect("json");
        assert!(matches!(
            VoiceProfileV2::from_json(&v1),
            Err(VoiceProfileError::UnsupportedVersion(1))
        ));
        match StoredProfile::from_json(&v1).expect("classified") {
            StoredProfile::Outdated(info) => {
                assert_eq!(info.id, "old-1");
                assert_eq!(info.name, "Antiga");
                assert_eq!(info.version, 1);
            }
            other @ StoredProfile::Current(_) => panic!("{other:?}"),
        }
    }

    #[test]
    fn tampered_v1_and_unknown_versions_are_errors() {
        let v1 = VoiceProfile::identity("old-1", "Antiga", "2026-10-02T12:00:00Z")
            .expect("v1")
            .to_json()
            .expect("json")
            .replace("Antiga", "Outra");
        assert!(matches!(
            StoredProfile::from_json(&v1),
            Err(VoiceProfileError::IntegrityMismatch { .. })
        ));
        assert!(matches!(
            StoredProfile::from_json("{\"version\": 9}"),
            Err(VoiceProfileError::UnsupportedVersion(9))
        ));
        assert!(StoredProfile::from_json("{}").is_err());
        assert!(StoredProfile::from_json("not json").is_err());
    }

    #[test]
    fn debug_never_prints_the_embedding() {
        let text = format!("{:?}", unit_embedding(0.3));
        assert_eq!(text, "SpeakerEmbedding(..)");
    }

    #[test]
    #[allow(clippy::float_cmp, clippy::suboptimal_flops)] // exact zeros/copies are the assertion
    fn scaled_embedding_follows_intensity() {
        let e = unit_embedding(0.3);
        assert!(e.scaled(0).iter().all(|v| *v == 0.0));
        assert_eq!(e.scaled(100), e.as_slice().to_vec());
        assert!((e.scaled(50)[0] - e.as_slice()[0] * 0.5).abs() < 1e-7);
    }
}
