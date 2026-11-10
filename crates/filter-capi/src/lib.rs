#![allow(unsafe_code)]
#![allow(clippy::missing_safety_doc)]

//! `libclearcore_filter`: the neural filter of the virtual microphone, loaded by the PipeWire
//! helper (`platform/linux/helper`) through `dlopen`.
//!
//! - Model: pDFNet3 Pro v3 FULL driven by its manifest (speaker embedding + cross-attention), or
//!   the v2 stateful Pro (`FiLM`) until v3 is installed — see [`select`].
//! - Runtime: ONNX Runtime CPU, or OpenVINO NPU → GPU → CPU per the `backend` setting with ONNX
//!   Runtime as hot standby on failure or overrun — see [`engine`].
//! - Profile: `active_profile.json` v2 (speaker embedding + EQ) — a v1 (`FiLM`) file is outdated
//!   for the v3 model (neutral voice); the EQ is applied before the iSTFT — see [`core`],
//!   [`profile_v2`] and `docs/virtual-mic-filter.md`.
//!
//! The C ABI is unchanged for the helper; `clearcore_filter_status_json` is new and optional.

pub mod core;
pub mod engine;
pub mod legacy;
pub mod profile_v2;
pub mod select;
pub mod v3;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::Path;
use std::sync::Arc;

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::{ACTIVE_PROFILE_FILE_NAME, ALGORITHM_LATENCY_SAMPLES, ProfileStore};
use realtime_noise_service::settings::{Settings, default_settings_path};
use studio_dsp::{Preset, StudioChain, StudioControl};

use crate::core::{ModelKind, NeuralCore};
use crate::engine::{Conditioning, LatencyPolicy};
use crate::profile_v2::StoredProfile;

pub use crate::select::{ENV_DEV_STATEFUL_ASSET, ENV_DEV_STATEFUL_SHA256};

pub struct ClearcoreFilter {
    core: NeuralCore,
    chain: StudioChain,
    control: Arc<StudioControl>,
    applied_preset: Preset,
    applied_leveler: u8,
    active_profile_id: Option<String>,
}

/// What [`ClearcoreFilter::apply_profile`] did with a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileOutcome {
    /// Applied (EQ, and the speaker part when the model consumes it).
    Applied,
    /// A v1 (`FiLM`) profile on the v3 model: outdated, neutral voice applied (no EQ either),
    /// the filter keeps running. Enrollment asks the user to regenerate it.
    Outdated,
}

impl ClearcoreFilter {
    /// Builds the filter for `repo_root` with the given `backend` request (see [`select`]).
    pub fn new(repo_root: &Path, backend: Option<&str>) -> Result<Self, String> {
        let source = select::resolve_model(repo_root)?;
        let engine = select::build_engine(&source, backend, LatencyPolicy::from_env())?;
        let core = NeuralCore::new(source.kind(), engine).map_err(|e| format!("{e:?}"))?;
        let control = Arc::new(StudioControl::new(Preset::Off));
        let applied_preset = control.preset();
        let applied_leveler = control.leveler_intensity();
        let mut chain = StudioChain::new(applied_preset);
        chain.set_leveler_intensity(applied_leveler);
        Ok(Self {
            core,
            chain,
            control,
            applied_preset,
            applied_leveler,
            active_profile_id: None,
        })
    }

    /// One hop through the neural core and the studio chain.
    pub fn process(&mut self, input: &AudioFrame) -> Result<AudioFrame, String> {
        let mut out = self.core.process(input).map_err(|e| format!("{e:?}"))?;
        let preset = self.control.preset();
        if preset != self.applied_preset {
            self.chain.set_preset(preset);
            self.applied_preset = preset;
        }
        let leveler = self.control.leveler_intensity();
        if leveler != self.applied_leveler {
            self.chain.set_leveler_intensity(leveler);
            self.applied_leveler = leveler;
        }
        self.chain.process(&mut out);
        if out.iter().all(|s| s.is_finite()) {
            Ok(out)
        } else {
            Err("non-finite output".to_owned())
        }
    }

