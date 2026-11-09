//! Voice profile schema v2 (pDFNet3 Pro v3): reader for the filter.
//!
//! The format is owned by enrollment (`crates/model/src/voice_profile_v2.rs` on the enrollment
//! branch `feat/pro-v3-enroll`, documented there in `docs/ipc-v1.md`; summary in
//! `docs/virtual-mic-filter.md`). This crate does not
//! depend on that module (it does not exist on this branch's `crates/model`), so the parser and
//! the canonical integrity payload are **replicated field for field** here; the round-trip test
//! below pins the byte layout. Same file as v1: `<profiles>/active_profile.json`, mode 0600.
//!
//! ```json
//! { "version": 2, "id": "p-1", "name": "Voz", "created_at_utc": "2026-11-09T12:00:00Z",
//!   "model_id": "pdfnet3-pro-v3-full", "model_version": 3, "embedding_dim": 192,
//!   "speaker_embedding_b64": "<192 x f32 little-endian, standard base64 with padding>",
//!   "eq": { "gains_db": [32 floats] } | null,
//!   "integrity_hash": "<sha256 hex of serde_json::to_vec of every field above, in this order>" }
//! ```

use std::path::Path;

use base64::Engine as _;
use realtime_noise_model::{BandGains, VoiceProfile};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::engine::SPEAKER_EMBEDDING_DIM;

pub const PROFILE_V2_SCHEMA_VERSION: u32 = 2;
pub const PDFNET3_PRO_V3_MODEL_ID: &str = "pdfnet3-pro-v3-full";
pub const PDFNET3_PRO_V3_MODEL_VERSION: u32 = 3;
/// Largest deviation of the embedding L2 norm from 1.0 accepted.
pub const EMBEDDING_NORM_TOLERANCE: f32 = 1e-3;
const MAX_PROFILE_BYTES: u64 = 1024 * 1024;

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

fn l2_norm(values: &[f32]) -> f32 {
    let sum: f64 = values.iter().map(|v| f64::from(*v) * f64::from(*v)).sum();
    #[allow(clippy::cast_possible_truncation)]
    let norm = sum.sqrt() as f32;
    norm
}

/// Validates 192 finite values with unit L2 norm.
pub fn validate_embedding(values: &[f32]) -> Result<(), String> {
    if values.len() != SPEAKER_EMBEDDING_DIM {
        return Err(format!(
            "embedding holds {} values, want {SPEAKER_EMBEDDING_DIM}",
            values.len()
        ));
    }
    if values.iter().any(|v| !v.is_finite()) {
        return Err("embedding has a non-finite value".to_owned());
    }
    let norm = l2_norm(values);
    if (norm - 1.0).abs() > EMBEDDING_NORM_TOLERANCE {
        return Err(format!("embedding L2 norm {norm} is not 1"));
    }
    Ok(())
}

/// Standard base64 (with padding) of `f32` little-endian values.
#[must_use]
pub fn embedding_to_base64(values: &[f32]) -> String {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Inverse of [`embedding_to_base64`], validated by [`validate_embedding`].
pub fn embedding_from_base64(text: &str) -> Result<Vec<f32>, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .map_err(|e| format!("speaker_embedding_b64: {e}"))?;
    if bytes.len() != SPEAKER_EMBEDDING_DIM * 4 {
        return Err(format!(
            "speaker_embedding_b64 decodes to {} bytes, want 768",
            bytes.len()
        ));
    }
    let values: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    validate_embedding(&values)?;
    Ok(values)
}

impl VoiceProfileV2 {
    /// Builds and seals a profile (reference writer, used by tests).
    pub fn new(
        id: &str,
        name: &str,
        created_at_utc: &str,
        embedding: &[f32],
        eq: Option<BandGains>,
    ) -> Result<Self, String> {
        validate_embedding(embedding)?;
        let mut profile = Self {
            version: PROFILE_V2_SCHEMA_VERSION,
            id: id.to_owned(),
            name: name.to_owned(),
            created_at_utc: created_at_utc.to_owned(),
            model_id: PDFNET3_PRO_V3_MODEL_ID.to_owned(),
            model_version: PDFNET3_PRO_V3_MODEL_VERSION,
            embedding_dim: 192,
            speaker_embedding_b64: embedding_to_base64(embedding),
            eq,
            integrity_hash: String::new(),
        };
        profile.integrity_hash = profile.compute_integrity_hash()?;
        profile.verify()?;
        Ok(profile)
    }

