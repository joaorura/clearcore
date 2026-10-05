//! Owned by task S2; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md

use std::path::PathBuf;

pub const ENV_ASSET: &str = "CLEARCORE_DEV_ENROLLMENT_ASSET";
pub const ENV_SHA256: &str = "CLEARCORE_DEV_ENROLLMENT_SHA256";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnrollmentConfig {
    pub archive_path: Option<PathBuf>,
    pub expected_sha256: Option<String>,
}

impl EnrollmentConfig {
    /// Constructs from a lookup function; trims strings, treats empty as None, lowercases SHA.
    #[must_use]
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let archive_path = get(ENV_ASSET)
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            });

        let expected_sha256 = get(ENV_SHA256)
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(trimmed.to_lowercase())
                }
            });

        Self {
            archive_path,
            expected_sha256,
        }
    }

    /// Constructs from environment variables via `std::env::var`.
    #[allow(dead_code)]
    // removed when Task S6 wires the module
    #[must_use]
    pub fn from_env() -> Self {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Returns true only if both `archive_path` and `expected_sha256` are present
    /// AND the sha256 is exactly 64 lowercase hex characters.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        matches!(
            (&self.archive_path, &self.expected_sha256),
            (Some(_), Some(sha)) if sha.len() == 64 && sha.chars().all(|c| c.is_ascii_hexdigit())
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_set_is_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/path/to/asset.tar.gz".to_string()),
            ENV_SHA256 => Some("ab".repeat(32)),
            _ => None,
        });
        assert!(cfg.is_configured());
    }

    #[test]
    fn missing_asset_is_not_configured() {
        let cfg = EnrollmentConfig::from_lookup(|_| None);
        assert!(!cfg.is_configured());
    }

    #[test]
    fn empty_asset_is_not_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("".to_string()),
            ENV_SHA256 => Some("ab".repeat(32)),
            _ => None,
        });
        assert!(!cfg.is_configured());
    }

    #[test]
    fn short_sha_is_not_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/path".to_string()),
            ENV_SHA256 => Some("ab".repeat(31)),
            _ => None,
        });
        assert!(!cfg.is_configured());
    }

    #[test]
    fn non_hex_sha_is_not_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/path".to_string()),
            ENV_SHA256 => Some("zz".repeat(32)),
            _ => None,
        });
        assert!(!cfg.is_configured());
    }

    #[test]
    fn uppercase_sha_is_normalized_and_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/path".to_string()),
            ENV_SHA256 => Some("AB".repeat(32)),
            _ => None,
        });
        assert!(cfg.is_configured());
        assert_eq!(cfg.expected_sha256, Some("ab".repeat(32)));
    }

    #[test]
    fn default_is_not_configured() {
        let cfg = EnrollmentConfig::default();
        assert!(!cfg.is_configured());
    }

    #[test]
    fn spaces_only_asset_is_none() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("   ".to_string()),
            ENV_SHA256 => Some("ab".repeat(32)),
            _ => None,
        });
        assert!(!cfg.is_configured());
    }

    #[test]
    fn missing_sha_is_not_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/path".to_string()),
            _ => None,
        });
        assert!(!cfg.is_configured());
    }
}
