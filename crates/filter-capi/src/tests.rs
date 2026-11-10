//! FFI and end-to-end tests of the filter. The v3 tests run on the synthetic fixture
//! `tests/fixtures/pdfnet3-pro-v3-full-synthetic.tar.gz` (same manifest format and I/O names as
//! the real export); the v2 tests on the bundled `models/pdfnet3/pdfnet3-stateful-asset-v1.tar.gz`.
//! OpenVINO tests skip when no runtime is found (set `OPENVINO_LIB_PATH`).

use std::ffi::{CString, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use realtime_noise_model::{BandGains, FILM_HIDDEN_DIM, FiLMVectors, SpectralModelBackend};
use tempfile::TempDir;

use super::*;
use crate::engine::{Engine, FallbackEngine, latency_stats};
use crate::profile_v2::{VoiceProfileV2, test_embedding};
use crate::select::{ENV_FILTER_BACKEND, ModelSource};
use crate::v3::{ENV_DEV_V3_ASSET, ENV_DEV_V3_SHA256, V3Asset, sha256_hex};
use realtime_noise_model::VoiceProfile;

static ENV_LOCK: Mutex<()> = Mutex::new(());

const VARS: [&str; 6] = [
    ENV_DEV_V3_ASSET,
    ENV_DEV_V3_SHA256,
    ENV_DEV_STATEFUL_ASSET,
    ENV_DEV_STATEFUL_SHA256,
    ENV_FILTER_BACKEND,
    "XDG_DATA_HOME",
];

/// Holds the process-wide env lock, points XDG dirs at an empty temp dir and restores every
/// variable it may touch on drop.
struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    data: TempDir,
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    fn new() -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let data = tempfile::tempdir().expect("tempdir");
        let mut saved: Vec<_> = VARS.iter().map(|v| (*v, std::env::var_os(v))).collect();
        saved.push(("XDG_CONFIG_HOME", std::env::var_os("XDG_CONFIG_HOME")));
        // SAFETY (edition 2024): env mutation under the process-wide test lock.
        unsafe {
            for var in VARS {
                std::env::remove_var(var);
            }
            std::env::set_var("XDG_DATA_HOME", data.path());
            std::env::set_var("XDG_CONFIG_HOME", data.path());
        }
        Self {
            _lock: lock,
            data,
            saved,
        }
    }

    fn set(&self, var: &str, value: impl AsRef<std::ffi::OsStr>) {
        unsafe { std::env::set_var(var, value) };
    }

    fn use_synthetic_v3(&self) {
        self.set(ENV_DEV_V3_ASSET, synthetic_path());
        self.set(ENV_DEV_V3_SHA256, synthetic_sha());
    }

    fn profiles_dir(&self) -> PathBuf {
        self.data.path().join("clearcore/profiles")
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        unsafe {
            for (var, value) in self.saved.drain(..) {
                match value {
                    Some(v) => std::env::set_var(var, v),
                    None => std::env::remove_var(var),
                }
            }
        }
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn synthetic_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdfnet3-pro-v3-full-synthetic.tar.gz")
}

fn synthetic_sha() -> String {
    sha256_hex(&std::fs::read(synthetic_path()).expect("fixture"))
}

fn synthetic_asset() -> V3Asset {
    V3Asset::read(&synthetic_path(), &synthetic_sha()).expect("synthetic v3 asset")
}

fn create_ffi() -> *mut ClearcoreFilter {
    let root = CString::new(repo_root().to_string_lossy().as_bytes()).expect("root");
    unsafe { clearcore_filter_create(root.as_ptr()) }
}

/// Voiced-speech-like frame: 120 Hz fundamental, 20 harmonics, 4 Hz envelope, plus a little hiss.
fn voiced_frame(index: u32) -> [f32; 480] {
    let mut frame = [0.0_f32; 480];
    let mut noise = index.wrapping_mul(747_796_405).wrapping_add(1);
    for (offset, sample) in (0_u32..).zip(frame.iter_mut()) {
        let t = (f64::from(index) * 480.0 + f64::from(offset)) / 48_000.0;
        let envelope = 0.5 + 0.5 * (std::f64::consts::TAU * 4.0 * t).sin();
        let voiced: f64 = (1..=20_u32)
            .map(|h| (std::f64::consts::TAU * 120.0 * f64::from(h) * t).sin() / f64::from(h))
            .sum();
        noise ^= noise << 13;
        noise ^= noise >> 17;
        noise ^= noise << 5;
        let hiss = f64::from(noise) / f64::from(u32::MAX) - 0.5;
        *sample = (0.25 * envelope * voiced + 0.02 * hiss) as f32;
    }
    frame
}

