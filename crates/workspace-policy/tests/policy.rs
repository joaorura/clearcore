use workspace_policy::has_nonempty_default_features;

#[test]
fn workspace_policy_rejects_nonempty_default_features() {
    // Given
    let manifest = include_str!("../Cargo.toml");

    // When
    let default_features_enabled = has_nonempty_default_features(manifest);

    // Then
    assert!(!default_features_enabled);
}
