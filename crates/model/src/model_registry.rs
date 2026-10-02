use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::{InferenceError, json::parse_unique, m0_records};

pub const DEV_KEY_ID: &str = "test-author-key";
const SPKI_PREFIX: &[u8] = &[
    0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ModelRole {
    DenoisingBase,
    DenoisingPersonalized,
    SpeakerEnrollment,
    NeuralEq,
}

impl ModelRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DenoisingBase => "denoising-base",
            Self::DenoisingPersonalized => "denoising-personalized",
            Self::SpeakerEnrollment => "speaker-enrollment",
            Self::NeuralEq => "neural-eq",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetDescriptor {
    pub asset_id: String,
    pub role: ModelRole,
    pub sha256: String,
    pub size_bytes: u64,
    pub allowed_members: &'static [&'static str],
}

/// An asset whose signature, digest and archive members the registry has checked.
///
/// The fields are private on purpose: the only way to obtain a `VerifiedAsset` outside this crate
/// is through `ModelAssetRegistry::verify*`, so holding one is proof of verification. Unit tests
/// inside this crate may build one with `for_test` (compiled only under `cfg(test)`).
#[derive(Debug, Clone)]
pub struct VerifiedAsset {
    descriptor: AssetDescriptor,
    candidate_record_sha256: String,
    legal_review_record_sha256: String,
    key_id: String,
    repository_root: PathBuf,
    archive_path: PathBuf,
    archive_bytes: Arc<[u8]>,
}

impl VerifiedAsset {
    #[must_use]
    pub const fn descriptor(&self) -> &AssetDescriptor {
        &self.descriptor
    }

    #[must_use]
    pub fn asset_id(&self) -> &str {
        &self.descriptor.asset_id
    }

    #[must_use]
    pub const fn role(&self) -> ModelRole {
        self.descriptor.role
    }

    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.descriptor.sha256
    }

    #[must_use]
    pub fn archive_path(&self) -> &Path {
        &self.archive_path
    }

    #[must_use]
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    #[must_use]
    pub fn archive_snapshot(&self) -> Arc<[u8]> {
        Arc::clone(&self.archive_bytes)
    }

    #[must_use]
    pub fn candidate_record_sha256(&self) -> &str {
        &self.candidate_record_sha256
    }

    #[must_use]
    pub fn legal_review_record_sha256(&self) -> &str {
        &self.legal_review_record_sha256
    }

    #[must_use]
    pub fn repository_root(&self) -> &Path {
        &self.repository_root
    }

    /// Builds an unverified asset for unit tests. Not compiled outside `cfg(test)`, so production
    /// code and downstream crates cannot fabricate one.
    #[cfg(test)]
    pub(crate) fn for_test(descriptor: AssetDescriptor, archive_bytes: Vec<u8>) -> Self {
        Self {
            descriptor,
            candidate_record_sha256: String::new(),
            legal_review_record_sha256: String::new(),
            key_id: "test".to_owned(),
            repository_root: PathBuf::new(),
            archive_path: PathBuf::new(),
            archive_bytes: archive_bytes.into(),
        }
    }

    /// Test-only: pretend the asset carries another role (to prove role gating).
    #[cfg(test)]
    pub(crate) const fn with_role_for_test(mut self, role: ModelRole) -> Self {
        self.descriptor.role = role;
        self
    }
}

/// Catalog of approved model assets.
///
/// The only way to obtain a [`VerifiedAsset`] is [`Self::verify`] / [`Self::verify_role`], which
/// take an asset id, never a caller-supplied descriptor, and the catalog cannot be edited from
/// outside the crate. So the role of a verified asset is always the one the catalog registered.
///
/// ```compile_fail,E0624
/// use realtime_noise_model::{ModelAssetRegistry, ModelRole};
///
/// let mut registry = ModelAssetRegistry::new();
/// let mut forged = registry.get("df-compatible-release-asset-v1").unwrap().clone();
/// forged.role = ModelRole::SpeakerEnrollment;
/// // Re-registering an existing id with another role is not available outside the crate.
/// registry.register(forged);
/// ```
///
/// ```compile_fail,E0624
/// use realtime_noise_model::{ModelAssetRegistry, ModelRole};
/// use std::path::Path;
///
/// let registry = ModelAssetRegistry::new();
/// let mut forged = registry.get("df-compatible-release-asset-v1").unwrap().clone();
/// forged.role = ModelRole::SpeakerEnrollment;
/// // Verifying a hand-built descriptor is not available outside the crate.
/// let _ = registry.verify_descriptor(Path::new("."), &forged);
/// ```
#[derive(Debug, Clone)]
pub struct ModelAssetRegistry {
    descriptors: HashMap<String, AssetDescriptor>,
    dev_keys: HashSet<String>,
    allow_dev_keys: bool,
}

