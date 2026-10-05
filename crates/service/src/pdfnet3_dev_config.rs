//! Development pDFNet3 base model (unsigned, `FiLM`), selected by two environment variables.
//!
//! `CLEARCORE_DEV_PDFNET3_ASSET` (absolute path to the M3 `pdfnet3-release-asset-v1.tar.gz`) and
//! `CLEARCORE_DEV_PDFNET3_SHA256` (64 lowercase hex characters: the SHA-256 of the WHOLE archive)
//! must be set together or not at all. Parsing is strict: no trimming, no case folding; one
//! variable alone, a relative path or a malformed hash is a configuration error, never a silent
//! "not configured". This model is the M2 NO-GO checkpoint; it exists so the runtime accepts a
//! voice profile during development and is never an approved model.

use std::path::PathBuf;

pub const ENV_ASSET: &str = "CLEARCORE_DEV_PDFNET3_ASSET";
pub const ENV_SHA256: &str = "CLEARCORE_DEV_PDFNET3_SHA256";

/// Status code when only one of the two variables is set.
pub const CONFIG_INCOMPLETE: &str = "DEV_MODEL_CONFIG_INCOMPLETE";
/// Status code for a relative/non-UTF-8 path or a hash that is not 64 lowercase hex characters.
pub const CONFIG_INVALID: &str = "DEV_MODEL_CONFIG_INVALID";

#[derive(Clone, PartialEq, Eq)]
pub struct PdfNet3DevConfig {
    pub archive_path: PathBuf,
    pub expected_sha256: String,
}

impl std::fmt::Debug for PdfNet3DevConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfNet3DevConfig").finish_non_exhaustive()
    }
}

impl PdfNet3DevConfig {
    /// `Ok(None)`: neither variable set (an empty value counts as unset). `Ok(Some)`: both set and
    /// valid. `Err(code)`: one variable alone ([`CONFIG_INCOMPLETE`]) or an invalid value
    /// ([`CONFIG_INVALID`]).
    pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, &'static str> {
        let asset = get(ENV_ASSET).filter(|value| !value.is_empty());
        let sha = get(ENV_SHA256).filter(|value| !value.is_empty());
        match (asset, sha) {
            (None, None) => Ok(None),
            (Some(_), None) | (None, Some(_)) => Err(CONFIG_INCOMPLETE),
            (Some(asset), Some(sha)) => {
                let archive_path = PathBuf::from(asset);
                let valid_sha =
                    sha.len() == 64 && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
                if archive_path.is_absolute() && valid_sha {
                    Ok(Some(Self {
                        archive_path,
                        expected_sha256: sha,
                    }))
                } else {
                    Err(CONFIG_INVALID)
                }
            }
        }
    }

    /// Reads the two variables from the process environment. A value that is not valid UTF-8 is
    /// a configuration error (it is present, just unusable).
    pub fn from_env() -> Result<Option<Self>, &'static str> {
        let read = |key: &str| std::env::var_os(key).map(std::ffi::OsString::into_string);
        let asset = read(ENV_ASSET);
        let sha = read(ENV_SHA256);
        if matches!(asset, Some(Err(_))) || matches!(sha, Some(Err(_))) {
            return Err(CONFIG_INVALID);
        }
        let asset = asset.and_then(Result::ok);
        let sha = sha.and_then(Result::ok);
        Self::from_lookup(|key| match key {
            ENV_ASSET => asset.clone(),
            ENV_SHA256 => sha.clone(),
            _ => None,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn lookup(
        asset: Option<&str>,
        sha: Option<&str>,
    ) -> Result<Option<PdfNet3DevConfig>, &'static str> {
        PdfNet3DevConfig::from_lookup(|key| match key {
            ENV_ASSET => asset.map(str::to_owned),
            ENV_SHA256 => sha.map(str::to_owned),
            _ => None,
        })
    }

    #[test]
    fn neither_variable_is_not_configured() {
        assert_eq!(lookup(None, None), Ok(None));
        assert_eq!(lookup(Some(""), Some("")), Ok(None));
    }

    #[test]
    fn both_valid_variables_are_configured() {
        let sha = "ab".repeat(32);
        let config = lookup(Some("/abs/pdfnet3.tar.gz"), Some(&sha))
            .expect("valid")
            .expect("configured");
        assert_eq!(config.archive_path, PathBuf::from("/abs/pdfnet3.tar.gz"));
        assert_eq!(config.expected_sha256, sha);
    }

    #[test]
    fn only_one_variable_is_an_error() {
        assert_eq!(lookup(Some("/abs/a.tar.gz"), None), Err(CONFIG_INCOMPLETE));
        assert_eq!(lookup(None, Some(&"ab".repeat(32))), Err(CONFIG_INCOMPLETE));
        assert_eq!(
            lookup(Some("/abs/a.tar.gz"), Some("")),
            Err(CONFIG_INCOMPLETE)
        );
    }

    #[test]
    fn invalid_values_are_an_error_not_a_silent_default() {
        let sha = "ab".repeat(32);
        assert_eq!(
            lookup(Some("relative/a.tar.gz"), Some(&sha)),
            Err(CONFIG_INVALID)
        );
        assert_eq!(
            lookup(Some("/abs"), Some(&"ab".repeat(31))),
            Err(CONFIG_INVALID)
        );
        assert_eq!(
            lookup(Some("/abs"), Some(&"AB".repeat(32))),
            Err(CONFIG_INVALID)
        );
        assert_eq!(
            lookup(Some("/abs"), Some(&"zz".repeat(32))),
            Err(CONFIG_INVALID)
        );
        assert_eq!(
            lookup(Some("/abs"), Some(&format!(" {sha}"))),
            Err(CONFIG_INVALID)
        );
        assert_eq!(lookup(Some(" /abs"), Some(&sha)), Err(CONFIG_INVALID));
    }

    #[test]
    fn debug_does_not_leak_path_or_hash() {
        let config = PdfNet3DevConfig {
            archive_path: PathBuf::from("/secret/place.tar.gz"),
            expected_sha256: "cd".repeat(32),
        };
        let debug = format!("{config:?}");
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("cdcd"));
    }
}