fn run(filter: &mut ClearcoreFilter, hops: std::ops::Range<u32>) -> Vec<f32> {
    hops.flat_map(|i| filter.process(&voiced_frame(i)).expect("hop"))
        .collect()
}

fn snr_db(reference: &[f32], test: &[f32]) -> f64 {
    let signal: f64 = reference.iter().map(|v| f64::from(*v).powi(2)).sum();
    let noise: f64 = reference
        .iter()
        .zip(test)
        .map(|(a, b)| (f64::from(*a) - f64::from(*b)).powi(2))
        .sum();
    if noise == 0.0 {
        f64::INFINITY
    } else {
        10.0 * (signal / noise).log10()
    }
}

fn v2_profile(seed: u32, eq: Option<BandGains>) -> StoredProfile {
    StoredProfile::V2(Box::new(
        VoiceProfileV2::new(
            "spk-v2",
            "Ana",
            "2026-11-09T12:00:00Z",
            &test_embedding(seed),
            eq,
        )
        .expect("v2 profile"),
    ))
}

fn v1_conditioned(id: &str) -> VoiceProfile {
    let film = FiLMVectors::new(
        vec![1.2; FILM_HIDDEN_DIM],
        vec![0.05; FILM_HIDDEN_DIM],
        vec![1.2; FILM_HIDDEN_DIM],
        vec![0.05; FILM_HIDDEN_DIM],
    )
    .expect("film");
    VoiceProfile::new(
        id,
        "Speaker",
        "2026-10-05T12:00:00Z",
        film,
        Some(BandGains::neutral()),
    )
    .expect("v1")
}

fn ov_available() -> bool {
    realtime_noise_runtime_openvino::is_openvino_available()
}

fn ov_devices() -> Vec<String> {
    realtime_noise_runtime_openvino::available_devices()
}

// ------------------------------------------------------------------ C ABI basics --------

#[test]
fn null_handles_are_rejected() {
    unsafe {
        assert_eq!(clearcore_filter_set_preset(std::ptr::null_mut(), 0), -1);
        assert_eq!(
            clearcore_filter_set_leveler_intensity(std::ptr::null_mut(), 50),
            -1
        );
        assert_eq!(
            clearcore_filter_clear_voice_profile(std::ptr::null_mut()),
            -1
        );
        assert_eq!(
            clearcore_filter_reload_active_profile(std::ptr::null_mut()),
            -1
        );
        assert_eq!(
            clearcore_filter_supports_voice_profile(std::ptr::null_mut()),
            -1
        );
        assert_eq!(
            clearcore_filter_is_voice_profile_active(std::ptr::null_mut()),
            -1
        );
        assert_eq!(
            clearcore_filter_status_json(std::ptr::null_mut(), std::ptr::null_mut(), 0),
            -1
        );
    }
}

#[test]
fn ffi_v3_create_process_presets_status_and_free() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let filter = create_ffi();
    assert!(!filter.is_null());
    unsafe {
        assert_eq!((*filter).model_kind(), ModelKind::V3Full);
        assert_eq!(clearcore_filter_supports_voice_profile(filter), 1);
        assert_eq!(clearcore_filter_is_voice_profile_active(filter), 0);
        assert_eq!(clearcore_filter_set_preset(filter, 4), -3);
        for p in 0..=3 {
            assert_eq!(clearcore_filter_set_preset(filter, p), 0);
        }
        assert_eq!(clearcore_filter_set_leveler_intensity(filter, 80), 0);
        assert_eq!((*filter).algorithmic_latency_samples(), 1_440 + 96);
        let input = voiced_frame(0);
        let mut out = [0.0f32; 480];
        assert_eq!(
            clearcore_filter_process(filter, input.as_ptr(), out.as_mut_ptr()),
            0
        );
        let needed = clearcore_filter_status_json(filter, std::ptr::null_mut(), 0);
        let mut buf = vec![0 as c_char; usize::try_from(needed).expect("len") + 1];
        assert_eq!(
            clearcore_filter_status_json(filter, buf.as_mut_ptr(), buf.len()),
            needed
        );
        let text = CStr::from_ptr(buf.as_ptr()).to_str().expect("utf8");
        let status: serde_json::Value = serde_json::from_str(text).expect("json");
        assert_eq!(status["model"], "pdfnet3-pro-v3-full");
        assert_eq!(status["backend"], "ort:CPU");
        assert_eq!(status["speaker_conditioning"], false);
        clearcore_filter_free(filter);
    }
}