impl Default for ModelAssetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ModelAssetRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            descriptors: HashMap::new(),
            dev_keys: HashSet::new(),
            allow_dev_keys: false,
        };

        registry.register(AssetDescriptor {
            asset_id: "df-compatible-release-asset-v1".to_owned(),
            role: ModelRole::DenoisingBase,
            sha256: "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616".to_owned(),
            size_bytes: 7_983_136,
            allowed_members: &[
                "tmp/export/enc.onnx",
                "tmp/export/erb_dec.onnx",
                "tmp/export/df_dec.onnx",
                "tmp/export/config.ini",
            ],
        });

        registry.register(AssetDescriptor {
            asset_id: "pdfnet3-release-asset-v1".to_owned(),
            role: ModelRole::DenoisingPersonalized,
            sha256: String::new(),
            size_bytes: 0,
            allowed_members: &["enc.onnx", "erb_dec.onnx", "df_dec.onnx", "config.ini"],
        });

        registry.register(AssetDescriptor {
            asset_id: "voice-enrollment-asset-v1".to_owned(),
            role: ModelRole::SpeakerEnrollment,
            sha256: String::new(),
            size_bytes: 0,
            allowed_members: &["enrollment.onnx"],
        });

        registry.register(AssetDescriptor {
            asset_id: "neural-eq-asset-v1".to_owned(),
            role: ModelRole::NeuralEq,
            sha256: String::new(),
            size_bytes: 0,
            allowed_members: &["neural_eq.onnx"],
        });

        registry
    }

    pub(crate) fn register(&mut self, descriptor: AssetDescriptor) {
        self.descriptors
            .insert(descriptor.asset_id.clone(), descriptor);
    }

    pub fn get(&self, asset_id: &str) -> Option<&AssetDescriptor> {
        self.descriptors.get(asset_id)
    }

    pub fn find_by_role(&self, role: ModelRole) -> Option<&AssetDescriptor> {
        self.descriptors.values().find(|d| d.role == role)
    }

    /// Test-only: a production registry must never be able to enable the development key.
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn with_allow_dev_keys(mut self, allow: bool) -> Self {
        self.allow_dev_keys = allow;
        self
    }

    #[must_use]
    pub const fn allow_dev_keys(&self) -> bool {
        self.allow_dev_keys
    }

    /// Test-only: see [`Self::with_allow_dev_keys`].
    #[cfg(test)]
    pub(crate) fn add_dev_key(&mut self, key_id: impl Into<String>) {
        self.dev_keys.insert(key_id.into());
    }

    pub fn is_key_authorized(&self, key_id: &str, policy: &Value) -> bool {
        if self.allow_dev_keys && (key_id == DEV_KEY_ID || self.dev_keys.contains(key_id)) {
            return true;
        }

        policy
            .as_object()
            .and_then(|obj| obj.get("authorized_key_ids"))
            .and_then(Value::as_array)
            .is_some_and(|ids| {
                ids.iter().all(Value::is_string) && ids.iter().any(|id| id.as_str() == Some(key_id))
            })
    }

    pub fn verify(
        &self,
        repository_root: &Path,
        asset_id: &str,
    ) -> Result<VerifiedAsset, InferenceError> {
        let descriptor = self.get(asset_id).ok_or_else(|| {
            InferenceError::AssetNotApproved(format!("asset descriptor not found: {asset_id}"))
        })?;
        self.verify_descriptor(repository_root, descriptor)
    }

    /// Like [`Self::verify`], but also requires the descriptor to have the expected role, so an
    /// enrollment or EQ asset can never be loaded where a denoising model is expected.
    pub fn verify_role(
        &self,
        repository_root: &Path,
        asset_id: &str,
        expected: ModelRole,
    ) -> Result<VerifiedAsset, InferenceError> {
        let descriptor = self.get(asset_id).ok_or_else(|| {
            InferenceError::AssetNotApproved(format!("asset descriptor not found: {asset_id}"))
        })?;
        if descriptor.role != expected {
            return Err(blocked(&format!(
                "asset {asset_id} has role {}, expected {}",
                descriptor.role.as_str(),
                expected.as_str()
            )));
        }
        self.verify_descriptor(repository_root, descriptor)
    }

    fn verify_descriptor(
        &self,
        repository_root: &Path,
        descriptor: &AssetDescriptor,
    ) -> Result<VerifiedAsset, InferenceError> {
        // Fail closed: a descriptor without a pinned digest and size would let the signed records
        // be checked against nothing, so any archive would pass.
        if descriptor.sha256.len() != 64 || descriptor.size_bytes == 0 {
            return Err(blocked(&format!(
                "asset {} is not pinned: the registry has no SHA-256 and size for it yet",
                descriptor.asset_id
            )));
        }
        let governance = repository_root
            .join("governance/model-assets")
            .join(&descriptor.asset_id);
        let candidate = read_json(&governance.join("candidate-provenance.json"))?;
        let legal = read_json(&governance.join("legal-review.json"))?;
        let mut approval = read_json(&governance.join("approval-manifest.json"))?;
        let policy = read_json(&repository_root.join("governance/model-assets/trust-policy.json"))?;

        m0_records::validate(&candidate, &legal, &approval).map_err(|error| blocked(&error))?;

        validate_record(&candidate, "asset_id", &descriptor.asset_id)?;
        validate_record(&candidate, "sha256", &descriptor.sha256)?;

        validate_record(&approval, "asset_id", &descriptor.asset_id)?;
        validate_record(&approval, "candidate_sha256", &descriptor.sha256)?;

        let cand_digest = canonical_digest(&candidate)?;
        let legal_digest = canonical_digest(&legal)?;
        validate_record(&approval, "candidate_record_sha256", &cand_digest)?;
        validate_record(&approval, "legal_review_record_sha256", &legal_digest)?;

        let policy_object = policy
            .as_object()
            .ok_or_else(|| blocked("trust policy must be an object"))?;
        if policy_object.len() != 1 || !policy_object.contains_key("authorized_key_ids") {
            return Err(blocked("trust policy fields mismatch"));
        }

        let key_bytes = read_regular(&governance.join("approver-public-key.pem"))?;
        let public_key = decode_public_key(&key_bytes)?;
        let computed_key_id = format!("sha256:{:x}", Sha256::digest(&public_key.0));

        validate_record(&approval, "key_id", &computed_key_id)?;

        if !self.is_key_authorized(&computed_key_id, &policy) {
            return Err(blocked("approval key is not authorized by policy"));
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

        let archive_filename = format!("{}.bin", descriptor.asset_id);
        let archive_path = repository_root
            .join("vendor/approved")
            .join(&archive_filename);
        let archive_bytes = read_regular(&archive_path)?;

        if archive_bytes.len() as u64 != descriptor.size_bytes {
            return Err(blocked("asset snapshot size mismatch"));
        }

        let computed_sha256 = format!("{:x}", Sha256::digest(&archive_bytes));
        if computed_sha256 != descriptor.sha256 {
            return Err(blocked("asset snapshot digest mismatch"));
        }

        validate_archive_members(&archive_bytes, descriptor.allowed_members)?;

        Ok(VerifiedAsset {
            descriptor: descriptor.clone(),
            candidate_record_sha256: cand_digest,
            legal_review_record_sha256: legal_digest,
            key_id: computed_key_id,
            repository_root: repository_root.to_path_buf(),
            archive_path,
            archive_bytes: archive_bytes.into(),
        })
    }
}