    #[must_use]
    pub fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES + self.chain.latency_samples()
    }

    #[must_use]
    pub const fn model_kind(&self) -> ModelKind {
        self.core.kind
    }

    #[must_use]
    pub fn core(&self) -> &NeuralCore {
        &self.core
    }

    pub fn core_mut(&mut self) -> &mut NeuralCore {
        &mut self.core
    }

    /// Applies `profile`:
    /// - v2 profile on the v3 model: `speaker_embedding` (+ `speaker_present = 1`) and the EQ;
    /// - v1 profile on the v2 stateful model: `FiLM` (scaled by `intensity`, as before) and the EQ;
    /// - v1 profile on the v3 model: outdated → neutral voice, no EQ ([`ProfileOutcome::Outdated`]);
    /// - v2 profile on the v2 stateful model: the EQ only (that model cannot use an embedding).
    ///
    /// `intensity` 0 turns the speaker part off (`speaker_present = 0`, zeros); any other value
    /// feeds the embedding as stored (the model was trained with `speaker_present` in {0, 1}).
    /// Nothing changes on error.
    pub fn apply_profile(
        &mut self,
        profile: &StoredProfile,
        intensity: u8,
    ) -> Result<ProfileOutcome, String> {
        let (eq, conditioning) = match (self.core.kind, profile) {
            (ModelKind::V3Full, StoredProfile::V2(p)) => (
                p.eq.clone(),
                Conditioning {
                    embedding: if intensity > 0 {
                        Some(p.speaker_embedding()?)
                    } else {
                        None
                    },
                    film: None,
                },
            ),
            (ModelKind::V3Full, StoredProfile::V1(_)) => {
                self.clear_profile()?;
                return Ok(ProfileOutcome::Outdated);
            }
            (ModelKind::V2Film, StoredProfile::V1(p)) => (
                p.eq.clone(),
                Conditioning {
                    embedding: None,
                    film: Some(p.film.with_intensity(intensity)),
                },
            ),
            (ModelKind::V2Film, StoredProfile::V2(p)) => (p.eq.clone(), Conditioning::default()),
        };
        self.set_profile_parts(eq, conditioning)?;
        self.active_profile_id = Some(profile.id().to_owned());
        Ok(ProfileOutcome::Applied)
    }

    /// Neutral conditioning, no EQ.
    pub fn clear_profile(&mut self) -> Result<(), String> {
        self.set_profile_parts(None, Conditioning::default())?;
        self.active_profile_id = None;
        Ok(())
    }

    fn set_profile_parts(
        &mut self,
        eq: Option<realtime_noise_model::BandGains>,
        conditioning: Conditioning,
    ) -> Result<(), String> {
        let previous = self.core_conditioning();
        self.core
            .set_conditioning(conditioning)
            .map_err(|e| format!("{e:?}"))?;
        if let Err(e) = self.core.set_eq(eq) {
            let _ = self.core.set_conditioning(previous);
            return Err(format!("{e:?}"));
        }
        Ok(())
    }

    fn core_conditioning(&self) -> Conditioning {
        self.core.conditioning().clone()
    }

    /// Reads the active profile from `ProfileStore::default_dir()` honouring `settings.json`.
    /// Returns the C codes of `clearcore_filter_reload_active_profile`.
    pub fn reload_active_profile(&mut self) -> i32 {
        let settings = Settings::load(&default_settings_path());
        let neutral = |f: &mut Self| if f.clear_profile().is_ok() { 0 } else { -4 };
        if !settings.voice_isolation_enabled {
            return neutral(self);
        }
        let Some(dir) = ProfileStore::default_dir() else {
            return neutral(self);
        };
        match StoredProfile::load_secure(&dir.join(ACTIVE_PROFILE_FILE_NAME)) {
            Err(_) => -2,
            Ok(None) => neutral(self),
            Ok(Some(profile)) => {
                match self.apply_profile(&profile, settings.voice_isolation_intensity) {
                    Ok(ProfileOutcome::Applied) => 1,
                    Ok(ProfileOutcome::Outdated) => 0,
                    Err(_) => -4,
                }
            }
        }
    }

    /// Diagnostics as a JSON object (see `clearcore_filter_status_json`).
    #[must_use]
    pub fn status_json(&self) -> String {
        let latency = self.core.engine.latency();
        serde_json::json!({
            "model": self.core.kind.name(),
            "backend": self.core.engine.label(),
            "fallback_reason": self.core.engine.fallback_reason(),
            "standby": self.core.engine.has_standby(),
            "profile_id": self.active_profile_id,
            "speaker_conditioning": self.core.speaker_active(),
            "eq": self.core.eq_active(),
            "silent_hops": self.core.silent_hops(),
            "latency_us": {
                "hops": latency.hops,
                "p50": latency.p50_us,
                "p99": latency.p99_us,
                "max": latency.max_us,
            },
        })
        .to_string()
    }
}

