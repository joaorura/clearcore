use std::path::PathBuf;

use workspace_policy::{PolicyViolation, check_workspace, has_nonempty_default_features};

#[test]
fn workspace_policy_rejects_nonempty_default_features() {
    // Given
    let manifest = include_str!("../Cargo.toml");

    // When
    let default_features_enabled = has_nonempty_default_features(manifest);

    // Then
    assert!(!default_features_enabled);
}

#[test]
fn workspace_policy_rejects_sibling_dependencies_with_implicit_default_features() {
    // Given
    let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/noncompliant-workspace");

    // When
    let violations = check_workspace(&fixture_root).expect("fixture manifests should be readable");

    // Then
    assert_eq!(
        violations,
        [
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "sibling".to_owned(),
                dependency: "inline".to_owned(),
            },
            PolicyViolation::DependencyUsesDefaultFeatures {
                member: "sibling".to_owned(),
                dependency: "shorthand".to_owned(),
            },
        ]
    );
}

#[test]
fn workspace_policy_accepts_all_registered_repository_manifests() {
    // Given
    let workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|crates| crates.parent())
        .expect("workspace-policy should be nested under the workspace root");

    // When
    let violations = check_workspace(workspace_root).expect("workspace manifests should be readable");

    // Then
    assert!(violations.is_empty(), "policy violations: {violations:?}");
}
