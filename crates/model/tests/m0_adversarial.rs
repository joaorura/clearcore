#![cfg(feature = "tract")]

mod support;

use std::fs;

use realtime_noise_model::{ApprovedAssetManifest, CpuProfile, InferenceError, TractBackend};
use serde_json::Value;
use support::isolated_repository;

const GOVERNANCE: &str = "governance/model-assets/df-compatible-release-asset-v1";

fn mutate_json(
    root: &std::path::Path,
    name: &str,
    mutation: impl FnOnce(&mut Value),
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join(GOVERNANCE).join(name);
    let mut value: Value = serde_json::from_slice(&fs::read(&path)?)?;
    mutation(&mut value);
    fs::write(path, serde_json::to_vec(&value)?)?;
    Ok(())
}

fn rejects(root: &std::path::Path) -> Result<String, Box<dyn std::error::Error>> {
    match ApprovedAssetManifest::verify(root) {
        Err(InferenceError::AssetNotApproved(message)) => Ok(message),
        Err(error) => Err(format!("wrong error variant: {error}").into()),
        Ok(_) => Err("M0 mutation was accepted".into()),
    }
}

#[test]
fn rejects_candidate_and_approval_hash_tampering() -> Result<(), Box<dyn std::error::Error>> {
    let candidate = isolated_repository()?;
    mutate_json(candidate.path(), "candidate-provenance.json", |value| {
        let new_sha256 = "0".repeat(64);
        value["sha256"] = Value::String(new_sha256.clone());
        value["source"] = Value::String(format!("urn:sha256:{}", new_sha256));
    })?;
    assert!(rejects(candidate.path())?.contains("candidate_sha256 does not match sha256"));

    let approval = isolated_repository()?;
    mutate_json(approval.path(), "approval-manifest.json", |value| {
        value["candidate_record_sha256"] = Value::String("0".repeat(64));
    })?;
    assert!(rejects(approval.path())?.contains("candidate_record_sha256 mismatch"));
    Ok(())
}

#[test]
fn rejects_cross_record_legal_license_and_conversion_mismatches()
-> Result<(), Box<dyn std::error::Error>> {
    for (field, expected) in [
        ("legal_approval_id", "legal_approval_id does not match"),
        ("weight_license", "weight_license does not match"),
        ("code_license", "code_license does not match"),
        ("conversion_terms", "conversion_terms does not match"),
    ] {
        let repository = isolated_repository()?;
        mutate_json(repository.path(), "legal-review.json", |value| {
            value[field] = Value::String("tampered".to_owned());
        })?;
        assert!(rejects(repository.path())?.contains(expected));
    }
    Ok(())
}

#[test]
fn rejects_signature_tamper_and_untrusted_key() -> Result<(), Box<dyn std::error::Error>> {
    let signature = isolated_repository()?;
    mutate_json(signature.path(), "approval-manifest.json", |value| {
        value["signature"] = Value::String("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==".to_owned());
    })?;
    assert!(!rejects(signature.path())?.is_empty());

    let policy = isolated_repository()?;
    fs::write(
        policy
            .path()
            .join("governance/model-assets/trust-policy.json"),
        br#"{"authorized_key_ids":["sha256:untrusted"]}"#,
    )?;
    assert!(rejects(policy.path())?.contains("not authorized"));
    Ok(())
}

#[test]
fn rejects_malformed_duplicate_and_non_finite_json() -> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        br#"{"status": "APPROVED""#.as_slice(),
        br#"{"status":"APPROVED","status":"APPROVED"}"#.as_slice(),
        br#"{"value":NaN}"#.as_slice(),
    ] {
        let repository = isolated_repository()?;
        fs::write(
            repository
                .path()
                .join(GOVERNANCE)
                .join("candidate-provenance.json"),
            bytes,
        )?;
        assert!(!rejects(repository.path())?.is_empty());
    }
    Ok(())
}

#[test]
fn rejects_trust_policy_wrong_types_and_extra_keys() -> Result<(), Box<dyn std::error::Error>> {
    for bytes in [
        br#"{"authorized_key_ids":"not-an-array"}"#.as_slice(),
        br#"{"authorized_key_ids":[1]}"#.as_slice(),
        br#"{"authorized_key_ids":[],"extra":true}"#.as_slice(),
    ] {
        let repository = isolated_repository()?;
        fs::write(
            repository
                .path()
                .join("governance/model-assets/trust-policy.json"),
            bytes,
        )?;
        assert!(!rejects(repository.path())?.is_empty());
    }
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_symlink_and_non_regular_inputs() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::fs::symlink;

    let symlinked = isolated_repository()?;
    let path = symlinked
        .path()
        .join(GOVERNANCE)
        .join("candidate-provenance.json");
    let target = symlinked.path().join("candidate-copy.json");
    fs::rename(&path, &target)?;
    symlink(&target, &path)?;
    assert!(rejects(symlinked.path())?.contains("not a regular non-symlink"));

    let directory = isolated_repository()?;
    let path = directory
        .path()
        .join(GOVERNANCE)
        .join("candidate-provenance.json");
    fs::remove_file(&path)?;
    fs::create_dir(&path)?;
    assert!(rejects(directory.path())?.contains("not a regular non-symlink"));
    Ok(())
}

#[test]
fn backend_rejects_archive_replacement_after_verification() -> Result<(), Box<dyn std::error::Error>>
{
    let repository = isolated_repository()?;
    let manifest = ApprovedAssetManifest::verify(repository.path())?;
    fs::write(
        repository
            .path()
            .join("vendor/approved/df-compatible-release-asset-v1.bin"),
        b"replacement",
    )?;

    assert!(matches!(
        TractBackend::new(&manifest, CpuProfile::Avx2Minimum),
        Err(InferenceError::AssetNotApproved(_))
    ));
    Ok(())
}
