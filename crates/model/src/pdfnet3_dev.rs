//! Development-only loader for the unsigned pDFNet3 (`FiLM`) archive produced by the training
//! repository (`clearcore-train`, M3: `pdfnet3-release-asset-v1.tar.gz`).
//!
//! This is NOT the signed asset gate ([`crate::ModelAssetRegistry`]): the archive is trusted only
//! because the developer pinned the SHA-256 of the WHOLE archive in the environment. The model it
//! carries is the M2 checkpoint that was judged NO-GO (no proven isolation benefit over the neutral
//! base); loading it only lets the runtime ACCEPT a voice profile during development. It must never
//! be reported as an approved or production model.
//!
//! Checks, in order: file size ceiling, SHA-256 of the whole archive (before any decompression),
//! then a bounded tar walk that accepts exactly the four flat members `enc.onnx`, `erb_dec.onnx`,
//! `df_dec.onnx` and `config.ini` (regular files, no paths, no duplicates, bounded sizes). Only
//! then are the bytes handed to tract, and the loaded graphs must declare the `FiLM` inputs.
//! Errors are fixed codes: no path, hash or member name is ever echoed.

use std::{fmt, fs::File, io::Read, path::Path, sync::Arc};

use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::{BackendDescriptor, CpuProfile, TractBackend};

/// Asset id reported by the backend descriptor. Deliberately says "unapproved".
pub const PDFNET3_DEV_ASSET_ID: &str = "pdfnet3-dev-unapproved";

/// The exact flat members the development archive must contain, each exactly once.
const MEMBERS: [&str; 4] = ["enc.onnx", "erb_dec.onnx", "df_dec.onnx", "config.ini"];
/// The real archive is about 8 MB; anything above this is refused before it is read.
pub const MAX_ARCHIVE_BYTES: u64 = 64 * 1024 * 1024;
/// Each ONNX member is about 2-3.5 MB.
const MAX_ONNX_MEMBER_BYTES: u64 = 32 * 1024 * 1024;
/// `config.ini` is about 2 KB.
const MAX_CONFIG_MEMBER_BYTES: u64 = 64 * 1024;
/// Ceiling on the total decompressed bytes the tar walk may consume (headers included).
const MAX_DECOMPRESSED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 8;

/// Why the development pDFNet3 archive was not loaded. Fixed codes only (see [`Self::code`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfNet3DevError {
    /// The file is missing, not a regular file or could not be read.
    AssetUnreadable,
    /// The file is larger than [`MAX_ARCHIVE_BYTES`].
    AssetTooLarge,
    /// The SHA-256 of the whole archive is not the pinned one (checked before decompressing).
    HashMismatch,
    /// The archive content is not exactly the four expected members.
    ArchiveInvalid(&'static str),
    /// tract could not build the model, or it breaks the frozen DSP contract.
    LoadFailed,
    /// The model loaded but has no `FiLM` inputs, so it could never apply a voice profile.
    FilmMissing,
}

impl PdfNet3DevError {
    /// Short machine code for status/diagnostics (never a path).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::AssetUnreadable => "DEV_MODEL_ASSET_UNREADABLE",
            Self::AssetTooLarge => "DEV_MODEL_ASSET_TOO_LARGE",
            Self::HashMismatch => "DEV_MODEL_HASH_MISMATCH",
            Self::ArchiveInvalid(_) => "DEV_MODEL_ARCHIVE_INVALID",
            Self::LoadFailed => "DEV_MODEL_LOAD_FAILED",
            Self::FilmMissing => "DEV_MODEL_NO_FILM",
        }
    }
}

impl fmt::Display for PdfNet3DevError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetUnreadable => f.write_str("development pDFNet3 archive is unreadable"),
            Self::AssetTooLarge => f.write_str("development pDFNet3 archive is too large"),
            Self::HashMismatch => f.write_str("development pDFNet3 archive hash mismatch"),
            Self::ArchiveInvalid(reason) => {
                write!(f, "development pDFNet3 archive is invalid: {reason}")
            }
            Self::LoadFailed => f.write_str("development pDFNet3 model failed to load"),
            Self::FilmMissing => f.write_str("development pDFNet3 model has no FiLM inputs"),
        }
    }
}

impl std::error::Error for PdfNet3DevError {}