fn repo_root_from(ptr: *const c_char) -> Option<&'static Path> {
    if ptr.is_null() {
        return Some(Path::new("."));
    }
    // SAFETY: the caller passes a valid NUL-terminated string that outlives the call; the path
    // is only used during `clearcore_filter_create`.
    let c_str = unsafe { CStr::from_ptr(ptr) };
    c_str.to_str().ok().map(Path::new)
}

/// Create a new neural filter instance.
///
/// Resolves the model fail-closed (v3 FULL, else v2 stateful Pro; never a base model), compiles it
/// for the `backend` of `settings.json` (or `CLEARCORE_FILTER_BACKEND`) and applies the persisted
/// active profile. Returns `NULL` on any failure. The studio preset starts at `Off`.
///
/// # Safety
/// If `repo_root_path` is non-null, it must be a valid null-terminated C string. Performs disk
/// I/O and compilation: call from a control thread, never from the realtime audio callback.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_create(
    repo_root_path: *const c_char,
) -> *mut ClearcoreFilter {
    let Some(repo_root) = repo_root_from(repo_root_path) else {
        return std::ptr::null_mut();
    };
    let settings = Settings::load(&default_settings_path());
    let backend = select::requested_backend(settings.backend.as_deref());
    match ClearcoreFilter::new(repo_root, backend.as_deref()) {
        Ok(mut filter) => {
            // Best effort: a missing or corrupt profile leaves the filter neutral.
            let _ = filter.reload_active_profile();
            Box::into_raw(Box::new(filter))
        }
        Err(error) => {
            eprintln!(
                "[clearcore_filter] create failed for {}: {error}",
                repo_root.display()
            );
            std::ptr::null_mut()
        }
    }
}

/// Process a single 480-sample (10 ms @ 48 kHz) audio frame. Returns 0, -1 on NULL arguments and
/// -2 on an inference error (the output is left untouched).
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
    let mut in_frame: AudioFrame = [0.0; 480];
    in_frame.copy_from_slice(unsafe { std::slice::from_raw_parts(in_samples, 480) });
    match filter_ref.process(&in_frame) {
        Ok(out) => {
            unsafe { std::slice::from_raw_parts_mut(out_samples, 480) }.copy_from_slice(&out);
            0
        }
        Err(_) => -2,
    }
}

/// Select the studio finishing preset (`0` Off, `1` Natural, `2` Podcast, `3` Broadcast).
/// Returns `0`, `-1` for NULL, `-3` for an unknown value.
///
/// # Safety
/// `filter` must be a valid pointer from `clearcore_filter_create`, or NULL; same thread as
/// `clearcore_filter_process`.
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
    unsafe { &*filter }.control.set_preset(preset);
    0
}

/// Set the voice auto-leveler intensity (0–100). Returns `0`, or `-1` for NULL.
///
/// # Safety
/// `filter` must be a valid pointer from `clearcore_filter_create`, or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_leveler_intensity(
    filter: *mut ClearcoreFilter,
    intensity: u8,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    unsafe { &*filter }.control.set_leveler_intensity(intensity);
    0
}

/// Destroy and free a neural filter instance.
///
/// # Safety
/// `filter` must be a valid pointer returned by `clearcore_filter_create` or NULL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_free(filter: *mut ClearcoreFilter) {
    if !filter.is_null() {
        drop(unsafe { Box::from_raw(filter) });
    }
}

