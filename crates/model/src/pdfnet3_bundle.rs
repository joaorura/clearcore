//! The pDFNet3 Pro (v3, `FiLM` speaker conditioning) that every package ships as its ONLY
//! isolation model, under `<models>/pdfnet3/`.
//!
//! Two exports of the same checkpoint (`clearcore-train`, run `m3_deploy_pro_v3_20261013`,
//! gates G0/G11 PASS) are bundled, each pinned here by the SHA-256 of the WHOLE archive:
//!
//! - [`PDFNET3_PRO_TRACT_FILE`]: the streaming graphs with `gamma`/`beta` inputs that `tract`
//!   pulses internally. The service daemon runs it (isolation backend that receives the voice
//!   profile, and the neutral denoiser of enrollment samples).
//! - [`PDFNET3_PRO_STATEFUL_FILE`]: the same graphs with every recurrent state exposed as a port
//!   (`h_in`/`h_out`, delay rings). `libclearcore_filter` (the virtual microphone) runs it on ONNX
//!   Runtime, one 10 ms hop at a time.
//!
//! Both archives go through [`PdfNet3DevArchive`] (size cap, whole-archive hash before any
//! decompression, exact flat member list, `FiLM` inputs required at load). There is no fallback
//! to the base `DFNet3`: a missing or tampered archive is an error the caller reports.

use std::path::{Path, PathBuf};

use crate::{PdfNet3DevArchive, PdfNet3DevError};

/// Sub-directory of the models directory that holds the bundled archives.
pub const PDFNET3_BUNDLE_DIR: &str = "pdfnet3";
/// `tract` export (streaming graphs with `FiLM` inputs, no state ports).
pub const PDFNET3_PRO_TRACT_FILE: &str = "pdfnet3-release-asset-v1.tar.gz";
/// SHA-256 (lowercase hex) of the whole [`PDFNET3_PRO_TRACT_FILE`] archive.
pub const PDFNET3_PRO_TRACT_SHA256: &str =
    "1cfa86c75e44e8ac45ace8a905bb3368c143d72e0d6b963fb8c2a3a3a1e782c8";
/// Stateful export (explicit recurrent-state ports and `FiLM` inputs) for ONNX Runtime.
pub const PDFNET3_PRO_STATEFUL_FILE: &str = "pdfnet3-stateful-asset-v1.tar.gz";
/// SHA-256 (lowercase hex) of the whole [`PDFNET3_PRO_STATEFUL_FILE`] archive.
pub const PDFNET3_PRO_STATEFUL_SHA256: &str =
    "d1b9c2fd49ba602ed48cd503d256885c5e174a869d23b62204e8d668de95d6a5";
/// pDFNet3 Pro v3 **full** (speaker memory + cross-attention, `speaker_embedding` conditioning):
/// ONE stateful manifest export (`manifest.json` + `enc`/`erb_dec`/`df_dec`, opset 12, `S = 1`) run
/// by the generic stateful executor on every runtime (tract in the daemon, `OpenVINO` CPU/GPU/NPU,
/// ONNX Runtime in the virtual microphone), so the daemon and the filter run the same graphs.
pub const PDFNET3_PRO_V3_FULL_FILE: &str = "pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz";
/// SHA-256 (lowercase hex) of the whole [`PDFNET3_PRO_V3_FULL_FILE`] archive (12 375 900 bytes,
/// handed off on 2026-11-09; per-graph SHA-256 in its `manifest.json`).
pub const PDFNET3_PRO_V3_FULL_SHA256: &str =
    "c1bee5593870bc326913e805803dbcfb19632236f0f42163635a5cc502d256ee";
/// Per-graph SHA-256 of the Pro v3 full export (`clearcore-train/runs/m3_deploy_pro_v3_full`,
/// parity ORT/`OpenVINO`/tract >= 107.6 dB against `PyTorch`). Checked against the archive's
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

/// Candidate models directories, most specific first: [`ENV_MODELS_DIR`], `models/` (and
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

/// Reads and verifies the bundled `tract` export found over `dirs`.
pub fn load_tract_archive_from(dirs: &[PathBuf]) -> Result<PdfNet3DevArchive, BundledModelError> {
    load_from(dirs, PDFNET3_PRO_TRACT_FILE, PDFNET3_PRO_TRACT_SHA256)
}

/// Reads and verifies the bundled stateful (ONNX Runtime) export found over `dirs`.
pub fn load_stateful_archive_from(
    dirs: &[PathBuf],
) -> Result<PdfNet3DevArchive, BundledModelError> {
    load_from(dirs, PDFNET3_PRO_STATEFUL_FILE, PDFNET3_PRO_STATEFUL_SHA256)
}

