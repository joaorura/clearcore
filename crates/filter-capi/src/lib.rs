#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{
    CpuProfile, InferenceBackend, InferenceError, PdfNet3DevArchive, ProfileStore, StudioBackend,
    TractBackend, VoiceProfile,
};
use studio_dsp::{Preset, StudioControl};

/// Default deployment path of the pDFNet3 Pro v2 archive (Stage 2, M3 run). Used as the tier-1
/// candidate when the `CLEARCORE_DEV_PDFNET3_ASSET` / `CLEARCORE_DEV_PDFNET3_SHA256` environment
/// variables are not set.
pub const PDFNET3_PRO_V2_DEFAULT_ASSET: &str =
    "/home/joaorura/orca/projects/clearcore-train/runs/m3_deploy_pro_v2_20261009/pdfnet3-release-asset-v1.tar.gz";
/// SHA-256 of [`PDFNET3_PRO_V2_DEFAULT_ASSET`] (the whole archive, lowercase hex).
pub const PDFNET3_PRO_V2_DEFAULT_SHA256: &str =
    "a3db32ae85a1c9dc81d97a548cc1d5c0c411186ff95fab8591d91d553453bdb2";

pub struct ClearcoreFilter {
    backend: StudioBackend<TractBackend>,
    control: Arc<StudioControl>,
    supports_film: bool,
    active_profile_id: Option<String>,
}

/// Candidate for the (unsigned, development) pDFNet3 Pro v2 archive, resolved the same way the
/// service daemon reads it: both `CLEARCORE_DEV_PDFNET3_ASSET` and `CLEARCORE_DEV_PDFNET3_SHA256`
/// must be set together (both valid). If neither is set, the machine-default archive
/// ([`PDFNET3_PRO_V2_DEFAULT_ASSET`] with [`PDFNET3_PRO_V2_DEFAULT_SHA256`]) is used. A partial or
/// malformed env configuration is treated as "not configured" (the caller then falls through to
/// the base manifest) rather than silently loading a different asset.
fn dev_pdfnet3_candidate() -> Option<(PathBuf, String)> {
    const ENV_ASSET: &str = "CLEARCORE_DEV_PDFNET3_ASSET";
    const ENV_SHA: &str = "CLEARCORE_DEV_PDFNET3_SHA256";
    let asset = std::env::var_os(ENV_ASSET).filter(|v| !v.is_empty());
    let sha = std::env::var_os(ENV_SHA).filter(|v| !v.is_empty());
    match (asset, sha) {
        (Some(a), Some(s)) => {
            let path = PathBuf::from(a);
            let sha = s.into_string().ok()?;
            let valid = path.is_absolute()
                && sha.len() == 64
                && sha.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
            if valid {
                Some((path, sha))
            } else {
                None
            }
        }
        (None, None) => Some((
            PathBuf::from(PDFNET3_PRO_V2_DEFAULT_ASSET),
            PDFNET3_PRO_V2_DEFAULT_SHA256.to_owned(),
        )),
        // One variable set alone or a non-UTF-8 value: not a valid configuration -> skip tier 1.
        _ => None,
    }
}

/// Tier-1 -> tier-2 model resolution.
///
/// Tier 1: when the pDFNet3 Pro v2 archive is available (env vars or machine default) and its
/// whole-file SHA-256 matches, it is verified (allowlisted members, no symlinks/exec bits) and
/// instantiated as a `TractBackend`. The loaded graphs must declare the `FiLM` inputs; if they do
/// not, the archive is rejected and we fall through to tier 2.
///
/// Tier 2: the approved base DeepFilterNet3 manifest from `repo_root` (`vendor/approved/`).
fn resolve_tract_backend(_repo_root: &Path) -> Option<TractBackend> {
    let (path, sha) = dev_pdfnet3_candidate()?;
    let archive = PdfNet3DevArchive::read(&path, &sha).ok()?;
    archive.instantiate(CpuProfile::Avx2Minimum).ok()
}