    pub fn compute_integrity_hash(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(&CanonicalV2 {
            version: self.version,
            id: &self.id,
            name: &self.name,
            created_at_utc: &self.created_at_utc,
            model_id: &self.model_id,
            model_version: self.model_version,
            embedding_dim: self.embedding_dim,
            speaker_embedding_b64: &self.speaker_embedding_b64,
            eq: &self.eq,
        })
        .map_err(|e| e.to_string())?;
        Ok(format!("{:x}", Sha256::digest(&bytes)))
    }

    /// Decoded, validated embedding.
    pub fn speaker_embedding(&self) -> Result<Vec<f32>, String> {
        embedding_from_base64(&self.speaker_embedding_b64)
    }

    /// Version, model id/version, embedding dimension and value, EQ range, integrity hash.
    pub fn verify(&self) -> Result<(), String> {
        if self.version != PROFILE_V2_SCHEMA_VERSION {
            return Err(format!("schema version {}", self.version));
        }
        if self.model_id != PDFNET3_PRO_V3_MODEL_ID
            || self.model_version != PDFNET3_PRO_V3_MODEL_VERSION
        {
            return Err("profile belongs to another model".to_owned());
        }
        if usize::try_from(self.embedding_dim) != Ok(SPEAKER_EMBEDDING_DIM) {
            return Err(format!("embedding_dim {}", self.embedding_dim));
        }
        self.speaker_embedding()?;
        if let Some(gains) = &self.eq {
            gains.validate().map_err(|e| format!("eq: {e:?}"))?;
        }
        let calculated = self.compute_integrity_hash()?;
        if calculated == self.integrity_hash {
            Ok(())
        } else {
            Err(format!(
                "integrity hash mismatch ({} != {calculated})",
                self.integrity_hash
            ))
        }
    }

    pub fn from_json(text: &str) -> Result<Self, String> {
        let profile: Self = serde_json::from_str(text).map_err(|e| e.to_string())?;
        profile.verify()?;
        Ok(profile)
    }

    pub fn to_json(&self) -> Result<String, String> {
        self.verify()?;
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}

/// What `active_profile.json` holds, as the filter sees it.
#[derive(Debug, Clone, PartialEq)]
pub enum StoredProfile {
    /// A verified v2 profile.
    V2(Box<VoiceProfileV2>),
    /// A verified v1 (`FiLM`) profile. Outdated for the v3 model (neutral voice there); still
    /// applied by the v2 stateful model while that is the installed one.
    V1(Box<VoiceProfile>),
}

impl StoredProfile {
    /// Classifies by `version` and verifies with that version's rules: a tampered file of either
    /// version is an error, an unknown version too.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        match value.get("version").and_then(serde_json::Value::as_u64) {
            Some(2) => VoiceProfileV2::from_json(text).map(|p| Self::V2(Box::new(p))),
            Some(1) => VoiceProfile::from_json(text)
                .map(|p| Self::V1(Box::new(p)))
                .map_err(|e| format!("{e:?}")),
            other => Err(format!("unsupported profile version {other:?}")),
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::V2(p) => &p.id,
            Self::V1(p) => &p.id,
        }
    }

    /// Reads `path` if it exists: regular file (not a symlink), no group/other permission bits,
    /// size-capped, then [`Self::from_json`].
    pub fn load_secure(path: &Path) -> Result<Option<Self>, String> {
        use std::io::Read as _;
        let metadata = match std::fs::symlink_metadata(path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        if !metadata.file_type().is_file() {
            return Err("profile path is not a regular file".to_owned());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = metadata.permissions().mode();
            if mode & 0o077 != 0 {
                return Err(format!("profile mode {mode:#o} (want 0600)"));
            }
        }
        let mut text = String::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_PROFILE_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() as u64 > MAX_PROFILE_BYTES {
            return Err("profile file too large".to_owned());
        }
        Self::from_json(&text).map(Some)
    }
}

