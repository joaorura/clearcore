#![cfg(feature = "tract")]

use std::{fs, path::PathBuf};

use realtime_noise_model::{
    APPROVED_ASSET_SHA256, ApprovedAssetManifest, CpuProfile, InferenceError, TractBackend,
};
use tempfile::TempDir;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn isolated_repository() -> Result<TempDir, Box<dyn std::error::Error>> {
    let temporary = TempDir::new()?;
    let source = repository_root();
    let governance = "governance/model-assets/df-compatible-release-asset-v1";
    fs::create_dir_all(temporary.path().join(governance))?;
    fs::create_dir_all(temporary.path().join("vendor/approved"))?;
    for relative in [
        "governance/model-assets/trust-policy.json",
        "governance/model-assets/df-compatible-release-asset-v1/candidate-provenance.json",
        "governance/model-assets/df-compatible-release-asset-v1/legal-review.json",
        "governance/model-assets/df-compatible-release-asset-v1/approval-manifest.json",
        "governance/model-assets/df-compatible-release-asset-v1/approver-public-key.pem",
        "vendor/approved/df-compatible-release-asset-v1.bin",
    ] {
        fs::copy(source.join(relative), temporary.path().join(relative))?;
    }
    Ok(temporary)
}

#[test]
fn approved_manifest_is_bound_to_fresh_m0_verification() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = ApprovedAssetManifest::verify(&repository_root())?;

    assert_eq!(manifest.asset_sha256(), APPROVED_ASSET_SHA256);
    assert_eq!(manifest.asset_id(), "df-compatible-release-asset-v1");
    Ok(())
}

#[test]
fn tract_backend_refuses_unapproved_asset() {
    let missing_root = repository_root().join("does-not-exist");
    assert!(ApprovedAssetManifest::verify(&missing_root).is_err());
}

#[test]
fn backend_revalidates_manifest_before_parsing_model() -> Result<(), Box<dyn std::error::Error>> {
    let repository = isolated_repository()?;
    let manifest = ApprovedAssetManifest::verify(repository.path())?;
    fs::write(
        repository
            .path()
            .join("governance/model-assets/trust-policy.json"),
        br#"{"authorized_key_ids":[],"unexpected":true}"#,
    )?;

    let error = TractBackend::new(&manifest, CpuProfile::Avx2Minimum)
        .err()
        .ok_or("stale approval unexpectedly reached model construction")?;
    assert!(matches!(error, InferenceError::AssetNotApproved(_)));
    Ok(())
}

#[test]
fn trust_policy_rejects_unexpected_fields() -> Result<(), Box<dyn std::error::Error>> {
    let repository = isolated_repository()?;
    let policy_path = repository
        .path()
        .join("governance/model-assets/trust-policy.json");
    let original = fs::read_to_string(&policy_path)?;
    let modified = original.replacen('{', "{\"unexpected\":true,", 1);
    fs::write(policy_path, modified)?;

    assert!(matches!(
        ApprovedAssetManifest::verify(repository.path()),
        Err(InferenceError::AssetNotApproved(_))
    ));
    Ok(())
}