#[test]
fn v3_profile_through_the_c_api_and_tamper_detection() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let filter = create_ffi();
    assert!(!filter.is_null());
    let StoredProfile::V2(profile) = v2_profile(3, None) else {
        unreachable!()
    };
    let json = CString::new(profile.to_json().expect("json")).expect("cstr");
    unsafe {
        assert_eq!(
            clearcore_filter_set_voice_profile(filter, std::ptr::null()),
            -1
        );
        assert_eq!(clearcore_filter_set_voice_profile(filter, json.as_ptr()), 0);
        assert_eq!(clearcore_filter_is_voice_profile_active(filter), 1);
        assert!((*filter).core().speaker_active());
        let tampered = CString::new(json.to_str().expect("utf8").replace("Ana", "Eva")).expect("c");
        assert_eq!(
            clearcore_filter_set_voice_profile(filter, tampered.as_ptr()),
            -2
        );
        assert!(
            (*filter).core().speaker_active(),
            "a rejected profile keeps the previous one"
        );
        assert_eq!(clearcore_filter_clear_voice_profile(filter), 0);
        assert_eq!(clearcore_filter_is_voice_profile_active(filter), 0);
        assert!(!(*filter).core().speaker_active());
        clearcore_filter_free(filter);
    }
}

// ------------------------------------------------------------------ v3 behaviour --------

#[test]
fn v3_states_advance_and_matter_and_reset_reproduces() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let asset = synthetic_asset();
    let engine = crate::v3::v3_ort_engine(&asset).expect("engine");
    let zero = engine.export_state().expect("state");
    assert!(zero.iter().flatten().all(|v| *v == 0.0));
    assert_eq!(zero.len(), 4, "enc_h, feat_erb_buf, erb_h, c0_buf");

    let mut filter = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    let first = run(&mut filter, 0..40);
    let state = filter.core.engine_state().expect("state");
    assert!(
        state.iter().all(|s| s.iter().any(|v| *v != 0.0)),
        "every state pair advanced"
    );

    // Same input with the recurrent state wiped before every hop: the output must differ.
    let mut stateless = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    let mut wiped = Vec::new();
    for i in 0..40 {
        stateless.core.zero_model_state();
        wiped.extend(stateless.process(&voiced_frame(i)).expect("hop"));
    }
    let snr = snr_db(&first, &wiped);
    eprintln!("stateless SNR {snr}");
    assert!(snr < 60.0, "state feedback has no effect ({snr:.1} dB)");

    filter.core.reset().expect("reset");
    let again = run(&mut filter, 0..40);
    assert_eq!(first, again, "reset reproduces bit for bit");
}

#[test]
fn v3_speaker_embedding_changes_the_output_and_no_profile_is_the_zero_fallback() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let mut neutral = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    let reference = run(&mut neutral, 0..60);

    let mut a = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    a.apply_profile(&v2_profile(1, None), 100).expect("profile");
    let with_a = run(&mut a, 0..60);
    let mut b = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    b.apply_profile(&v2_profile(2, None), 100).expect("profile");
    let with_b = run(&mut b, 0..60);
    let (effect, between) = (snr_db(&reference, &with_a), snr_db(&with_a, &with_b));
    eprintln!("embedding effect SNR {effect}, between speakers {between}");
    assert!(effect < 60.0, "embedding had no effect ({effect:.1} dB)");
    assert!(
        between < 60.0,
        "two speakers give the same output ({between:.1} dB)"
    );

    // Intensity 0 = speaker_present 0 + zeros: identical to no profile at all.
    let mut off = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    off.apply_profile(&v2_profile(1, None), 0).expect("profile");
    assert!(!off.core().speaker_active());
    assert_eq!(run(&mut off, 0..60), reference);

    // Clearing mid-stream returns to the no-profile conditioning (state kept: compare deltas).
    let mut c = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    c.apply_profile(&v2_profile(1, None), 100).expect("profile");
    let _ = run(&mut c, 0..10);
    c.clear_profile().expect("clear");
    assert!(!c.core().speaker_active() && !c.core().eq_active());
}