/// Clears the active voice profile and restores neutral (identity) conditioning. `Ok(())` on
/// success, an [`InferenceError`] otherwise.
fn clear_to_neutral(backend: &mut StudioBackend<TractBackend>, active_id: &mut Option<String>) {
    let _ = backend.set_voice_profile(None);
    *active_id = None;
}

/// Applies `profile` to the backend. A non-identity profile on a model without `FiLM` inputs is
/// rejected up front ([`InferenceError::UnsupportedFeature`]); the previous profile stays active.
fn apply_profile(
    backend: &mut StudioBackend<TractBackend>,
    supports_film: bool,
    active_id: &mut Option<String>,
    profile: &VoiceProfile,
) -> Result<(), InferenceError> {
    if !supports_film && !profile.is_neutral() {
        return Err(InferenceError::UnsupportedFeature(
            "voice profile conditioning".into(),
        ));
    }
    backend.set_voice_profile(Some(profile))?;
    *active_id = Some(profile.id.clone());
    Ok(())
}

/// Best-effort load of `~/.local/share/clearcore/profiles/active_profile.json` from
/// [`ProfileStore::default_dir`]. Failures are swallowed: a missing or corrupt profile simply
/// leaves the filter in neutral conditioning rather than aborting filter construction.
fn try_load_active_profile(filter: &mut ClearcoreFilter) {
    let Some(dir) = ProfileStore::default_dir() else {
        return;
    };
    let store = ProfileStore::new(dir);
    let loaded = store.load_active().unwrap_or(None);
    if let Some(profile) = loaded {
        let _ = apply_profile(&mut filter.backend, filter.supports_film, &mut filter.active_profile_id, &profile);
    }
}

/// Create a new neural filter instance.
///
/// The model asset is resolved through a two-tier policy: first the pDFNet3 Pro v2 development
/// archive (env vars `CLEARCORE_DEV_PDFNET3_ASSET`/`_SHA256`, or the machine-default archive at
/// [`PDFNET3_PRO_V2_DEFAULT_ASSET`] — verified against [`PDFNET3_PRO_V2_DEFAULT_SHA256`]); on any
/// failure it falls back to the approved base DeepFilterNet3 manifest under `repo_root`. When the
/// Pro v2 archive is loaded, its `FiLM` inputs are detected and a persisted
/// `active_profile.json` (if any) is applied atomically as part of construction.
///
/// After construction the studio preset starts at `Off` (bit-exact passthrough of the neural
/// output); select another one with `clearcore_filter_set_preset`.
///
/// # Safety
/// If `repo_root_path` is non-null, it must be a valid null-terminated C string. This function
/// performs disk I/O and must be called from a control thread, never from the realtime audio
/// callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_create(
    repo_root_path: *const c_char,
) -> *mut ClearcoreFilter {
    let repo_root = if repo_root_path.is_null() {
        Path::new(".")
    } else {
        let c_str = unsafe { CStr::from_ptr(repo_root_path) };
        match c_str.to_str() {
            Ok(s) => Path::new(s),
            Err(_) => return std::ptr::null_mut(),
        }
    };

    let backend = match resolve_tract_backend(repo_root) {
        Some(backend) => backend,
        None => return std::ptr::null_mut(),
    };
    let supports_film = backend.supports_speaker_conditioning();

    let control = Arc::new(StudioControl::new(Preset::Off));
    let mut filter = ClearcoreFilter {
        backend: StudioBackend::new(backend, Arc::clone(&control)),
        control,
        supports_film,
        active_profile_id: None,
    };
    // Best-effort: activate the persisted profile. Ignored on failure so creation is robust
    // against a corrupt/missing profile file (fail-closed to neutral, not to no device).
    try_load_active_profile(&mut filter);
    let filter = Box::new(filter);
    Box::into_raw(filter)
}

