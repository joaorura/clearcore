//! Enforces repository-wide Cargo manifest policy.

/// Reports whether a manifest declares a nonempty `default` feature.
#[must_use]
pub fn has_nonempty_default_features(manifest: &str) -> bool {
    let mut in_features = false;

    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            in_features = line == "[features]";
            continue;
        }

        if !in_features {
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };

        if key.trim() == "default" {
            return value.trim() != "[]";
        }
    }

    false
}