/// A development pDFNet3 archive whose hash and member list were verified. Cheap to clone; every
/// [`Self::instantiate`] builds a fresh backend (fresh recurrent state) from the same bytes.
#[derive(Clone)]
pub struct PdfNet3DevArchive {
    bytes: Arc<[u8]>,
    sha256: String,
}

impl fmt::Debug for PdfNet3DevArchive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PdfNet3DevArchive")
            .field("size_bytes", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl PdfNet3DevArchive {
    /// Reads `path` (size-capped) and verifies it like [`Self::verify`].
    pub fn read(path: &Path, expected_sha256_hex: &str) -> Result<Self, PdfNet3DevError> {
        let file = File::open(path).map_err(|_| PdfNet3DevError::AssetUnreadable)?;
        let metadata = file
            .metadata()
            .map_err(|_| PdfNet3DevError::AssetUnreadable)?;
        if !metadata.is_file() {
            return Err(PdfNet3DevError::AssetUnreadable);
        }
        if metadata.len() > MAX_ARCHIVE_BYTES {
            return Err(PdfNet3DevError::AssetTooLarge);
        }
        let mut bytes = Vec::new();
        file.take(MAX_ARCHIVE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| PdfNet3DevError::AssetUnreadable)?;
        Self::verify(bytes, expected_sha256_hex)
    }

    /// Verifies an in-memory archive: size, whole-archive SHA-256 (lowercase hex, before any
    /// decompression), then the exact member list.
    pub fn verify(bytes: Vec<u8>, expected_sha256_hex: &str) -> Result<Self, PdfNet3DevError> {
        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err(PdfNet3DevError::AssetTooLarge);
        }
        let actual = sha256_hex(&bytes);
        if expected_sha256_hex.len() != 64 || actual != expected_sha256_hex {
            return Err(PdfNet3DevError::HashMismatch);
        }
        validate_members(&bytes)?;
        Ok(Self {
            bytes: Arc::from(bytes),
            sha256: actual,
        })
    }

    /// Lowercase hex SHA-256 of the whole archive (the pinned value).
    #[must_use]
    pub fn sha256(&self) -> &str {
        &self.sha256
    }

    /// Builds a tract backend with this model. Fails with [`PdfNet3DevError::FilmMissing`] if the
    /// graphs do not declare the `FiLM` inputs: a backend that cannot apply a profile is useless
    /// as the development profile backend and must not be reported as `pdfnet3-dev`.
    pub fn instantiate(&self, profile: CpuProfile) -> Result<TractBackend, PdfNet3DevError> {
        profile
            .ensure_supported()
            .map_err(|_| PdfNet3DevError::LoadFailed)?;
        let descriptor = BackendDescriptor {
            backend: "tract",
            backend_version: "deep_filter-v0.5.6",
            runtime: "tract",
            runtime_version: "0.19.16",
            asset_id: PDFNET3_DEV_ASSET_ID.to_owned(),
            asset_sha256: self.sha256.clone(),
            cpu_profile: profile.name(),
        };
        let backend = TractBackend::from_archive(Arc::clone(&self.bytes), descriptor)
            .map_err(|_| PdfNet3DevError::LoadFailed)?;
        if !backend.supports_speaker_conditioning() {
            return Err(PdfNet3DevError::FilmMissing);
        }
        Ok(backend)
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        hex.extend(
            [byte >> 4, byte & 0x0f]
                .into_iter()
                .filter_map(|n| char::from_digit(u32::from(n), 16)),
        );
    }
    hex
}

