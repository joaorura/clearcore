//! The pDFNet3 Pro v3 full that every package ships as its ONLY isolation model, under
//! `<models>/pdfnet3/`, and the enrollment v3 model it pairs with, under `<models>/enrollment/`.
//!
//! - [`PDFNET3_PRO_V3_FULL_FILE`]: ONE stateful manifest export (`manifest.json` + `enc`/`erb_dec`/
//!   `df_dec`) run by the generic stateful executor on every runtime: tract or `OpenVINO` in the
//!   daemon, ONNX Runtime or `OpenVINO` in `libclearcore_filter` (the virtual microphone).
//! - [`ENROLLMENT_V3_FILE`]: the enrollment v3 `enrollment.onnx` (`audio` [1, N] 16 kHz ->
//!   `speaker_embedding` [1, 192], ECAPA L2) whose embedding conditions the Pro v3 full.
//!
//! Each file is pinned by the SHA-256 of the WHOLE file ([`PDFNET3_PRO_V3_FULL_SHA256`],
//! [`ENROLLMENT_V3_SHA256`]), mirrored by `crates/app-tauri/scripts/package-app.mjs` and the
//! release workflow. There is no fallback to the base `DFNet3` nor to the older Pro v2 exports: a
//! missing or tampered archive is an error the caller reports.

use std::path::{Path, PathBuf};

use crate::{PdfNet3DevArchive, PdfNet3DevError};

/// Sub-directory of the models directory that holds the bundled archives.
pub const PDFNET3_BUNDLE_DIR: &str = "pdfnet3";
/// pDFNet3 Pro v3 **full** archive (speaker memory + cross-attention, `speaker_embedding` conditioning).
///
/// ONE stateful manifest export (`manifest.json` + `enc`/`erb_dec`/`df_dec`, opset 12, `S = 1`) run
/// by the generic stateful executor on every runtime (tract in the daemon, `OpenVINO` CPU/GPU/NPU,
/// ONNX Runtime in the virtual microphone), so the daemon and the filter run the same graphs.
pub const PDFNET3_PRO_V3_FULL_FILE: &str = "pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz";
/// SHA-256 (lowercase hex) of the whole [`PDFNET3_PRO_V3_FULL_FILE`] archive (12 375 900 bytes,
/// handed off on 2026-11-09; per-graph SHA-256 in its `manifest.json`).
pub const PDFNET3_PRO_V3_FULL_SHA256: &str =
    "c1bee5593870bc326913e805803dbcfb19632236f0f42163635a5cc502d256ee";
/// Per-graph SHA-256 of the Pro v3 full export.
///
/// Export `clearcore-train/runs/m3_deploy_pro_v3_full`, parity ORT/`OpenVINO`/tract >= 107.6 dB
/// against `PyTorch`. Checked against the archive's
/// `manifest.json` on top of the whole-archive pin.
pub const PDFNET3_PRO_V3_FULL_GRAPH_SHA256: [(&str, &str); 3] = [
    (
        "enc.onnx",
        "daf285e7026e75a1cde945fa6d1f7d2bd6a67fd0b832db96a3bcbcf476780ca6",
    ),
    (
        "erb_dec.onnx",
        "35974689c35e4aa00622c236f534e999dc737c407894e1a2a17c04c2ac34f31c",
    ),
    (
        "df_dec.onnx",
        "42c5c8b610022125c88553a323e0566c2f9553ccaf5f466d3a637bc5bc2b9df4",
    ),
];
/// Sub-directory of the models directory that holds the enrollment model.
pub const ENROLLMENT_BUNDLE_DIR: &str = "enrollment";
/// Enrollment v3 model (speaker embedding of the Pro v3 full).
pub const ENROLLMENT_V3_FILE: &str = "enrollment.onnx";
/// SHA-256 (lowercase hex) of the whole [`ENROLLMENT_V3_FILE`] (84 012 231 bytes, `clearcore-train`
/// `scripts/export_enrollment_v3.py`, parity `PyTorch` x ORT >= 117.5 dB, tract x ORT 103.5 dB).
pub const ENROLLMENT_V3_SHA256: &str =
    "82a292b95121dd7d7e82a2f4515497512693d499f06bafa0d3d627bed34ad3c1";
/// Asset id reported by every backend built from a bundled archive.
pub const PDFNET3_PRO_ASSET_ID: &str = "pdfnet3-pro-v3";
/// Optional override of the models directory (the one that contains `pdfnet3/`).
pub const ENV_MODELS_DIR: &str = "CLEARCORE_MODELS_DIR";
/// Fixed code when no candidate directory holds the requested archive.
pub const MODEL_ASSET_NOT_FOUND: &str = "MODEL_ASSET_NOT_FOUND";