#[test]
fn v3_profile_eq_is_applied_and_a_v1_profile_is_outdated_and_neutral() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let mut flat = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    let reference = run(&mut flat, 0..40);
    let gains = BandGains::from_array([-6.0; 32]).expect("gains");
    let mut eq = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    eq.apply_profile(&v2_profile(1, Some(gains.clone())), 0)
        .expect("profile");
    assert!(eq.core().eq_active() && !eq.core().speaker_active());
    let out = run(&mut eq, 0..40);
    let g = 10f32.powf(-6.0 / 20.0);
    let worst = reference
        .iter()
        .zip(&out)
        .map(|(r, o)| (g * r - o).abs())
        .fold(0.0f32, f32::max);
    assert!(
        worst < 1e-5,
        "flat -6 dB profile EQ is a pure gain ({worst})"
    );

    let mut v1 = ClearcoreFilter::new(&repo_root(), None).expect("filter");
    v1.apply_profile(&v2_profile(3, Some(gains.clone())), 100)
        .expect("v2 first");
    let profile = v1_conditioned("spk-v1").with_eq(Some(gains)).expect("eq");
    assert_eq!(
        v1.apply_profile(&StoredProfile::V1(Box::new(profile)), 100)
            .expect("v1 on v3"),
        ProfileOutcome::Outdated
    );
    assert!(
        !v1.core().eq_active() && !v1.core().speaker_active(),
        "outdated = neutral voice"
    );
    assert_eq!(
        run(&mut v1, 0..40).len(),
        40 * 480,
        "the filter keeps running"
    );
}

#[test]
fn reload_reads_v2_from_disk_honours_settings_and_rejects_bad_files() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let filter = create_ffi();
    assert!(!filter.is_null());
    let path = env.profiles_dir().join(ACTIVE_PROFILE_FILE_NAME);
    let StoredProfile::V2(profile) = v2_profile(5, None) else {
        unreachable!()
    };
    let write = |json: &str| crate::profile_v2::write_secure(&path, json).expect("write");
    unsafe {
        assert_eq!(clearcore_filter_reload_active_profile(filter), 0);
        write(&profile.to_json().expect("json"));
        // Off by default: without the user's opt-in in settings.json the stored profile is ignored.
        assert_eq!(clearcore_filter_reload_active_profile(filter), 0);
        assert!(!(*filter).core().speaker_active());
        let mut settings = Settings::default();
        assert!(!settings.voice_isolation_enabled);
        settings.voice_isolation_enabled = true;
        settings.save(&default_settings_path()).expect("settings");
        assert_eq!(clearcore_filter_reload_active_profile(filter), 1);
        assert!((*filter).core().speaker_active());

        settings.voice_isolation_enabled = false;
        settings.save(&default_settings_path()).expect("settings");
        assert_eq!(clearcore_filter_reload_active_profile(filter), 0);
        assert!(!(*filter).core().speaker_active());
        settings.voice_isolation_enabled = true;
        settings.save(&default_settings_path()).expect("settings");

        // A v1 file is outdated for the v3 model: neutral voice, rc 0, the filter keeps going.
        write(&v1_conditioned("old").to_json().expect("json"));
        assert_eq!(clearcore_filter_reload_active_profile(filter), 0);
        assert_eq!(clearcore_filter_is_voice_profile_active(filter), 0);
        let v1_json = CString::new(v1_conditioned("old").to_json().expect("json")).expect("c");
        assert_eq!(
            clearcore_filter_set_voice_profile(filter, v1_json.as_ptr()),
            -3
        );

        write("{ broken");
        assert_eq!(clearcore_filter_reload_active_profile(filter), -2);
        std::fs::remove_file(&path).expect("rm");
        assert_eq!(clearcore_filter_reload_active_profile(filter), 0);
        clearcore_filter_free(filter);
    }
}