/// Layout prefix of the upstream release archives (`tmp/export/<member>`); members are also
/// accepted flat. Both spellings name the same member.
const EXPORT_PREFIX: &str = "tmp/export/";

/// Checks that `bytes` is a tar.gz with exactly the `allowed_members` (each once, regular files,
/// no executables). An empty allowlist accepts nothing: the allowlist is the contract.
pub fn validate_archive_members(
    bytes: &[u8],
    allowed_members: &[&str],
) -> Result<(), InferenceError> {
    let decoder = GzDecoder::new(bytes);
    let mut archive = Archive::new(decoder);
    let entries = archive.entries().map_err(archive_error)?;
    let mut seen = BTreeSet::new();
    let allowed: BTreeSet<&str> = allowed_members
        .iter()
        .map(|member| member.strip_prefix(EXPORT_PREFIX).unwrap_or(member))
        .collect();

    for entry in entries {
        let mut entry = entry.map_err(archive_error)?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid_archive("archive contains a non-regular member"));
        }
        let mode = entry.header().mode().map_err(archive_error)?;
        if mode & 0o111 != 0 {
            return Err(invalid_archive("archive contains an executable member"));
        }
        let path = entry.path().map_err(archive_error)?;
        let path = path
            .to_str()
            .ok_or_else(|| invalid_archive("member path is not UTF-8"))?;
        let name = path.strip_prefix(EXPORT_PREFIX).unwrap_or(path);

        if !allowed.contains(name) {
            return Err(invalid_archive("archive contains an unexpected member"));
        }
        if !seen.insert(name.to_owned()) {
            return Err(invalid_archive("archive contains a duplicate member"));
        }

        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).map_err(archive_error)?;
    }

    if seen.len() != allowed.len() {
        return Err(invalid_archive("archive is missing required members"));
    }

    Ok(())
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