/// Installation prefixes searched after the paths relative to the process.
const SYSTEM_MODEL_DIRS: &[&str] = &[
    "/opt/clearcore/models",
    "/opt/clearcore/resources/models",
    "/usr/share/clearcore/models",
    "/usr/local/share/clearcore/models",
];

/// Why the bundled archive could not be used. Codes are fixed (`MODEL_*`), never a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundledModelError {
    /// No candidate directory contains the archive.
    NotFound,
    /// The archive exists but failed verification or loading.
    Invalid(PdfNet3DevError),
}

impl BundledModelError {
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::NotFound => MODEL_ASSET_NOT_FOUND,
            Self::Invalid(error) => error.model_code(),
        }
    }
}

impl std::fmt::Display for BundledModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => f.write_str("bundled pDFNet3 Pro archive not found"),
            Self::Invalid(error) => write!(f, "bundled pDFNet3 Pro archive rejected: {error}"),
        }
    }
}

impl std::error::Error for BundledModelError {}

/// Candidate models directories, most specific first.
///
/// Order: [`ENV_MODELS_DIR`], `models/` (and
/// `resources/models/`) in the current directory, the executable's directory and their
/// ancestors, the user data directory, then the system prefixes.
#[must_use]
pub fn candidate_model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::var_os(ENV_MODELS_DIR).filter(|value| !value.is_empty()) {
        dirs.push(PathBuf::from(dir));
    }
    let mut push_ancestors = |start: &Path| {
        for ancestor in start.ancestors().take(6) {
            dirs.push(ancestor.join("models"));
            dirs.push(ancestor.join("resources/models"));
        }
    };
    if let Ok(cwd) = std::env::current_dir() {
        push_ancestors(&cwd);
    }
    if let Some(exe_dir) = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
    {
        push_ancestors(&exe_dir);
    }
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".local/share"))
        });
    if let Some(data_home) = data_home {
        dirs.push(data_home.join("clearcore/models"));
    }
    dirs.extend(SYSTEM_MODEL_DIRS.iter().map(PathBuf::from));
    dirs
}

/// First `<dir>/pdfnet3/<file>` that is a regular file, over `dirs` in order.
#[must_use]
pub fn find_in(dirs: &[PathBuf], file: &str) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(PDFNET3_BUNDLE_DIR).join(file))
        .find(|candidate| candidate.is_file())
}

/// Locates the bundled archive `file` over [`candidate_model_dirs`].
#[must_use]
pub fn find_bundled(file: &str) -> Option<PathBuf> {
    find_in(&candidate_model_dirs(), file)
}

/// Reads and verifies the bundled pDFNet3 Pro v3 full export (manifest export, every runtime)
/// found over `dirs`.
pub fn load_isolation_archive_from(
    dirs: &[PathBuf],
) -> Result<PdfNet3DevArchive, BundledModelError> {
    let archive = load_from(dirs, PDFNET3_PRO_V3_FULL_FILE, PDFNET3_PRO_V3_FULL_SHA256)?;
    let files = archive
        .stateful_files()
        .map_err(BundledModelError::Invalid)?;
    let pinned = PDFNET3_PRO_V3_FULL_GRAPH_SHA256.iter().all(|(file, hex)| {
        files
            .manifest
            .sha256
            .iter()
            .any(|(listed, digest)| listed == file && digest == hex)
    });
    if pinned {
        Ok(archive)
    } else {
        Err(BundledModelError::Invalid(PdfNet3DevError::HashMismatch))
    }
}

/// [`load_isolation_archive_from`] over [`candidate_model_dirs`]: the single isolation model of
/// the daemon (tract or `OpenVINO`) and of the virtual microphone (ONNX Runtime).
pub fn load_isolation_archive() -> Result<PdfNet3DevArchive, BundledModelError> {
    load_isolation_archive_from(&candidate_model_dirs())
}

fn load_from(
    dirs: &[PathBuf],
    file: &str,
    sha256: &str,
) -> Result<PdfNet3DevArchive, BundledModelError> {
    let path = find_in(dirs, file).ok_or(BundledModelError::NotFound)?;
    PdfNet3DevArchive::read_pinned(&path, sha256, PDFNET3_PRO_ASSET_ID)
        .map_err(BundledModelError::Invalid)
}

/// SHA-256 (lowercase hex) of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