/// Process a single 480-sample (10 ms @ 48 kHz) audio frame through the DeepFilterNet3 neural network and the studio chain.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create`.
/// `in_samples` and `out_samples` must each point to at least 480 contiguous `f32` samples.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_process(
    filter: *mut ClearcoreFilter,
    in_samples: *const f32,
    out_samples: *mut f32,
) -> i32 {
    if filter.is_null() || in_samples.is_null() || out_samples.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &mut *filter };
    let in_slice = unsafe { std::slice::from_raw_parts(in_samples, 480) };
    let mut in_frame: AudioFrame = [0.0; 480];
    in_frame.copy_from_slice(in_slice);

    match filter_ref.backend.process(&in_frame) {
        Ok(processed) => {
            let out_slice = unsafe { std::slice::from_raw_parts_mut(out_samples, 480) };
            out_slice.copy_from_slice(&processed.samples);
            0
        }
        Err(_) => -2,
    }
}

/// Select the studio finishing preset applied after the neural network.
///
/// `preset` is `0` = Off, `1` = Natural, `2` = Podcast, `3` = Broadcast. The change takes effect at
/// the next 480-sample frame boundary.
///
/// Returns `0` on success, `-1` if `filter` is NULL and `-3` if `preset` is not one of the values
/// above (the current preset is left untouched). `-2` is reserved for `clearcore_filter_process`.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create`, or NULL. It must not be
/// called concurrently with `clearcore_filter_process` or `clearcore_filter_free` on the same
/// handle: call it from the thread that calls `clearcore_filter_process` (the C helper does).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_preset(
    filter: *mut ClearcoreFilter,
    preset: u8,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let Some(preset) = Preset::from_u8(preset) else {
        return -3;
    };
    let filter_ref = unsafe { &*filter };
    filter_ref.control.set_preset(preset);
    0
}

/// Destroy and free a neural filter instance.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create` or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_free(filter: *mut ClearcoreFilter) {
    if !filter.is_null() {
        unsafe {
            drop(Box::from_raw(filter));
        }
    }
}

/// Activate a voice profile from a JSON payload.
///
/// # Returns
/// - `0`: success, profile applied (its id is cached).
/// - `-1`: `filter` or `profile_json` is NULL.
/// - `-2`: non-UTF-8 payload, JSON deserialization failure or integrity mismatch.
/// - `-3`: the loaded model cannot apply a conditioned profile (base DFNet3).
/// - `-4`: backend conditioning error.
///
/// # Safety
/// `filter` must be a valid pointer from `clearcore_filter_create` or NULL. Must be called outside
/// the realtime audio callback and not concurrently with `clearcore_filter_process` on the same
/// handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_voice_profile(
    filter: *mut ClearcoreFilter,
    profile_json: *const c_char,
) -> i32 {
    if filter.is_null() || profile_json.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &mut *filter };
    let c_str = unsafe { CStr::from_ptr(profile_json) };
    let json = match c_str.to_str() {
        Ok(s) => s,
        Err(_) => return -2,
    };
    let profile = match VoiceProfile::from_json(json) {
        Ok(p) => p,
        Err(_) => return -2,
    };
    match apply_profile(
        &mut filter_ref.backend,
        filter_ref.supports_film,
        &mut filter_ref.active_profile_id,
        &profile,
    ) {
        Ok(()) => 0,
        Err(InferenceError::UnsupportedFeature(_)) => -3,
        Err(_) => -4,
    }
}

/// Clear the active voice profile and restore neutral (identity) conditioning.
///
/// # Returns
/// - `0`: success.
/// - `-1`: `filter` is NULL.
/// - `-2`: backend conditioning error.
///
/// # Safety
/// Same threading contract as `clearcore_filter_set_voice_profile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_clear_voice_profile(filter: *mut ClearcoreFilter) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &mut *filter };
    match filter_ref.backend.set_voice_profile(None) {
        Ok(()) => {
            filter_ref.active_profile_id = None;
            0
        }
        Err(_) => -2,
    }
}