fn archive_error(error: impl std::fmt::Display) -> InferenceError {
    invalid_archive(&error.to_string())
}

fn invalid_archive(message: &str) -> InferenceError {
    InferenceError::ArchiveValidation(message.to_owned())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::redundant_clone)]
mod tests {
    use super::*;

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf()
    }

    #[test]
    fn test_catalog_descriptors_registered() {
        let registry = ModelAssetRegistry::new();
        assert!(registry.get("df-compatible-release-asset-v1").is_some());
        assert!(registry.get("pdfnet3-release-asset-v1").is_some());
        assert!(registry.get("voice-enrollment-asset-v1").is_some());
        assert!(registry.get("neural-eq-asset-v1").is_some());

        assert_eq!(
            registry
                .find_by_role(ModelRole::DenoisingBase)
                .map(|d| d.asset_id.as_str()),
            Some("df-compatible-release-asset-v1")
        );
        assert_eq!(
            registry
                .find_by_role(ModelRole::DenoisingPersonalized)
                .map(|d| d.asset_id.as_str()),
            Some("pdfnet3-release-asset-v1")
        );
        assert_eq!(
            registry
                .find_by_role(ModelRole::SpeakerEnrollment)
                .map(|d| d.asset_id.as_str()),
            Some("voice-enrollment-asset-v1")
        );
        assert_eq!(
            registry
                .find_by_role(ModelRole::NeuralEq)
                .map(|d| d.asset_id.as_str()),
            Some("neural-eq-asset-v1")
        );
    }

    #[test]
    fn test_verify_approved_base_asset() {
        let root = repo_root();
        let registry = ModelAssetRegistry::default();
        let verified = registry
            .verify(&root, "df-compatible-release-asset-v1")
            .expect("standard approved asset must verify");
        assert_eq!(verified.role(), ModelRole::DenoisingBase);
        assert_eq!(verified.asset_id(), "df-compatible-release-asset-v1");
        assert_eq!(
            verified.sha256(),
            "c94d91f70911001c946e0fabb4aa9adc37045f45a03b56008cb0c8244cb63616"
        );
        assert_eq!(verified.archive_bytes.len(), 7_983_136);
    }

    #[test]
    fn test_rejects_tampered_hash() {
        let root = repo_root();
        let mut registry = ModelAssetRegistry::default();
        let mut bad_desc = registry
            .get("df-compatible-release-asset-v1")
            .unwrap()
            .clone();
        bad_desc.sha256 = "0".repeat(64);
        registry.register(bad_desc);

        let err = registry.verify(&root, "df-compatible-release-asset-v1");
        assert!(err.is_err(), "must reject tampered sha256");
    }

    #[test]
    fn test_rejects_unexpected_member() {
        let root = repo_root();
        let mut registry = ModelAssetRegistry::default();
        let mut bad_desc = registry
            .get("df-compatible-release-asset-v1")
            .unwrap()
            .clone();
        bad_desc.allowed_members = &["only_one_member.onnx"];
        registry.register(bad_desc);

        let err = registry.verify(&root, "df-compatible-release-asset-v1");
        assert!(err.is_err(), "must reject archive with unexpected members");
    }

    fn build_archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        use flate2::{Compression, write::GzEncoder};
        use tar::{Builder, Header};
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
        for (path, bytes) in members {
            let mut header = Header::new_gnu();
            // Raw name bytes: `set_path` refuses `..`, and the traversal case must be buildable.
            header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, *bytes).expect("append");
        }
        builder.into_inner().expect("tar").finish().expect("gzip")
    }

    #[test]
    fn unpinned_descriptors_cannot_be_verified() {
        // The pDFNet3, enrollment and neural EQ descriptors have no digest/size yet (the weights
        // come from the training repository). Verifying them would bind the archive to nothing,
        // so it must fail closed before any I/O.
        let registry = ModelAssetRegistry::default();
        for asset_id in [
            "pdfnet3-release-asset-v1",
            "voice-enrollment-asset-v1",
            "neural-eq-asset-v1",
        ] {
            let error = registry.verify(&repo_root(), asset_id).expect_err(asset_id);
            assert!(
                matches!(&error, InferenceError::AssetNotApproved(message) if message.contains("not pinned")),
                "{asset_id}: {error:?}"
            );
        }
    }

    #[test]
    fn registering_a_pinned_descriptor_makes_it_verifiable_again() {
        let mut registry = ModelAssetRegistry::default();
        let mut descriptor = registry.get("pdfnet3-release-asset-v1").unwrap().clone();
        descriptor.sha256 = "ab".repeat(32);
        descriptor.size_bytes = 1;
        registry.register(descriptor);
        // Now it gets past the pin check and fails later, on the missing governance records.
        let error = registry
            .verify(&repo_root(), "pdfnet3-release-asset-v1")
            .expect_err("no records");
        assert!(
            !matches!(&error, InferenceError::AssetNotApproved(message) if message.contains("not pinned"))
        );
    }

    #[test]
    fn role_mismatch_is_rejected() {
        let registry = ModelAssetRegistry::default();
        let root = repo_root();
        let verified = registry
            .verify_role(
                &root,
                "df-compatible-release-asset-v1",
                ModelRole::DenoisingBase,
            )
            .expect("right role verifies");
        assert_eq!(verified.role(), ModelRole::DenoisingBase);
        for wrong in [
            ModelRole::DenoisingPersonalized,
            ModelRole::SpeakerEnrollment,
            ModelRole::NeuralEq,
        ] {
            let error = registry
                .verify_role(&root, "df-compatible-release-asset-v1", wrong)
                .expect_err("wrong role");
            assert!(
                matches!(&error, InferenceError::AssetNotApproved(message) if message.contains("role")),
                "{wrong:?}: {error:?}"
            );
        }
    }

    #[test]
    fn empty_allowlist_rejects_every_member() {
        let archive = build_archive(&[("enc.onnx", b"x")]);
        assert!(validate_archive_members(&archive, &[]).is_err());
    }

    #[test]
    fn member_spelled_two_ways_counts_as_duplicate() {
        let archive = build_archive(&[("enc.onnx", b"a"), ("tmp/export/enc.onnx", b"b")]);
        let error = validate_archive_members(&archive, &["enc.onnx"]).expect_err("duplicate");
        assert!(
            matches!(&error, InferenceError::ArchiveValidation(message) if message.contains("duplicate"))
        );
    }

    #[test]
    fn missing_member_is_rejected() {
        let archive = build_archive(&[("enc.onnx", b"a")]);
        let error =
            validate_archive_members(&archive, &["enc.onnx", "df_dec.onnx"]).expect_err("missing");
        assert!(
            matches!(&error, InferenceError::ArchiveValidation(message) if message.contains("missing"))
        );
    }

    #[test]
    fn exact_members_in_either_layout_are_accepted_and_traversal_is_not() {
        let allowed = ["enc.onnx", "config.ini"];
        let flat = build_archive(&[("enc.onnx", b"a"), ("config.ini", b"b")]);
        assert!(validate_archive_members(&flat, &allowed).is_ok());
        let nested = build_archive(&[
            ("tmp/export/enc.onnx", b"a"),
            ("tmp/export/config.ini", b"b"),
        ]);
        assert!(validate_archive_members(&nested, &allowed).is_ok());
        let traversal = build_archive(&[("../enc.onnx", b"a"), ("config.ini", b"b")]);
        assert!(validate_archive_members(&traversal, &allowed).is_err());
    }

    #[test]
    fn test_dev_key_policy_enforcement() {
        let registry = ModelAssetRegistry::new();
        assert!(!registry.allow_dev_keys());

        let policy = serde_json::json!({
            "authorized_key_ids": [
                "sha256:production_official_key"
            ]
        });

        // In production mode, dev key is rejected
        assert!(!registry.is_key_authorized(DEV_KEY_ID, &policy));
        assert!(!registry.is_key_authorized("sha256:custom_dev_key", &policy));
        assert!(registry.is_key_authorized("sha256:production_official_key", &policy));

        // When dev keys allowed, dev key is authorized
        let mut dev_registry = registry.clone().with_allow_dev_keys(true);
        assert!(dev_registry.is_key_authorized(DEV_KEY_ID, &policy));
        dev_registry.add_dev_key("sha256:custom_dev_key");
        assert!(dev_registry.is_key_authorized("sha256:custom_dev_key", &policy));
        // Production key still authorized
        assert!(dev_registry.is_key_authorized("sha256:production_official_key", &policy));
        // Unknown key still rejected
        assert!(!dev_registry.is_key_authorized("sha256:unknown_key", &policy));
    }
}