fn validate_members(bytes: &[u8]) -> Result<(), PdfNet3DevError> {
    let invalid = PdfNet3DevError::ArchiveInvalid;
    let mut archive = Archive::new(GzDecoder::new(bytes).take(MAX_DECOMPRESSED_BYTES));
    let entries = archive
        .entries()
        .map_err(|_| invalid("unreadable archive"))?;
    let mut seen = [false; MEMBERS.len()];
    for (index, entry) in entries.enumerate() {
        if index >= MAX_ENTRIES {
            return Err(invalid("too many entries"));
        }
        let mut entry = entry.map_err(|_| invalid("unreadable archive"))?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid("member is not a regular file"));
        }
        let position = {
            let path = entry
                .path()
                .map_err(|_| invalid("member path is not valid"))?;
            path.to_str()
                .and_then(|name| MEMBERS.iter().position(|member| *member == name))
                .ok_or_else(|| invalid("unexpected member"))?
        };
        if seen[position] {
            return Err(invalid("duplicate member"));
        }
        seen[position] = true;
        let limit = if MEMBERS[position] == "config.ini" {
            MAX_CONFIG_MEMBER_BYTES
        } else {
            MAX_ONNX_MEMBER_BYTES
        };
        if entry.size() > limit {
            return Err(invalid("member is too large"));
        }
        // Read through the member so a short or corrupt stream fails here, not inside tract.
        let read = std::io::copy(&mut entry.by_ref().take(limit + 1), &mut std::io::sink())
            .map_err(|_| invalid("member is unreadable"))?;
        if read > limit || read != entry.size() {
            return Err(invalid("member is truncated or too large"));
        }
    }
    if seen.iter().all(|present| *present) {
        Ok(())
    } else {
        Err(invalid("missing member"))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::{
        fs,
        io::Write,
        path::PathBuf,
        time::{Duration, Instant},
    };

    use flate2::{Compression, write::GzEncoder};
    use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
    use tar::{Builder, Header};

    use super::*;
    use crate::{FILM_HIDDEN_DIM, FiLMVectors, InferenceBackend, VoiceProfile};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const REAL_ARCHIVE: &str =
        "/home/joaorura/orca/projects/clearcore-train/runs/m3/pdfnet3-release-asset-v1.tar.gz";
    const REAL_SHA256: &str = "42dfc577fdf8a881ecbafce7777bf6f0a4cf914ffc1aaff2580aec0cbac79505";

    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (name, bytes) in members {
            let mut header = Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            builder
                .append_data(&mut header, name, *bytes)
                .expect("append");
        }
        builder.into_inner().expect("tar").finish().expect("gzip")
    }

    fn fake_members() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("enc.onnx", b"enc"),
            ("erb_dec.onnx", b"erb"),
            ("df_dec.onnx", b"df"),
            ("config.ini", b"[df]"),
        ]
    }

    fn verify(bytes: &[u8]) -> Result<PdfNet3DevArchive, PdfNet3DevError> {
        PdfNet3DevArchive::verify(bytes.to_vec(), &sha256_hex(bytes))
    }

    /// Hand-written gzipped tar so hostile names and entry types are not normalised by `Builder`.
    fn raw_archive(entries: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        for (name, kind, data) in entries {
            raw_entry(&mut gz, name, *kind, data.len() as u64);
            gz.write_all(data).expect("data");
            gz.write_all(&vec![0; (512 - data.len() % 512) % 512])
                .expect("pad");
        }
        gz.write_all(&[0; 1024]).expect("end");
        gz.finish().expect("gzip")
    }

    fn raw_entry(out: &mut impl Write, name: &str, kind: u8, size: u64) {
        let mut h = [0_u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[108..115].copy_from_slice(b"0000000");
        h[116..123].copy_from_slice(b"0000000");
        h[124..135].copy_from_slice(format!("{size:011o}").as_bytes());
        h[136..147].copy_from_slice(b"00000000000");
        h[156] = kind;
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        h[148..156].copy_from_slice(b"        ");
        let sum: u32 = h.iter().map(|b| u32::from(*b)).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        h[155] = b' ';
        out.write_all(&h).expect("header");
    }

    fn assert_invalid(result: Result<PdfNet3DevArchive, PdfNet3DevError>) {
        match result {
            Err(error @ PdfNet3DevError::ArchiveInvalid(_)) => {
                assert_eq!(error.code(), "DEV_MODEL_ARCHIVE_INVALID");
            }
            other => panic!("expected ArchiveInvalid, got {other:?}"),
        }
    }

    #[test]
    fn wrong_hash_is_refused_before_decompressing() {
        // Not even gzip: a hash check that ran after decompression would report something else.
        let junk = b"definitely not a gzip stream".to_vec();
        assert_eq!(
            PdfNet3DevArchive::verify(junk, &"00".repeat(32)).unwrap_err(),
            PdfNet3DevError::HashMismatch
        );
        let good = archive(&fake_members());
        let hash = sha256_hex(&good);
        let error = PdfNet3DevArchive::verify(good.clone(), &"ab".repeat(32)).unwrap_err();
        assert_eq!(error, PdfNet3DevError::HashMismatch);
        assert!(!error.to_string().contains(&hash));
        // Only the documented lowercase form is accepted.
        assert_eq!(
            PdfNet3DevArchive::verify(good, &hash.to_uppercase()).unwrap_err(),
            PdfNet3DevError::HashMismatch
        );
    }

    #[test]
    fn exact_member_set_passes_validation() {
        let good = archive(&fake_members());
        let verified = verify(&good).expect("valid member set");
        assert_eq!(verified.sha256(), sha256_hex(&good));
        assert!(!format!("{verified:?}").contains(verified.sha256()));
    }

    #[test]
    fn extra_missing_and_duplicate_members_are_refused() {
        let mut extra = fake_members();
        extra.push(("evil.bin", b"x"));
        assert_invalid(verify(&archive(&extra)));

        for skip in 0..4 {
            let missing: Vec<_> = fake_members()
                .into_iter()
                .enumerate()
                .filter(|(index, _)| *index != skip)
                .map(|(_, member)| member)
                .collect();
            assert_invalid(verify(&archive(&missing)));
        }

        let mut duplicate = fake_members();
        duplicate.push(("enc.onnx", b"enc2"));
        assert_invalid(verify(&archive(&duplicate)));
        assert_invalid(verify(&archive(&[])));
    }

    #[test]
    fn paths_traversal_and_non_files_are_refused() {
        let base: Vec<(&str, u8, &[u8])> = vec![
            ("erb_dec.onnx", b'0', b"erb"),
            ("df_dec.onnx", b'0', b"df"),
            ("config.ini", b'0', b"[df]"),
        ];
        for hostile in [
            ("../enc.onnx", b'0', &b"enc"[..]),
            ("/enc.onnx", b'0', b"enc"),
            ("a/../enc.onnx", b'0', b"enc"),
            ("tmp/export/enc.onnx", b'0', b"enc"),
            ("./enc.onnx", b'0', b"enc"),
            ("enc.onnx", b'2', b""),
            ("enc.onnx", b'1', b""),
            ("enc.onnx", b'5', b""),
        ] {
            let mut entries = base.clone();
            entries.insert(0, hostile);
            assert_invalid(verify(&raw_archive(&entries)));
        }
    }

    #[test]
    fn oversized_member_is_refused_quickly() {
        let started = Instant::now();
        let mut gz = GzEncoder::new(Vec::new(), Compression::fast());
        let size = 40_u64 * 1024 * 1024;
        raw_entry(&mut gz, "enc.onnx", b'0', size);
        let chunk = vec![0_u8; 1024 * 1024];
        for _ in 0..40 {
            gz.write_all(&chunk).expect("zeros");
        }
        gz.write_all(&[0; 1024]).expect("end");
        let bomb = gz.finish().expect("gzip");
        assert!(bomb.len() < 4 * 1024 * 1024, "compressed bomb stays small");
        assert_invalid(verify(&bomb));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn oversized_archive_is_refused_before_hashing() -> TestResult {
        let too_big = vec![0_u8; usize::try_from(MAX_ARCHIVE_BYTES)? + 1];
        assert_eq!(
            PdfNet3DevArchive::verify(too_big, &"00".repeat(32)).unwrap_err(),
            PdfNet3DevError::AssetTooLarge
        );
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("big.tar.gz");
        let file = File::create(&path)?;
        file.set_len(MAX_ARCHIVE_BYTES + 1)?;
        assert_eq!(
            PdfNet3DevArchive::read(&path, &"00".repeat(32)).unwrap_err(),
            PdfNet3DevError::AssetTooLarge
        );
        Ok(())
    }

    #[test]
    fn missing_file_and_directory_are_unreadable() -> TestResult {
        let dir = tempfile::tempdir()?;
        assert_eq!(
            PdfNet3DevArchive::read(&dir.path().join("absent"), &"00".repeat(32)).unwrap_err(),
            PdfNet3DevError::AssetUnreadable
        );
        assert_eq!(
            PdfNet3DevArchive::read(dir.path(), &"00".repeat(32)).unwrap_err(),
            PdfNet3DevError::AssetUnreadable
        );
        Ok(())
    }

    #[test]
    fn valid_layout_with_garbage_models_fails_to_load() {
        let verified = verify(&archive(&fake_members())).expect("layout is valid");
        assert_eq!(
            verified.instantiate(CpuProfile::Avx2Minimum).err(),
            Some(PdfNet3DevError::LoadFailed)
        );
    }

    fn repository_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Repacks a `tmp/export/*` asset with the flat member names the dev archive requires.
    fn flat_repack(path: &Path) -> Vec<u8> {
        let bytes = fs::read(path).expect("fixture");
        let mut source = Archive::new(GzDecoder::new(&bytes[..]));
        let mut members: Vec<(String, Vec<u8>)> = Vec::new();
        for entry in source.entries().expect("entries") {
            let mut entry = entry.expect("entry");
            let name = entry
                .path()
                .expect("path")
                .file_name()
                .and_then(|n| n.to_str())
                .expect("name")
                .to_owned();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).expect("read");
            members.push((name, data));
        }
        let refs: Vec<(&str, &[u8])> = members
            .iter()
            .map(|(name, data)| (name.as_str(), data.as_slice()))
            .collect();
        archive(&refs)
    }

    #[test]
    fn film_fixture_loads_with_speaker_conditioning() -> TestResult {
        let bytes =
            flat_repack(&repository_root().join("fixtures/film/film-identity-asset.tar.gz"));
        let backend = verify(&bytes)?.instantiate(CpuProfile::Avx2Minimum)?;
        assert!(backend.supports_speaker_conditioning());
        assert!(InferenceBackend::supports_voice_profile(&backend));
        assert_eq!(backend.descriptor().asset_id, PDFNET3_DEV_ASSET_ID);
        Ok(())
    }

    #[test]
    fn base_model_without_film_is_refused() -> TestResult {
        let bytes = flat_repack(
            &repository_root().join("vendor/approved/df-compatible-release-asset-v1.bin"),
        );
        assert_eq!(
            verify(&bytes)?.instantiate(CpuProfile::Avx2Minimum).err(),
            Some(PdfNet3DevError::FilmMissing)
        );
        Ok(())
    }

    fn frames(count: usize) -> Vec<AudioFrame> {
        (0..count)
            .map(|frame| {
                std::array::from_fn(|index| {
                    let n =
                        f32::from(u16::try_from(frame * HOP_SAMPLES + index).unwrap_or(u16::MAX));
                    (n * 0.0291).sin() * 0.3
                })
            })
            .collect()
    }

    fn run(backend: &mut TractBackend, input: &[AudioFrame]) -> Vec<f32> {
        let mut out = Vec::new();
        for frame in input {
            let processed = backend.process(frame).expect("frame");
            assert_eq!(processed.samples.len(), HOP_SAMPLES);
            out.extend(processed.samples);
        }
        out
    }

    #[test]
    #[ignore = "reads the real M3 pDFNet3 archive from the training repository"]
    fn real_m3_archive_accepts_a_conditioned_profile() -> TestResult {
        let path = Path::new(REAL_ARCHIVE);
        if !path.exists() {
            eprintln!("real M3 pDFNet3 archive not present; skipping");
            return Ok(());
        }
        let archive = PdfNet3DevArchive::read(path, REAL_SHA256)?;
        let mut backend = archive.instantiate(CpuProfile::Avx2Minimum)?;
        assert!(backend.supports_speaker_conditioning());
        assert_eq!(backend.descriptor().asset_id, PDFNET3_DEV_ASSET_ID);
        let input = frames(40);

        let identity = VoiceProfile::identity("spk", "Speaker", "2026-10-05T12:00:00Z")?;
        backend.set_voice_profile(Some(&identity))?;
        let neutral = run(&mut backend, &input);
        assert_eq!(neutral.len(), input.len() * HOP_SAMPLES);
        assert!(neutral.iter().all(|sample| sample.is_finite()));

        let film = FiLMVectors::new(
            vec![1.2; FILM_HIDDEN_DIM],
            vec![0.05; FILM_HIDDEN_DIM],
            vec![1.2; FILM_HIDDEN_DIM],
            vec![0.05; FILM_HIDDEN_DIM],
        )?;
        let conditioned = VoiceProfile::new("spk", "Speaker", "2026-10-05T12:00:00Z", film, None)?;
        backend.set_voice_profile(Some(&conditioned))?;
        assert_eq!(backend.active_voice_profile(), Some(&conditioned));
        let out = run(&mut backend, &input);
        assert_eq!(out.len(), input.len() * HOP_SAMPLES);
        assert!(out.iter().all(|sample| sample.is_finite()));
        Ok(())
    }
}
