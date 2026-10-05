//! Owned by task S2; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md

use std::ffi::OsString;
use std::path::PathBuf;

pub const ENV_ASSET: &str = "CLEARCORE_DEV_ENROLLMENT_ASSET";
pub const ENV_SHA256: &str = "CLEARCORE_DEV_ENROLLMENT_SHA256";

#[derive(Clone, PartialEq, Eq, Default)]
pub struct EnrollmentConfig {
    pub archive_path: Option<PathBuf>,
    pub expected_sha256: Option<String>,
}

impl std::fmt::Debug for EnrollmentConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EnrollmentConfig")
            .field("configured", &self.is_configured())
            .finish()
    }
}

/// Converts `OsString` to `Option<String>`, returning `None` on non-UTF8.
fn os_to_string(os: OsString) -> Option<String> {
    os.into_string().ok()
}

impl EnrollmentConfig {
    /// Constructs from a lookup function; trims strings, treats empty as None, lowercases SHA.
    /// Validates that SHA is 64 lowercase hex and path is absolute.
    #[must_use]
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self {
        let archive_path = get(ENV_ASSET).and_then(|s| {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(PathBuf::from(trimmed))
            }
        });

        let expected_sha256 = get(ENV_SHA256).and_then(|s| {
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

    /// Constructs from environment variables.
    /// Uses `std::env::var_os` for the asset path (handles non-UTF8 gracefully)
    /// and `std::env::var` for the SHA256.
    #[must_use]
    pub fn from_env() -> Self {
        let archive_path = std::env::var_os(ENV_ASSET)
            .and_then(os_to_string)
            .and_then(|s| {
                let trimmed = s.trim();
                if trimmed.is_empty() {
                    None
                } else {
                    Some(PathBuf::from(trimmed))
                }
            });

        let expected_sha256 = std::env::var(ENV_SHA256).ok().and_then(|s| {
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

    /// Returns true only if both `archive_path` and `expected_sha256` are present,
    /// the path is absolute, and the `sha256` is exactly 64 lowercase hex characters.
    /// The daemon's working directory varies; relative paths are never configured.
    #[must_use]
    pub fn is_configured(&self) -> bool {
        matches!(
            (&self.archive_path, &self.expected_sha256),
            (Some(path), Some(sha)) if path.is_absolute()
                && sha.len() == 64
                && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
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
            ENV_ASSET => Some(String::new()),
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
    fn uppercase_sha_is_normalized_when_from_lookup() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/path".to_string()),
            ENV_SHA256 => Some("AB".repeat(32)),
            _ => None,
        });
        assert!(cfg.is_configured());
        assert_eq!(cfg.expected_sha256, Some("ab".repeat(32)));
    }

    #[test]
    fn uppercase_sha_direct_construction_is_not_configured() {
        let cfg = EnrollmentConfig {
            archive_path: Some(PathBuf::from("/path")),
            expected_sha256: Some("AB".repeat(32)),
        };
        assert!(!cfg.is_configured());
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

    #[test]
    fn relative_path_is_not_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("relative/path".to_string()),
            ENV_SHA256 => Some("ab".repeat(32)),
            _ => None,
        });
        assert!(!cfg.is_configured());
    }

    #[test]
    fn absolute_path_with_lowercase_sha_is_configured() {
        let cfg = EnrollmentConfig::from_lookup(|k| match k {
            ENV_ASSET => Some("/absolute/path".to_string()),
            ENV_SHA256 => Some("cd".repeat(32)),
            _ => None,
        });
        assert!(cfg.is_configured());
    }

    #[test]
    fn debug_does_not_print_path_or_sha() {
        let cfg = EnrollmentConfig {
            archive_path: Some(PathBuf::from("/secret/path")),
            expected_sha256: Some("ab".repeat(32)),
        };
        let debug_str = format!("{cfg:?}");
        assert!(!debug_str.contains("/secret/path"));
        assert!(!debug_str.contains("ab"));
        assert!(debug_str.contains("configured"));
    }
}
