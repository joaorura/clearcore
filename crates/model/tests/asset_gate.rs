use std::path::PathBuf;

use realtime_noise_model::{APPROVED_ASSET_SHA256, ApprovedAssetManifest};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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
