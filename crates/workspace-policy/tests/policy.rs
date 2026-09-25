use std::{error::Error, path::PathBuf};

use workspace_policy::{PolicyViolation, check_workspace, has_nonempty_default_features};

type TestResult = Result<(), Box<dyn Error>>;

fn fixture_root(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn workspace_policy_rejects_nonempty_default_features() {
    let manifest = "[features]\n\"default\" = ['enabled']";

    assert!(has_nonempty_default_features(manifest));
}

#[test]
fn workspace_policy_rejects_effective_defaults_across_supported_forms() -> TestResult {
    let violations = check_workspace(&fixture_root("noncompliant-workspace"))?;

    assert_eq!(
        violations,
        [
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "<workspace>".to_owned(),
                dependency: "inherited-bad".to_owned(),
            },
            PolicyViolation::NonemptyDefaultFeatures {
                member: "sibling".to_owned(),
            },
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "sibling".to_owned(),
                dependency: "inherited-bad".to_owned(),
            },
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "sibling".to_owned(),
                dependency: "quoted#inline".to_owned(),
            },
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "sibling".to_owned(),
                dependency: "dev-bad".to_owned(),
            },
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "sibling".to_owned(),
                dependency: "build-bad".to_owned(),
            },
        ]
    );
    Ok(())
}

#[test]
fn workspace_policy_accepts_all_supported_fixture_forms() -> TestResult {
    let violations = check_workspace(&fixture_root("compliant-workspace"))?;

    assert!(violations.is_empty(), "policy violations: {violations:?}");
    Ok(())
}

#[test]
fn workspace_policy_rejects_unsupported_dependency_tables() {
    let result = check_workspace(&fixture_root("unsupported-workspace"));

    assert!(result.is_err());
}

#[test]
fn malformed_dependency_declarations_fail_closed() {
    let manifest = r#"
        [dependencies]
        bad = { version = "1", default-features = "false" }
    "#;

    assert!(has_nonempty_default_features(manifest));
}

#[test]
fn workspace_policy_accepts_all_registered_repository_manifests() -> TestResult {
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let violations = check_workspace(&workspace_root)?;

    assert!(violations.is_empty(), "policy violations: {violations:?}");
    Ok(())
}
