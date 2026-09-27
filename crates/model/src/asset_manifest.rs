#[cfg(feature = "tract")]
use std::sync::Arc;
use std::{
    fs,
    path::{Path, PathBuf},
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::{InferenceError, archive, json::parse_unique};

const ASSET_ID: &str = "df-compatible-release-asset-v1";
pub const APPROVED_ASSET_SHA256: &str =
    "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616";
const CANDIDATE_DIGEST: &str = "f382facf4d055a8d5387ca0102b3ff521bd9d1f3764b22ebe90c29d92dfb10ba";
const LEGAL_DIGEST: &str = "ea6f77e32a6b5aa80b6562e605f89bad56e860963d39f62361821af076bf9e44";
const KEY_ID: &str = "sha256:cfa7e10c021031f5481775cf32093d41e5aaaad454653f3ed0022769cbbfd3f2";
const SPKI_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

#[derive(Debug, Clone)]
pub struct ApprovedAssetManifest {
    asset_id: String,
    asset_sha256: String,
    candidate_record_sha256: String,
    legal_review_record_sha256: String,
    key_id: String,
    archive_path: PathBuf,
    #[cfg(feature = "tract")]
    archive: Arc<[u8]>,
}

impl ApprovedAssetManifest {
    pub fn verify(repository_root: &Path) -> Result<Self, InferenceError> {
        verify(repository_root).map_err(|error| match error {
            InferenceError::ArchiveValidation(_) => error,
            _ => InferenceError::AssetNotApproved(error.to_string()),
        })
    }

    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }
    pub fn asset_sha256(&self) -> &str {
        &self.asset_sha256
    }
    pub fn candidate_record_sha256(&self) -> &str {
        &self.candidate_record_sha256
    }
    pub fn legal_review_record_sha256(&self) -> &str {
        &self.legal_review_record_sha256
    }
    pub fn key_id(&self) -> &str {
        &self.key_id
    }
    pub fn archive_path(&self) -> &Path {
        &self.archive_path
    }
    #[cfg(feature = "tract")]
    pub fn archive_snapshot(&self) -> Arc<[u8]> {
        Arc::clone(&self.archive)
    }
}