/// Writes `json` with mode 0600 from the first byte, then renames (tests and tools).
pub fn write_secure(path: &Path, json: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = options
        .open(&tmp)
        .and_then(|mut f| {
            f.write_all(json.as_bytes())?;
            f.sync_all()
        })
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// A deterministic unit-norm embedding for tests.
#[must_use]
pub fn test_embedding(seed: u32) -> Vec<f32> {
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let mut values: Vec<f32> = (0..SPEAKER_EMBEDDING_DIM)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            #[allow(clippy::cast_precision_loss)]
            let unit = (state as f32 / u32::MAX as f32) * 2.0 - 1.0;
            unit
        })
        .collect();
    let norm = l2_norm(&values);
    for v in &mut values {
        *v /= norm;
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> VoiceProfileV2 {
        VoiceProfileV2::new(
            "p-1",
            "Voz",
            "2026-11-09T12:00:00Z",
            &test_embedding(7),
            None,
        )
        .expect("profile")
    }

    #[test]
    fn round_trips_and_detects_tampering() {
        let p = profile();
        let json = p.to_json().expect("json");
        assert_eq!(VoiceProfileV2::from_json(&json).expect("parse"), p);
        assert_eq!(p.speaker_embedding().expect("embedding"), test_embedding(7));
        assert!(VoiceProfileV2::from_json(&json.replace("Voz", "Eva")).is_err());
    }

    /// Byte layout of the canonical payload, as written by enrollment's `CanonicalV2`.
    #[test]
    fn the_canonical_payload_is_the_documented_field_order() {
        let mut p = profile();
        p.speaker_embedding_b64 = "AA==".to_owned();
        p.eq = Some(BandGains::neutral());
        let expected = format!(
            "{{\"version\":2,\"id\":\"p-1\",\"name\":\"Voz\",\"created_at_utc\":\"2026-11-09T12:00:00Z\",\
             \"model_id\":\"pdfnet3-pro-v3-full\",\"model_version\":3,\"embedding_dim\":192,\
             \"speaker_embedding_b64\":\"AA==\",\"eq\":{{\"gains_db\":[{}]}}}}",
            vec!["0.0"; 32].join(",")
        );
        let hash = format!("{:x}", Sha256::digest(expected.as_bytes()));
        assert_eq!(p.compute_integrity_hash().expect("hash"), hash);
    }

    #[test]
    fn rejects_bad_embeddings_and_other_models() {
        let scaled: Vec<f32> = test_embedding(1).iter().map(|v| v * 2.0).collect();
        assert!(VoiceProfileV2::new("a", "b", "c", &scaled, None).is_err());
        let mut p = profile();
        p.model_id = "pdfnet3-pro-v2".to_owned();
        p.integrity_hash = p.compute_integrity_hash().expect("hash");
        assert!(p.verify().is_err());
    }

    #[test]
    fn stored_profile_classifies_versions_and_rejects_insecure_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("active_profile.json");
        assert_eq!(StoredProfile::load_secure(&path).expect("missing"), None);
        write_secure(&path, &profile().to_json().expect("json")).expect("write");
        assert!(matches!(
            StoredProfile::load_secure(&path),
            Ok(Some(StoredProfile::V2(_)))
        ));
        let v1 = VoiceProfile::identity("old", "Old", "2026-10-01T00:00:00Z").expect("v1");
        write_secure(&path, &v1.to_json().expect("json")).expect("write");
        assert!(matches!(
            StoredProfile::load_secure(&path),
            Ok(Some(StoredProfile::V1(_)))
        ));
        assert!(StoredProfile::from_json("{\"version\":7}").is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            assert!(StoredProfile::load_secure(&path).is_err());
        }
    }
}