/// Activate a voice profile from a JSON payload (schema v2: speaker embedding + EQ; or v1).
/// Applied at full intensity.
///
/// Returns `0` applied; `-1` NULL argument; `-2` non-UTF-8, invalid JSON, unknown version or
/// integrity mismatch; `-3` a v1 profile on the v3 model (outdated: neutral voice applied, the
/// filter keeps running); `-4` conditioning error (previous profile kept).
///
/// # Safety
/// `filter` must be a valid pointer from `clearcore_filter_create` or NULL. Call outside the
/// realtime callback and not concurrently with `clearcore_filter_process` on the same handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_set_voice_profile(
    filter: *mut ClearcoreFilter,
    profile_json: *const c_char,
) -> i32 {
    if filter.is_null() || profile_json.is_null() {
        return -1;
    }
    let filter_ref = unsafe { &mut *filter };
    let Ok(json) = unsafe { CStr::from_ptr(profile_json) }.to_str() else {
        return -2;
    };
    let Ok(profile) = StoredProfile::from_json(json) else {
        return -2;
    };
    match filter_ref.apply_profile(&profile, 100) {
        Ok(ProfileOutcome::Applied) => 0,
        Ok(ProfileOutcome::Outdated) => -3,
        Err(_) => -4,
    }
}

/// Clear the active voice profile (neutral conditioning, no EQ). `0`, `-1` NULL, `-2` error.
///
/// # Safety
/// Same contract as `clearcore_filter_set_voice_profile`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_clear_voice_profile(filter: *mut ClearcoreFilter) -> i32 {
    if filter.is_null() {
        return -1;
    }
    match unsafe { &mut *filter }.clear_profile() {
        Ok(()) => 0,
        Err(_) => -2,
    }
}

/// Reload the persisted `active_profile.json` from `ProfileStore::default_dir()`, honouring
/// `voice_isolation_enabled`/`_intensity` of `settings.json`.
///
/// Returns `1` loaded and applied; `0` none, isolation disabled, or a v1 profile on the v3 model
/// (outdated → neutral voice); `-1` NULL; `-2` a profile file exists but
/// failed verification (permissions, JSON, hash; previous state kept); `-4` conditioning error.
///
/// # Safety
/// Disk I/O: call from a control thread, never inside the audio callback, and not concurrently
/// with `clearcore_filter_process` on the same handle.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_reload_active_profile(
    filter: *mut ClearcoreFilter,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    unsafe { &mut *filter }.reload_active_profile()
}

/// `1`: the loaded model consumes a speaker (v3 embedding or v2 `FiLM`); `-1` for NULL.
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
    i32::from(unsafe { &*filter }.core.engine.supports_speaker())
}

/// `1` when a voice profile is active, `0` when neutral, `-1` for NULL.
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
    i32::from(unsafe { &*filter }.active_profile_id.is_some())
}

/// Writes the diagnostics JSON (`model`, `backend`, `fallback_reason`, `standby`, `profile_id`,
/// `speaker_conditioning`, `eq`, `silent_hops`, `latency_us` {`hops`,`p50`,`p99`,`max`}) into
/// `buf` (NUL-terminated, truncated to `len - 1` bytes). Returns the full length without the NUL,
/// or `-1` when `filter` is NULL. `buf` may be NULL with `len` 0 to query the length.
///
/// # Safety
/// `filter` as above; `buf` must point to `len` writable bytes when non-NULL. Same thread as
/// `clearcore_filter_process` (it reads the latency record).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn clearcore_filter_status_json(
    filter: *mut ClearcoreFilter,
    buf: *mut c_char,
    len: usize,
) -> i32 {
    if filter.is_null() {
        return -1;
    }
    let json = unsafe { &*filter }.status_json();
    if !buf.is_null() && len > 0 {
        let n = json.len().min(len - 1);
        unsafe {
            std::ptr::copy_nonoverlapping(json.as_ptr(), buf.cast::<u8>(), n);
            *buf.add(n) = 0;
        }
    }
    i32::try_from(json.len()).unwrap_or(i32::MAX)
}

#[cfg(test)]
mod tests;