#[cfg(all(test, feature = "tract"))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{CpuProfile, InferenceBackend};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn repository_models() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models")
    }

    #[test]
    fn bundled_v3_full_archive_loads_with_the_speaker_embedding() -> TestResult {
        let archive = load_isolation_archive_from(&[repository_models()])?;
        assert_eq!(archive.sha256(), PDFNET3_PRO_V3_FULL_SHA256);
        assert_eq!(archive.asset_id(), PDFNET3_PRO_ASSET_ID);
        assert!(archive.is_manifest_export());
        let backend = archive.instantiate(CpuProfile::Avx2Minimum)?;
        assert!(backend.supports_speaker_embedding());
        assert_eq!(backend.descriptor().asset_id, PDFNET3_PRO_ASSET_ID);
        Ok(())
    }

    #[cfg(feature = "ort")]
    #[test]
    fn bundled_v3_full_archive_loads_on_onnx_runtime() -> TestResult {
        let archive = load_isolation_archive_from(&[repository_models()])?;
        let backend = archive.instantiate_stateful_ort(CpuProfile::Avx2Minimum)?;
        assert!(backend.supports_speaker_embedding());
        assert_eq!(backend.descriptor().asset_id, PDFNET3_PRO_ASSET_ID);
        Ok(())
    }

    #[test]
    fn bundled_enrollment_v3_matches_its_pin() -> TestResult {
        let path = repository_models()
            .join(ENROLLMENT_BUNDLE_DIR)
            .join(ENROLLMENT_V3_FILE);
        assert_eq!(sha256_hex(&std::fs::read(path)?), ENROLLMENT_V3_SHA256);
        Ok(())
    }

    #[test]
    fn the_old_pro_v2_exports_are_not_bundled_any_more() {
        let bundle = repository_models().join(PDFNET3_BUNDLE_DIR);
        for old in [
            "pdfnet3-release-asset-v1.tar.gz",
            "pdfnet3-stateful-asset-v1.tar.gz",
        ] {
            assert!(!bundle.join(old).exists(), "{old}");
        }
    }

    #[test]
    fn missing_bundle_is_not_found() -> TestResult {
        let dir = tempfile::tempdir()?;
        let error = load_isolation_archive_from(&[dir.path().to_path_buf()]).unwrap_err();
        assert_eq!(error, BundledModelError::NotFound);
        assert_eq!(error.code(), "MODEL_ASSET_NOT_FOUND");
        Ok(())
    }

    #[test]
    fn tampered_bundle_is_a_hash_mismatch_without_fallback() -> TestResult {
        let dir = tempfile::tempdir()?;
        let bundle = dir.path().join(PDFNET3_BUNDLE_DIR);
        std::fs::create_dir_all(&bundle)?;
        std::fs::write(
            bundle.join(PDFNET3_PRO_V3_FULL_FILE),
            b"not the pinned archive",
        )?;
        // A valid archive later in the search order must NOT be used once a candidate exists.
        let error = load_isolation_archive_from(&[dir.path().to_path_buf(), repository_models()])
            .unwrap_err();
        assert_eq!(
            error,
            BundledModelError::Invalid(PdfNet3DevError::HashMismatch)
        );
        assert_eq!(error.code(), "MODEL_HASH_MISMATCH");
        Ok(())
    }

    #[test]
    fn search_order_prefers_the_first_directory() -> TestResult {
        let first = tempfile::tempdir()?;
        let second = tempfile::tempdir()?;
        for dir in [&first, &second] {
            std::fs::create_dir_all(dir.path().join(PDFNET3_BUNDLE_DIR))?;
            std::fs::write(
                dir.path()
                    .join(PDFNET3_BUNDLE_DIR)
                    .join(PDFNET3_PRO_V3_FULL_FILE),
                b"x",
            )?;
        }
        let found = find_in(
            &[
                PathBuf::from("/nonexistent/clearcore"),
                first.path().to_path_buf(),
                second.path().to_path_buf(),
            ],
            PDFNET3_PRO_V3_FULL_FILE,
        );
        assert_eq!(
            found,
            Some(
                first
                    .path()
                    .join(PDFNET3_BUNDLE_DIR)
                    .join(PDFNET3_PRO_V3_FULL_FILE)
            )
        );
        Ok(())
    }

    #[test]
    fn system_prefixes_are_searched() {
        let dirs = candidate_model_dirs();
        assert!(dirs.contains(&PathBuf::from("/opt/clearcore/models")));
        assert!(dirs.contains(&PathBuf::from("/usr/share/clearcore/models")));
    }
}