/// Reload the persisted `active_profile.json` from [`ProfileStore::default_dir`].
///
/// # Returns
/// - `1`: a profile was loaded from disk and applied.
/// - `0`: no profile file found; neutral conditioning applied.
/// - `-1`: `filter` is NULL.
/// - `-2`: a profile file exists but failed verification (permissions, JSON, hash).
/// - `-3`: the loaded model cannot apply a conditioned profile (base DFNet3).
/// - `-4`: backend conditioning error.
///
/// # Safety
/// Performs disk I/O; call from a control thread, never inside the audio callback, and not
/// concurrently with `clearcore_filter_process` on the same handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_reload_active_profile(filter: *mut ClearcoreFilter) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &mut *filter };
    let Some(dir) = ProfileStore::default_dir() else {
        clear_to_neutral(&mut filter_ref.backend, &mut filter_ref.active_profile_id);
        return 0;
    };
    let store = ProfileStore::new(dir);
    match store.load_active() {
        Ok(None) => {
            clear_to_neutral(&mut filter_ref.backend, &mut filter_ref.active_profile_id);
            0
        }
        Ok(Some(profile)) => {
            match apply_profile(
                &mut filter_ref.backend,
                filter_ref.supports_film,
                &mut filter_ref.active_profile_id,
                &profile,
            ) {
                Ok(()) => 1,
                Err(InferenceError::UnsupportedFeature(_)) => -3,
                Err(_) => -4,
            }
        }
        Err(_) => -2,
    }
}

/// Report whether the loaded model can apply a voice profile (`1`) or is the unconditioned base
/// model (`0`). Returns `-1` if `filter` is NULL.
///
/// # Safety
/// `filter` must be a valid pointer from `clearcore_filter_create` or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_supports_voice_profile(
    filter: *mut ClearcoreFilter,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &*filter };
    i32::from(filter_ref.supports_film)
}