#[test]
fn a_tampered_or_partial_v3_override_fails_closed() {
    let env = EnvGuard::new();
    env.set(ENV_DEV_V3_ASSET, synthetic_path());
    assert!(create_ffi().is_null(), "asset without SHA");
    env.set(ENV_DEV_V3_SHA256, "0".repeat(64));
    assert!(create_ffi().is_null(), "wrong SHA");
}

#[test]
fn an_engine_error_moves_to_the_standby_with_the_state_carried_over() {
    struct Broken;
    impl Engine for Broken {
        fn label(&self) -> String {
            "broken".to_owned()
        }
        fn run(
            &mut self,
            _: &realtime_noise_model::ExtractedFeatures,
        ) -> Result<realtime_noise_model::ModelFrameOutput, String> {
            Err("device lost".to_owned())
        }
        fn reset(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn set_conditioning(&mut self, _: &Conditioning) -> Result<(), String> {
            Ok(())
        }
        fn supports_speaker(&self) -> bool {
            true
        }
    }
    let asset = synthetic_asset();
    let descriptor = realtime_noise_model::BackendDescriptor {
        backend: "t",
        backend_version: "1",
        runtime: "t",
        runtime_version: "1",
        asset_id: "t".to_owned(),
        asset_sha256: String::new(),
        cpu_profile: "any",
    };
    let standby = crate::v3::v3_ort_engine(&asset).expect("ort");
    let mut engine = FallbackEngine::new(
        descriptor,
        Box::new(Broken),
        Some(Box::new(standby)),
        LatencyPolicy::from_env(),
    );
    let features = crate::engine::zero_features();
    assert!(
        engine.process_spectrum(&features).is_ok(),
        "the failed hop is re-run on the standby"
    );
    assert_eq!(engine.label(), "ort:CPU");
    assert!(
        engine
            .fallback_reason()
            .is_some_and(|r| r.contains("device lost"))
    );
    assert!(!engine.has_standby());
}

// ------------------------------------------------------------------ OpenVINO ------------

/// OpenVINO CPU (fp32) must reproduce ONNX Runtime on the same graphs; a forced swap to the ORT
/// standby carries the v3 state over with no discontinuity.
#[test]
fn v3_openvino_cpu_matches_onnx_runtime_and_swaps_seamlessly() {
    let env = EnvGuard::new();
    if !ov_available() {
        eprintln!("skipped: OpenVINO runtime not found (set OPENVINO_LIB_PATH)");
        return;
    }
    env.use_synthetic_v3();
    let mut ort = ClearcoreFilter::new(&repo_root(), None).expect("ort");
    let mut ov = ClearcoreFilter::new(&repo_root(), Some("openvino-cpu")).expect("ov");
    assert_eq!(
        ov.core.engine.label(),
        "openvino:CPU",
        "{}",
        ov.status_json()
    );
    assert!(ov.core.engine.has_standby());
    for f in [&mut ort, &mut ov] {
        f.apply_profile(&v2_profile(9, None), 100).expect("profile");
    }
    let a = run(&mut ort, 0..50);
    let b = run(&mut ov, 0..50);
    let snr = snr_db(&a, &b);
    assert!(snr > 60.0, "OpenVINO CPU vs ORT: {snr:.1} dB");
    assert!(ov.core.engine.force_fallback("test"));
    assert_eq!(ov.core.engine.label(), "ort:CPU");
    let a2 = run(&mut ort, 50..100);
    let b2 = run(&mut ov, 50..100);
    let snr2 = snr_db(&a2, &b2);
    assert!(
        snr2 > 60.0,
        "after the swap: {snr2:.1} dB (state not carried?)"
    );
    assert!(
        ov.core().speaker_active(),
        "conditioning carried to the standby"
    );
}

#[test]
fn an_unavailable_openvino_device_falls_back_to_onnx_runtime_with_a_reason() {
    let env = EnvGuard::new();
    env.use_synthetic_v3();
    let source = ModelSource::V3(synthetic_asset());
    let engine = crate::select::openvino_engine(&source, "NOSUCHDEVICE");
    assert!(engine.is_err());
    if !ov_devices().iter().any(|d| d.starts_with("NPU")) {
        let filter = ClearcoreFilter::new(&repo_root(), Some("openvino-npu")).expect("filter");
        // NPU absent: GPU or CPU or ORT, never a failure.
        assert!(!filter.core.engine.label().is_empty());
    }
}

/// Latency per hop of the selected engine (no profile and with profile) — run with `--ignored
/// --nocapture`; the real asset when `CLEARCORE_V3_BENCH_ASSET`/`_SHA256` are set.
#[test]
#[ignore = "benchmark"]
fn bench_latency_per_hop() {
    let env = EnvGuard::new();
    match (
        std::env::var_os("CLEARCORE_V3_BENCH_ASSET"),
        std::env::var_os("CLEARCORE_V3_BENCH_SHA256"),
    ) {
        (Some(a), Some(s)) => {
            env.set(ENV_DEV_V3_ASSET, a);
            env.set(ENV_DEV_V3_SHA256, s);
        }
        _ => env.use_synthetic_v3(),
    }
    let mut backends = vec![None];
    if ov_available() {
        for d in ov_devices() {
            match realtime_noise_runtime_openvino::DeviceKind::from_device_name(&d) {
                realtime_noise_runtime_openvino::DeviceKind::Npu => {
                    backends.push(Some("openvino-npu"))
                }
                realtime_noise_runtime_openvino::DeviceKind::Gpu => {
                    backends.push(Some("openvino-gpu"))
                }
                realtime_noise_runtime_openvino::DeviceKind::Cpu => {
                    backends.push(Some("openvino-cpu"))
                }
                realtime_noise_runtime_openvino::DeviceKind::Other => {}
            }
        }
        backends.dedup();
    }
    for backend in backends {
        let mut filter = match ClearcoreFilter::new(&repo_root(), backend) {
            Ok(f) => f,
            Err(e) => {
                println!("{backend:?}: {e}");
                continue;
            }
        };
        for (label, profile) in [
            ("sem perfil", None),
            ("com perfil", Some(v2_profile(4, None))),
        ] {
            if let Some(p) = &profile {
                filter.apply_profile(p, 100).expect("profile");
            }
            let _ = run(&mut filter, 0..50);
            let mut samples = Vec::new();
            for i in 50..1050 {
                let frame = voiced_frame(i);
                let start = std::time::Instant::now();
                filter.process(&frame).expect("hop");
                samples.push(u32::try_from(start.elapsed().as_micros()).unwrap_or(u32::MAX));
            }
            let s = latency_stats(&samples, samples.len() as u64);
            println!(
                "{:<14} {:<16} {label:<11} p50 {:>5} us  p99 {:>5} us  max {:>6} us  fallback={:?}",
                format!("{backend:?}"),
                filter.core.engine.label(),
                s.p50_us,
                s.p99_us,
                s.max_us,
                filter.core.engine.fallback_reason()
            );
        }
    }
}

/// Same model on every runtime (with and without a profile): SNR of each OpenVINO device against
/// ONNX Runtime CPU over 400 hops. Real asset via `CLEARCORE_V3_BENCH_ASSET`/`_SHA256`.
#[test]
#[ignore = "needs OpenVINO; real asset optional"]
fn runtimes_agree_on_the_v3_model() {
    let env = EnvGuard::new();
    match (
        std::env::var_os("CLEARCORE_V3_BENCH_ASSET"),
        std::env::var_os("CLEARCORE_V3_BENCH_SHA256"),
    ) {
        (Some(a), Some(s)) => {
            env.set(ENV_DEV_V3_ASSET, a);
            env.set(ENV_DEV_V3_SHA256, s);
        }
        _ => env.use_synthetic_v3(),
    }
    let source = select::resolve_model(&repo_root()).expect("model");
    // Raw model outputs (lsnr, mask, coefs) on real features: independent of the output gating,
    // which zeroes everything when the speaker does not match.
    let mut dsp =
        realtime_noise_model::DspPipeline::new(realtime_noise_model::DspPipelineConfig::default())
            .expect("dsp");
    let features: Vec<_> = (0..400)
        .map(|i| dsp.step_analysis(&voiced_frame(i)).expect("features"))
        .collect();
    for embedding in [None, Some(test_embedding(4))] {
        let raw = |backend: Option<&str>| -> Option<(String, Vec<f32>, Vec<f32>)> {
            let mut engine =
                select::build_engine(&source, backend, LatencyPolicy::from_env()).ok()?;
            engine
                .set_conditioning(Conditioning {
                    embedding: embedding.clone(),
                    film: None,
                })
                .expect("conditioning");
            let (mut flat, mut audio) = (Vec::new(), Vec::new());
            for f in &features {
                let o = engine.process_spectrum(f).expect("hop");
                flat.push(o.lsnr);
                flat.extend(o.erb_gains.expect("mask"));
                flat.extend(
                    o.df_coefs
                        .expect("coefs")
                        .iter()
                        .flat_map(|b| b.iter().flat_map(|c| [c.re, c.im])),
                );
            }
            let mut f = ClearcoreFilter::new(&repo_root(), backend).ok()?;
            if embedding.is_some() {
                f.apply_profile(&v2_profile(4, None), 100).expect("profile");
            }
            audio.extend(run(&mut f, 0..400));
            Some((engine.label(), flat, audio))
        };
        let (_, reference, ref_audio) = raw(None).expect("ort");
        let energy: f64 = ref_audio.iter().map(|v| f64::from(*v).powi(2)).sum();
        for backend in ["openvino-cpu", "openvino-gpu", "openvino-npu"] {
            if let Some((label, flat, audio)) = raw(Some(backend)) {
                println!(
                    "{backend:<13} -> {label:<13} {}: model outputs SNR {:.1} dB, audio SNR {:.1} dB (ref energy {energy:.3e})",
                    if embedding.is_some() {
                        "com perfil"
                    } else {
                        "sem perfil"
                    },
                    snr_db(&reference, &flat),
                    snr_db(&ref_audio[50 * 480..], &audio[50 * 480..]),
                );
            }
        }
    }
}

/// Parity against the daemon reference in `CLEARCORE_FILTER_PARITY_DIR` (format of the daemon
/// hand-off: `manifest.json`, `input.f32`, `expected_daemon_neutral.f32`,
/// `expected_daemon_profile.f32` raw LE f32 mono 48 kHz, `conditioning.json` with
/// `speaker_embedding` [192], `eq_gains_db` [32], `intensity_percent`). Both cases, on ONNX Runtime
/// and OpenVINO CPU (when found); SNR over the whole file >= 60 dB. The asset is the manifest's
/// `asset` next to the parity dir (`../pro-v3-full/`) unless `CLEARCORE_V3_BENCH_ASSET`/`_SHA256`.
#[test]
#[ignore = "needs the daemon parity vectors"]
fn parity_against_the_daemon_reference() {
    let env = EnvGuard::new();
    let dir = PathBuf::from(
        std::env::var_os("CLEARCORE_FILTER_PARITY_DIR").expect("set CLEARCORE_FILTER_PARITY_DIR"),
    );
    let json = |name: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(name)).expect(name)).expect(name)
    };
    let read_f32 = |name: &str| -> Vec<f32> {
        std::fs::read(dir.join(name))
            .expect(name)
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect()
    };
    let floats = |v: &serde_json::Value| -> Vec<f32> {
        #[allow(clippy::cast_possible_truncation)]
        v.as_array()
            .expect("array")
            .iter()
            .map(|x| x.as_f64().expect("number") as f32)
            .collect()
    };
    let manifest = json("manifest.json");
    let conditioning = json("conditioning.json");
    match (
        std::env::var_os("CLEARCORE_V3_BENCH_ASSET"),
        std::env::var_os("CLEARCORE_V3_BENCH_SHA256"),
    ) {
        (Some(a), Some(s)) => {
            env.set(ENV_DEV_V3_ASSET, a);
            env.set(ENV_DEV_V3_SHA256, s);
        }
        _ => {
            let asset = manifest["asset"].as_str().expect("asset");
            env.set(ENV_DEV_V3_ASSET, dir.join("../pro-v3-full").join(asset));
            env.set(
                ENV_DEV_V3_SHA256,
                manifest["asset_sha256"].as_str().expect("sha"),
            );
        }
    }
    let input = read_f32("input.f32");
    let embedding = floats(&conditioning["speaker_embedding"]);
    let eq = BandGains::from_slice(&floats(&conditioning["eq_gains_db"])).expect("eq");
    let intensity = conditioning["intensity_percent"].as_u64().unwrap_or(100);
    let mut backends = vec![None];
    if ov_devices().iter().any(|d| d == "CPU") {
        backends.push(Some("openvino-cpu"));
    }
    let mut failures = Vec::new();
    for backend in backends {
        for (case, expected_file) in [
            ("neutral", "expected_daemon_neutral.f32"),
            ("profile", "expected_daemon_profile.f32"),
        ] {
            let mut filter = ClearcoreFilter::new(&repo_root(), backend).expect("filter");
            assert_eq!(filter.model_kind(), ModelKind::V3Full);
            if case == "profile" {
                filter
                    .core
                    .set_conditioning(Conditioning {
                        embedding: (intensity > 0).then(|| embedding.clone()),
                        film: None,
                    })
                    .expect("conditioning");
                filter.core.set_eq(Some(eq.clone())).expect("eq");
            }
            let mut out = Vec::with_capacity(input.len());
            for hop in input.chunks_exact(480) {
                let mut frame = [0.0f32; 480];
                frame.copy_from_slice(hop);
                out.extend(filter.core.process(&frame).expect("hop"));
            }
            let expected = read_f32(expected_file);
            assert_eq!(expected.len(), out.len(), "{expected_file} length");
            let snr = snr_db(&expected, &out);
            let worst = expected
                .iter()
                .zip(&out)
                .map(|(a, b)| (a - b).abs())
                .fold(0.0f32, f32::max);
            println!(
                "parity {:<8} {case:<8}: SNR {snr:.2} dB, max |err| {worst:.2e}, silent hops {}",
                filter.core.engine.label(),
                filter.core.silent_hops()
            );
            if snr < 60.0 {
                failures.push(format!(
                    "{} {case}: {snr:.2} dB",
                    filter.core.engine.label()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "parity below 60 dB: {failures:?}");
}

// ------------------------------------------------------------------ v2 (bundled) --------

#[test]
fn without_any_environment_a_pdfnet3_pro_model_loads() {
    let _env = EnvGuard::new();
    let filter = create_ffi();
    assert!(
        !filter.is_null(),
        "the bundled Pro v3 full must load"
    );
    unsafe {
        assert_eq!(clearcore_filter_supports_voice_profile(filter), 1);
        clearcore_filter_free(filter);
    }
}

#[test]
fn a_disabled_model_yields_a_null_filter_without_fallback() {
    let env = EnvGuard::new();
    env.set(ENV_DEV_STATEFUL_ASSET, "/nonexistent/clearcore-disabled");
    assert!(create_ffi().is_null());
}

/// The runtime comes from the `backend` of `settings.json` (what the UI writes), and
/// `CLEARCORE_FILTER_BACKEND` overrides it.
#[test]
fn the_backend_setting_selects_openvino_and_the_env_overrides_it() {
    let env = EnvGuard::new();
    if !ov_devices().iter().any(|d| d == "CPU") {
        eprintln!("skipped: OpenVINO CPU not available (set OPENVINO_LIB_PATH)");
        return;
    }
    env.use_synthetic_v3();
    let settings = Settings {
        backend: Some("openvino-cpu".to_owned()),
        ..Settings::default()
    };
    settings.save(&default_settings_path()).expect("settings");
    let filter = create_ffi();
    assert!(!filter.is_null());
    unsafe {
        assert_eq!((*filter).core.engine.label(), "openvino:CPU");
        clearcore_filter_free(filter);
    }
    env.set(ENV_FILTER_BACKEND, "ort");
    let filter = create_ffi();
    unsafe {
        assert_eq!((*filter).core.engine.label(), "ort:CPU");
        clearcore_filter_free(filter);
    }
}