fn verify(root: &Path) -> Result<ApprovedAssetManifest, InferenceError> {
    let governance = root.join("governance/model-assets/df-compatible-release-asset-v1");
    let candidate = read_json(&governance.join("candidate-provenance.json"))?;
    let legal = read_json(&governance.join("legal-review.json"))?;
    let mut approval = read_json(&governance.join("approval-manifest.json"))?;
    let policy = read_json(&root.join("governance/model-assets/trust-policy.json"))?;
    validate_record(&candidate, "asset_id", ASSET_ID)?;
    validate_record(&candidate, "sha256", APPROVED_ASSET_SHA256)?;
    validate_record(&approval, "asset_id", ASSET_ID)?;
    validate_record(&approval, "candidate_sha256", APPROVED_ASSET_SHA256)?;
    validate_record(&approval, "candidate_record_sha256", CANDIDATE_DIGEST)?;
    validate_record(&approval, "legal_review_record_sha256", LEGAL_DIGEST)?;
    validate_record(&approval, "key_id", KEY_ID)?;
    if canonical_digest(&candidate)? != CANDIDATE_DIGEST
        || canonical_digest(&legal)? != LEGAL_DIGEST
    {
        return Err(blocked("reviewed record digest mismatch"));
    }
    let authorized = policy
        .get("authorized_key_ids")
        .and_then(Value::as_array)
        .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(KEY_ID)));
    if !authorized {
        return Err(blocked("approval key is not authorized by fixed policy"));
    }
    let key_bytes = read_regular(&governance.join("approver-public-key.pem"))?;
    let public_key = decode_public_key(&key_bytes)?;
    let computed_key_id = format!("sha256:{:x}", Sha256::digest(&public_key.0));
    if computed_key_id != KEY_ID {
        return Err(blocked("public key identity mismatch"));
    }
    let signature_text = approval
        .get("signature")
        .and_then(Value::as_str)
        .ok_or_else(|| blocked("approval signature is missing"))?
        .to_owned();
    approval
        .as_object_mut()
        .ok_or_else(|| blocked("approval is not an object"))?
        .remove("signature");
    let payload = serde_json::to_vec(&approval).map_err(|error| json_error(&error))?;
    let signature_bytes = STANDARD
        .decode(signature_text)
        .map_err(|error| blocked(&error.to_string()))?;
    let signature =
        Signature::from_slice(&signature_bytes).map_err(|error| blocked(&error.to_string()))?;
    key_bytes_to_verifier(public_key.1)?
        .verify(&payload, &signature)
        .map_err(|error| blocked(&error.to_string()))?;
    let archive_path = root.join("vendor/approved/df-compatible-release-asset-v1.bin");
    if archive_path.file_name().and_then(|name| name.to_str())
        != Some("df-compatible-release-asset-v1.bin")
    {
        return Err(blocked("asset filename mismatch"));
    }
    let archive_bytes = read_regular(&archive_path)?;
    if archive_bytes.len() != 7_983_136
        || format!("{:x}", Sha256::digest(&archive_bytes)) != APPROVED_ASSET_SHA256
    {
        return Err(blocked("asset snapshot digest mismatch"));
    }
    archive::validate(&archive_bytes)?;
    Ok(ApprovedAssetManifest {
        asset_id: ASSET_ID.to_owned(),
        asset_sha256: APPROVED_ASSET_SHA256.to_owned(),
        candidate_record_sha256: CANDIDATE_DIGEST.to_owned(),
        legal_review_record_sha256: LEGAL_DIGEST.to_owned(),
        key_id: KEY_ID.to_owned(),
        archive_path,
        #[cfg(feature = "tract")]
        archive: archive_bytes.into(),
    })
}

fn read_json(path: &Path) -> Result<Value, InferenceError> {
    parse_unique(&read_regular(path)?).map_err(|error| json_error(&error))
}
fn canonical_digest(value: &Value) -> Result<String, InferenceError> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|error| json_error(&error))?)
    ))
}
fn validate_record(value: &Value, key: &str, expected: &str) -> Result<(), InferenceError> {
    if value.get(key).and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(blocked(&format!("{key} mismatch")))
    }
}

fn read_regular(path: &Path) -> Result<Vec<u8>, InferenceError> {
    let before = fs::symlink_metadata(path).map_err(InferenceError::Io)?;
    if !before.file_type().is_file() {
        return Err(blocked(
            "verification input is not a regular non-symlink file",
        ));
    }
    let bytes = fs::read(path).map_err(InferenceError::Io)?;
    let after = fs::symlink_metadata(path).map_err(InferenceError::Io)?;
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err(blocked("verification input changed during snapshot"));
    }
    Ok(bytes)
}

fn decode_public_key(pem: &[u8]) -> Result<(Vec<u8>, [u8; 32]), InferenceError> {
    let text = std::str::from_utf8(pem).map_err(|error| blocked(&error.to_string()))?;
    let body: String = text
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    let der = STANDARD
        .decode(body)
        .map_err(|error| blocked(&error.to_string()))?;
    if der.len() != 44 || !der.starts_with(SPKI_PREFIX) {
        return Err(blocked("invalid Ed25519 SubjectPublicKeyInfo"));
    }
    let raw: [u8; 32] = der[12..]
        .try_into()
        .map_err(|_| blocked("invalid Ed25519 key length"))?;
    Ok((der, raw))
}

fn key_bytes_to_verifier(raw: [u8; 32]) -> Result<VerifyingKey, InferenceError> {
    VerifyingKey::from_bytes(&raw).map_err(|error| blocked(&error.to_string()))
}
fn blocked(message: &str) -> InferenceError {
    InferenceError::AssetNotApproved(message.to_owned())
}
fn json_error(error: &serde_json::Error) -> InferenceError {
    blocked(&error.to_string())
}