/// Report whether a voice profile is currently active (`1`) or the filter is running neutral
/// (`0`). Returns `-1` if `filter` is NULL.
///
/// # Safety
/// `filter` must be a valid pointer from `clearcore_filter_create` or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_is_voice_profile_active(
    filter: *mut ClearcoreFilter,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &*filter };
    i32::from(filter_ref.active_profile_id.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{CString, OsString};
    use std::sync::Mutex;
    use realtime_noise_model::{BandGains, FiLMVectors, FILM_HIDDEN_DIM};
    use tempfile::TempDir;

    /// Serializes env-touching tests so `CLEARCORE_DEV_PDFNET3_*` and `XDG_DATA_HOME` mutations
    /// never race. Each holder also gets an empty, isolated profile-store directory so `create_filter`
    /// always starts from neutral conditioning regardless of any real machine state.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Guard returned by [`lock_env`]. Releases the env lock (and the temp dir) on drop; the env vars
    /// it sets are not automatically restored because the lock is process-wide and exclusive — each
    /// test sets what it needs before touching the FFI. Tests that alter `CLEARCORE_DEV_*` restore
    /// them via [`DisableProTier`].
    struct EnvGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
        _data: TempDir,
    }

    fn lock_env() -> EnvGuard {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let data = tempfile::tempdir().expect("tempdir");
        // SAFETY (edition 2024): `set_var` is unsafe; we hold the process-wide env lock for the
        // whole test so the mutation is race-free within this test binary.
        unsafe { std::env::set_var("XDG_DATA_HOME", data.path()) };
        EnvGuard {
            _lock: lock,
            _data: data,
        }
    }

    /// Forces tier-2 (approved base DeepFilterNet3) by making `dev_pdfnet3_candidate` return `None`
    /// through a *partial* env configuration (asset set, SHA unset). Restores the previous values on
    /// drop so subsequent tests see the original machine config.
    struct DisableProTier {
        prev_asset: Option<OsString>,
        prev_sha: Option<OsString>,
    }

    impl DisableProTier {
        fn engage() -> Self {
            let prev_asset = std::env::var_os("CLEARCORE_DEV_PDFNET3_ASSET");
            let prev_sha = std::env::var_os("CLEARCORE_DEV_PDFNET3_SHA256");
            unsafe {
                std::env::set_var(
                    "CLEARCORE_DEV_PDFNET3_ASSET",
                    "/nonexistent/clearcore-tier1-disabled",
                )
            };
            unsafe { std::env::remove_var("CLEARCORE_DEV_PDFNET3_SHA256") };
            Self {
                prev_asset,
                prev_sha,
            }
        }
    }

    impl Drop for DisableProTier {
        fn drop(&mut self) {
            unsafe {
                match self.prev_asset.take() {
                    Some(v) => std::env::set_var("CLEARCORE_DEV_PDFNET3_ASSET", v),
                    None => std::env::remove_var("CLEARCORE_DEV_PDFNET3_ASSET"),
                }
                match self.prev_sha.take() {
                    Some(v) => std::env::set_var("CLEARCORE_DEV_PDFNET3_SHA256", v),
                    None => std::env::remove_var("CLEARCORE_DEV_PDFNET3_SHA256"),
                }
            }
        }
    }

    fn repo_root() -> CString {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        CString::new(root.to_string_lossy().as_bytes()).unwrap_or_default()
    }

    fn create_filter() -> *mut ClearcoreFilter {
        let root = repo_root();
        unsafe { clearcore_filter_create(root.as_ptr()) }
    }

    /// Voiced-speech-like frame: 120 Hz fundamental with 20 harmonics and a 4 Hz syllabic envelope.
    fn voiced_frame(index: u32) -> [f32; 480] {
        let mut frame = [0.0_f32; 480];
        for (offset, sample) in (0_u32..).zip(frame.iter_mut()) {
            let t = (f64::from(index) * 480.0 + f64::from(offset)) / 48_000.0;
            let envelope = 0.5 + 0.5 * (std::f64::consts::TAU * 4.0 * t).sin();
            let voiced: f64 = (1..=20_u32)
                .map(|h| {
                    let h = f64::from(h);
                    (std::f64::consts::TAU * 120.0 * h * t).sin() / h
                })
                .sum();
            *sample = (0.25 * envelope * voiced) as f32;
        }
        frame
    }

    fn process(filter: *mut ClearcoreFilter, input: &[f32; 480]) -> ([f32; 480], i32) {
        let mut output = [0.0_f32; 480];
        let rc = unsafe { clearcore_filter_process(filter, input.as_ptr(), output.as_mut_ptr()) };
        (output, rc)
    }

    /// A non-identity (conditioned) profile: gamma=1.2, beta=0.05, neutral EQ. Mirrors the test
    /// fixture in `crates/model/src/pdfnet3_dev.rs` so it is within `FiLM` range bounds.
    fn conditioned_profile(id: &str) -> VoiceProfile {
        let film = FiLMVectors::new(
            vec![1.2; FILM_HIDDEN_DIM],
            vec![0.05; FILM_HIDDEN_DIM],
            vec![1.2; FILM_HIDDEN_DIM],
            vec![0.05; FILM_HIDDEN_DIM],
        )
        .expect("conditioned FiLM vectors");
        VoiceProfile::new(id, "Speaker", "2026-10-05T12:00:00Z", film, Some(BandGains::neutral()))
            .expect("conditioned voice profile")
    }

    /// Neutral identity profile: accepted by both the FiLM-capable pro model and the base model.
    fn identity_profile(id: &str) -> VoiceProfile {
        VoiceProfile::identity(id, "Speaker", "2026-10-05T12:00:00Z").expect("identity profile")
    }

    /// `true` when this machine will resolve the pDFNet3 Pro v2 archive through tier 1.
    fn pro_archive_present() -> bool {
        dev_pdfnet3_candidate()
            .map(|(path, sha)| PdfNet3DevArchive::read(&path, &sha).is_ok())
            .unwrap_or(false)
    }

    #[test]
    fn set_preset_rejects_a_null_handle() {
        assert_eq!(
            unsafe { clearcore_filter_set_preset(std::ptr::null_mut(), 0) },
            -1
        );
    }

    #[test]
    fn set_preset_rejects_unknown_values_and_keeps_the_filter_usable() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 4) }, -3);
        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 255) }, -3);
        assert_eq!(process(filter, &voiced_frame(0)).1, 0);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn set_preset_accepts_every_preset() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        for value in 0..=3_u8 {
            assert_eq!(unsafe { clearcore_filter_set_preset(filter, value) }, 0);
        }
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn new_filter_reports_the_model_latency_plus_the_chain() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        let latency = unsafe { (*filter).backend.algorithmic_latency_samples() };
        assert_eq!(latency, 1_440 + 96);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn off_is_bit_exact_with_the_raw_neural_backend_and_a_preset_changes_it() {
        let _env = lock_env();
        let root = repo_root();
        let root_path = Path::new(root.to_str().unwrap_or("."));
        // Match `clearcore_filter_create`'s exact model-resolution so the reference and the FFI
        // filter use the same (tier-1 or tier-2) graphs — otherwise the bit-exact comparison
        // below is meaningless on machines that ship the pro archive.
        let reference = resolve_tract_backend(root_path);
        assert!(reference.is_some());
        let Some(mut reference) = reference else { return };

        // `lock_env` already pointed XDG_DATA_HOME at an empty temp dir, so `create_filter` starts
        // from neutral conditioning and is comparable to the unconditioned `reference`.
        let filter = create_filter();
        assert!(!filter.is_null());

        for index in 0..30 {
            let input = voiced_frame(index);
            let expected = reference.process(&input).map(|frame| frame.samples).ok();
            let (output, rc) = process(filter, &input);
            assert_eq!(rc, 0);
            assert_eq!(Some(output), expected, "Off must not alter frame {index}");
        }

        assert_eq!(unsafe { clearcore_filter_set_preset(filter, 3) }, 0);
        let mut changed = false;
        for index in 30..130 {
            let input = voiced_frame(index);
            let expected = reference.process(&input).map(|frame| frame.samples).ok();
            let (output, rc) = process(filter, &input);
            assert_eq!(rc, 0);
            if Some(output) != expected {
                changed = true;
                break;
            }
        }
        assert!(changed, "Broadcast never altered the neural output");
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn voice_profile_support_flag_matches_the_resolved_model_tier() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        assert_eq!(
            unsafe { clearcore_filter_supports_voice_profile(filter) },
            i32::from(unsafe { (*filter).supports_film })
        );
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn create_strictly_requires_pdfnet3_pro() {
        let _env = lock_env();
        let filter = create_filter();
        if pro_archive_present() {
            assert!(!filter.is_null());
            let supports = unsafe { clearcore_filter_supports_voice_profile(filter) };
            assert_eq!(
                supports, 1,
                "pDFNet3 Pro v2 archive present -> model must expose FiLM inputs"
            );
            assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 0);
            unsafe { clearcore_filter_free(filter) };
        } else {
            assert!(filter.is_null(), "must be null when pro archive is absent");
        }
    }

    // --- pDFNet3 Pro v2 voice-profile acceptance (presence-gated) ---

    #[test]
    fn set_voice_profile_applies_a_conditioned_profile_on_the_pro_model() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        // Presence-gated: a conditioned profile only applies when tier 1 (FiLM) is active.
        if unsafe { clearcore_filter_supports_voice_profile(filter) } != 1 {
            unsafe { clearcore_filter_free(filter) };
            eprintln!("skipped: pDFNet3 pro archive not present on this machine");
            return;
        }
        let json = CString::new(conditioned_profile("spk-pro-set").to_json().expect("json")).unwrap();
        assert_eq!(unsafe { clearcore_filter_set_voice_profile(filter, json.as_ptr()) }, 0);
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 1);
        assert_eq!(unsafe { (*filter).active_profile_id.as_deref() }, Some("spk-pro-set"));

        // Clearing must restore neutral and forget the cached id.
        assert_eq!(unsafe { clearcore_filter_clear_voice_profile(filter) }, 0);
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 0);
        assert_eq!(unsafe { (*filter).active_profile_id.as_deref() }, None);
        unsafe { clearcore_filter_free(filter) };
    }

    // --- always-run error/neutral paths (independent of which model tier is present) ---

    #[test]
    fn set_voice_profile_null_and_payload_errors() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        let json = CString::new(conditioned_profile("spk-x").to_json().expect("json")).unwrap();

        // NULL handle / NULL payload.
        assert_eq!(
            unsafe { clearcore_filter_set_voice_profile(std::ptr::null_mut(), json.as_ptr()) },
            -1
        );
        assert_eq!(unsafe { clearcore_filter_set_voice_profile(filter, std::ptr::null()) }, -1);

        // Malformed JSON -> -2 (parse failure before any integrity check).
        let garbage = CString::new("{ this is not valid json").unwrap();
        assert_eq!(
            unsafe { clearcore_filter_set_voice_profile(filter, garbage.as_ptr()) },
            -2
        );

        // Tampered integrity (the integrity hash no longer matches its body).
        let tampered = json.to_str().expect("utf8").replace("Speaker", "Imposter");
        let tampered = CString::new(tampered).unwrap();
        assert_eq!(
            unsafe { clearcore_filter_set_voice_profile(filter, tampered.as_ptr()) },
            -2
        );
        // A failed apply must not leave a stale active id behind.
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 0);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn clear_voice_profile_on_a_neutral_filter_is_a_no_op() {
        let _env = lock_env();
        let filter = create_filter();
        assert!(!filter.is_null());
        assert_eq!(unsafe { clearcore_filter_clear_voice_profile(filter) }, 0);
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 0);
        assert_eq!(unsafe { clearcore_filter_clear_voice_profile(std::ptr::null_mut()) }, -1);
        unsafe { clearcore_filter_free(filter) };
    }

    #[test]
    fn disabled_pro_tier_yields_null_filter_without_fallback() {
        let _env = lock_env();
        let _pro = DisableProTier::engage();
        assert!(!pro_archive_present(), "precondition: pro tier must be disabled");

        let filter = create_filter();
        assert!(filter.is_null(), "without pro archive, filter creation must fail closed with no fallback");
    }

    #[test]
    fn reload_active_profile_reads_the_store_and_falls_back_to_neutral() {
        let _env = lock_env();
        // `lock_env` already redirected XDG_DATA_HOME to an empty dir; `ProfileStore::default_dir`
        // therefore points inside the temp dir we own.
        let profiles_dir = ProfileStore::default_dir().expect("default_dir under XDG_DATA_HOME");
        let store = ProfileStore::new(&profiles_dir);

        let filter = create_filter();
        assert!(!filter.is_null());
        // No profile on disk yet -> neutral.
        assert_eq!(unsafe { clearcore_filter_reload_active_profile(filter) }, 0);
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 0);

        // An identity profile loads successfully on both the base and the pro model.
        store.save_active(&identity_profile("spk-reload")).expect("save active");
        assert_eq!(unsafe { clearcore_filter_reload_active_profile(filter) }, 1);
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 1);

        // `reload` honors the NULL handle.
        assert_eq!(
            unsafe { clearcore_filter_reload_active_profile(std::ptr::null_mut()) },
            -1
        );

        // Removing the file returns the filter to neutral.
        store.clear_active().expect("clear active");
        assert_eq!(unsafe { clearcore_filter_reload_active_profile(filter) }, 0);
        assert_eq!(unsafe { clearcore_filter_is_voice_profile_active(filter) }, 0);
        unsafe { clearcore_filter_free(filter) };
    }
}
