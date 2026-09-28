use std::{fs, path::PathBuf};

use tempfile::TempDir;

pub fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn isolated_repository() -> Result<TempDir, Box<dyn std::error::Error>> {
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