/// Reads and verifies the bundled pDFNet3 Pro v3 full export (manifest export, every runtime)
/// found over `dirs`.
pub fn load_isolation_archive_from(
    dirs: &[PathBuf],
) -> Result<PdfNet3DevArchive, BundledModelError> {
    let archive = load_from(dirs, PDFNET3_PRO_V3_FULL_FILE, PDFNET3_PRO_V3_FULL_SHA256)?;
    let files = archive.stateful_files().map_err(BundledModelError::Invalid)?;
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

/// [`load_tract_archive_from`] over [`candidate_model_dirs`].
pub fn load_tract_archive() -> Result<PdfNet3DevArchive, BundledModelError> {
    load_tract_archive_from(&candidate_model_dirs())
}

/// [`load_stateful_archive_from`] over [`candidate_model_dirs`].
pub fn load_stateful_archive() -> Result<PdfNet3DevArchive, BundledModelError> {
    load_stateful_archive_from(&candidate_model_dirs())
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
    fn bundled_tract_archive_loads_with_film_and_the_pro_asset_id() -> TestResult {
        let archive = load_tract_archive_from(&[repository_models()])?;
        assert_eq!(archive.sha256(), PDFNET3_PRO_TRACT_SHA256);
        assert_eq!(archive.asset_id(), PDFNET3_PRO_ASSET_ID);
        let backend = archive.instantiate(CpuProfile::Avx2Minimum)?;
        assert!(backend.supports_speaker_conditioning());
        assert!(InferenceBackend::supports_voice_profile(&backend));
        assert_eq!(backend.descriptor().asset_id, PDFNET3_PRO_ASSET_ID);
        Ok(())
    }

    #[cfg(feature = "ort")]
    #[test]
    fn bundled_stateful_archive_loads_on_onnx_runtime_with_film() -> TestResult {
        let archive = load_stateful_archive_from(&[repository_models()])?;
        assert_eq!(archive.sha256(), PDFNET3_PRO_STATEFUL_SHA256);
        let backend = archive.instantiate_ort(CpuProfile::Avx2Minimum)?;
        assert!(backend.supports_speaker_conditioning());
        assert_eq!(backend.descriptor().asset_id, PDFNET3_PRO_ASSET_ID);
        Ok(())
    }

    #[test]
    fn bundled_stateful_archive_yields_its_three_graphs() -> TestResult {
        let archive = load_stateful_archive_from(&[repository_models()])?;
        let graphs = archive.graphs()?;
        for graph in [&graphs.enc, &graphs.erb_dec, &graphs.df_dec] {
            // ONNX protobufs, 1.9-3.4 MB each.
            assert!(
                graph.len() > 1_000_000 && graph.len() < 4_000_000,
                "{}",
                graph.len()
            );
        }
        Ok(())
    }

    #[test]
    fn missing_bundle_is_not_found() -> TestResult {
        let dir = tempfile::tempdir()?;
        let error = load_tract_archive_from(&[dir.path().to_path_buf()]).unwrap_err();
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
            bundle.join(PDFNET3_PRO_TRACT_FILE),
            b"not the pinned archive",
        )?;
        // A valid archive later in the search order must NOT be used once a candidate exists.
        let error =
            load_tract_archive_from(&[dir.path().to_path_buf(), repository_models()]).unwrap_err();
        assert_eq!(
            error,
            BundledModelError::Invalid(PdfNet3DevError::HashMismatch)
        );
        assert_eq!(error.code(), "MODEL_HASH_MISMATCH");
        Ok(())
    }

    #[test]
    fn the_wrong_export_is_refused_by_its_pin() -> TestResult {
        // The stateful export under the tract name has a different hash.
        let dir = tempfile::tempdir()?;
        let bundle = dir.path().join(PDFNET3_BUNDLE_DIR);
        std::fs::create_dir_all(&bundle)?;
        std::fs::copy(
            repository_models()
                .join(PDFNET3_BUNDLE_DIR)
                .join(PDFNET3_PRO_STATEFUL_FILE),
            bundle.join(PDFNET3_PRO_TRACT_FILE),
        )?;
        assert_eq!(
            load_tract_archive_from(&[dir.path().to_path_buf()]).unwrap_err(),
            BundledModelError::Invalid(PdfNet3DevError::HashMismatch)
        );
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
                    .join(PDFNET3_PRO_TRACT_FILE),
                b"x",
            )?;
        }
        let found = find_in(
            &[
                PathBuf::from("/nonexistent/clearcore"),
                first.path().to_path_buf(),
                second.path().to_path_buf(),
            ],
            PDFNET3_PRO_TRACT_FILE,
        );
        assert_eq!(
            found,
            Some(
                first
                    .path()
                    .join(PDFNET3_BUNDLE_DIR)
                    .join(PDFNET3_PRO_TRACT_FILE)
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
