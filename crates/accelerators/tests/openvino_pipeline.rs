#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::suboptimal_flops
)]

use std::path::PathBuf;

use realtime_noise_accelerators::{OpenVINOBackend, StatefulDigests};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::{
    ApprovedAssetManifest, CpuProfile, InferenceBackend, InferenceError, TractBackend,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// `models/stateful`, or `None` (with a note) when OpenVINO or the assets are missing.
fn stateful_dir() -> Option<PathBuf> {
    if !OpenVINOBackend::is_available() {
        eprintln!("OpenVINO runtime not installed; skipping");
        return None;
    }
    let dir = repo_root().join("models/stateful");
    if dir.join("enc.onnx").exists() {
        Some(dir)
    } else {
        eprintln!("models/stateful not present; skipping");
        None
    }
}

/// Deterministic noisy "voiced" signal: harmonic tone with a slow envelope plus white noise.
fn noisy_signal(frames: usize) -> Vec<AudioFrame> {
    let mut state = 0x9E37_79B9_u32;
    let mut next_noise = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state as f32 / u32::MAX as f32) - 0.5
    };
    (0..frames)
        .map(|frame| {
            let mut samples = [0.0f32; HOP_SAMPLES];
            for (i, sample) in samples.iter_mut().enumerate() {
                let n = (frame * HOP_SAMPLES + i) as f32;
                let t = n / 48_000.0;
                let envelope = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
                let tone = (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                    + 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                    + 0.25 * (2.0 * std::f32::consts::PI * 660.0 * t).sin();
                *sample = 0.25 * envelope * tone + 0.05 * next_noise();
            }
            samples
        })
        .collect()
}

fn run<B: InferenceBackend>(backend: &mut B, signal: &[AudioFrame]) -> Vec<AudioFrame> {
    signal
        .iter()
        .map(|frame| backend.process(frame).unwrap().samples)
        .collect()
}

fn rms(frames: &[AudioFrame]) -> f64 {
    let (sum, count) = frames.iter().flatten().fold((0.0f64, 0usize), |(s, c), v| {
        (s + f64::from(*v).powi(2), c + 1)
    });
    (sum / count.max(1) as f64).sqrt()
}

#[test]
fn stateful_backend_runs_real_inference_through_the_dsp_pipeline() {
    let Some(dir) = stateful_dir() else {
        return;
    };
    let mut backend =
        OpenVINOBackend::load_stateful(&dir, "CPU", "dfn3-stateful", "unverified").unwrap();
    assert!(backend.is_hardware_accelerated());
    assert_eq!(backend.device(), "CPU");
    assert_eq!(backend.descriptor().backend, "openvino");
    assert_eq!(backend.descriptor().runtime, "openvino-cpu");
    assert_eq!(backend.descriptor().cpu_profile, "intel-cpu");
    assert_eq!(backend.algorithmic_latency_samples(), 1_440);

    let signal = noisy_signal(120);
    let output = run(&mut backend, &signal);
    assert!(output.iter().flatten().all(|s| s.is_finite()));
    // A passthrough stub would return the input verbatim; the network must change the signal.
    assert_ne!(output, signal);

    // Reset restores the initial state: the same signal gives the same audio again.
    backend.reset().unwrap();
    let replay = run(&mut backend, &signal);
    assert_eq!(output, replay);
}

#[test]
fn stateful_backend_rejects_bad_input_and_honours_simulated_failure() {
    let Some(dir) = stateful_dir() else {
        return;
    };
    let mut backend =
        OpenVINOBackend::load_stateful(&dir, "CPU", "dfn3-stateful", "unverified").unwrap();
    let mut nan: AudioFrame = [0.0; HOP_SAMPLES];
    nan[7] = f32::NAN;
    assert!(matches!(
        backend.process(&nan),
        Err(InferenceError::InputContract(_))
    ));

    backend.set_simulated_failure(true);
    let silence: AudioFrame = [0.0; HOP_SAMPLES];
    assert!(matches!(
        backend.process(&silence),
        Err(InferenceError::InferenceExecution(_))
    ));
    backend.set_simulated_failure(false);
    assert!(backend.process(&silence).is_ok());
}

#[test]
fn load_stateful_reports_missing_assets_without_panicking() {
    if !OpenVINOBackend::is_available() {
        return;
    }
    let result = OpenVINOBackend::load_stateful("/nonexistent/stateful", "CPU", "x", "y");
    assert!(matches!(result, Err(InferenceError::ModelCorruption(_))));
}

#[test]
fn auto_load_walks_npu_gpu_cpu_and_matches_the_policy() {
    let Some(dir) = stateful_dir() else {
        return;
    };
    let devices = OpenVINOBackend::available_devices();
    assert!(devices.iter().any(|d| d == "CPU"));
    let rank = |name: &str| {
        ["NPU", "GPU", "CPU"]
            .iter()
            .position(|k| name.starts_with(k))
    };
    assert!(devices.windows(2).all(|w| rank(&w[0]) <= rank(&w[1])));

    let mut backend = OpenVINOBackend::load_stateful_auto(&dir, "dfn3", "unverified").unwrap();
    assert!(devices.contains(&backend.device().to_owned()));
    eprintln!(
        "auto-selected OpenVINO device {} ({})",
        backend.device(),
        backend.precision().as_str()
    );
    let signal = noisy_signal(30);
    assert!(
        run(&mut backend, &signal)
            .iter()
            .flatten()
            .all(|s| s.is_finite())
    );
}

#[test]
fn simulated_backends_remain_passthrough_and_say_so() {
    let mut stub = OpenVINOBackend::new_mock_gpu();
    assert!(!stub.is_hardware_accelerated());
    assert_eq!(stub.descriptor().runtime, "openvino-gpu");
    let input: AudioFrame = [0.2; HOP_SAMPLES];
    assert_eq!(stub.process(&input).unwrap().samples, input);
    // Device names with an index still resolve to the right runtime label.
    assert_eq!(
        OpenVINOBackend::new("a", "b", "GPU.1").descriptor().runtime,
        "openvino-gpu"
    );
}

/// Reads a 16-bit PCM mono 48 kHz WAV file (data chunk only).
fn read_pcm16_wav(path: &str) -> Option<Vec<f32>> {
    let bytes = std::fs::read(path).ok()?;
    let mut offset = 12;
    while offset + 8 <= bytes.len() {
        let size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().ok()?) as usize;
        if &bytes[offset..offset + 4] == b"data" {
            let end = (offset + 8 + size).min(bytes.len());
            return Some(
                bytes[offset + 8..end]
                    .chunks_exact(2)
                    .map(|c| f32::from(i16::from_le_bytes([c[0], c[1]])) / 32_768.0)
                    .collect(),
            );
        }
        offset += 8 + size;
    }
    None
}

/// Parity of the OpenVINO pipeline against the Tract reference on real speech.
///
/// Speech is needed because DeepFilterNet3 mutes non-speech (a synthetic tone is classified as
/// noise and gated to silence on one backend but not necessarily the other). Point
/// `CLEARCORE_PARITY_WAV` at any 16-bit mono 48 kHz speech recording to enable it.
#[test]
fn parity_against_the_tract_reference_on_cpu() {
    let Some(dir) = stateful_dir() else {
        return;
    };
    let Ok(wav_path) = std::env::var("CLEARCORE_PARITY_WAV") else {
        eprintln!("CLEARCORE_PARITY_WAV not set; skipping Tract parity");
        return;
    };
    let Some(speech) = read_pcm16_wav(&wav_path) else {
        eprintln!("cannot read {wav_path}; skipping Tract parity");
        return;
    };
    let Ok(manifest) = ApprovedAssetManifest::verify(&repo_root()) else {
        eprintln!("approved asset not available; skipping Tract parity");
        return;
    };
    let Ok(mut tract) = TractBackend::new(&manifest, CpuProfile::Avx2Minimum) else {
        eprintln!("Tract backend unavailable on this CPU; skipping parity");
        return;
    };
    let mut openvino =
        OpenVINOBackend::load_stateful(&dir, "CPU", "dfn3-stateful", "unverified").unwrap();

    // 400 hops (4 s) of speech plus deterministic white noise.
    let mut state = 12_345_u32;
    let mut noise = move || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state as f32 / u32::MAX as f32) - 0.5
    };
    let signal: Vec<AudioFrame> = speech
        .chunks_exact(HOP_SAMPLES)
        .skip(200)
        .take(400)
        .map(|chunk| std::array::from_fn(|i| chunk[i] + 0.06 * noise()))
        .collect();
    if signal.len() < 400 {
        eprintln!("recording too short for the parity window; skipping");
        return;
    }

    let reference = run(&mut tract, &signal);
    let candidate = run(&mut openvino, &signal);

    // Skip the warm-up, then compare error energy to reference energy at zero lag.
    let skip = 40;
    let error: Vec<AudioFrame> = reference[skip..]
        .iter()
        .zip(&candidate[skip..])
        .map(|(a, b)| std::array::from_fn(|i| a[i] - b[i]))
        .collect();
    let relative = rms(&error) / rms(&reference[skip..]).max(1e-9);
    eprintln!("OpenVINO(CPU) vs Tract relative RMS error: {relative:.4}");
    // Measured around 0.09 (-21 dB) on this model pair; 0.25 catches misalignment (lag +-1 hop
    // already gives ~0.18 and a full hop shift ~1.4) and gross numerical errors.
    assert!(relative < 0.25, "OpenVINO diverges from Tract: {relative}");
}

/// Directory with the repository graphs, whether or not OpenVINO is installed.
fn repo_stateful_dir() -> Option<PathBuf> {
    let dir = repo_root().join("models/stateful");
    dir.join("enc.onnx").exists().then_some(dir)
}

fn copy_stateful_to_scratch(source: &std::path::Path, tag: &str) -> PathBuf {
    let scratch =
        std::env::temp_dir().join(format!("clearcore-stateful-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    for file in ["enc.onnx", "erb_dec.onnx", "df_dec.onnx"] {
        std::fs::copy(source.join(file), scratch.join(file)).unwrap();
    }
    scratch
}

#[test]
fn load_stateful_fails_closed_on_a_tampered_graph() {
    let Some(source) = repo_stateful_dir() else {
        eprintln!("models/stateful not present; skipping");
        return;
    };
    let scratch = copy_stateful_to_scratch(&source, "tampered");
    let target = scratch.join("erb_dec.onnx");
    let mut bytes = std::fs::read(&target).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;
    std::fs::write(&target, bytes).unwrap();

    let result = OpenVINOBackend::load_stateful(&scratch, "CPU", "dfn3", "x");
    std::fs::remove_dir_all(&scratch).unwrap();

    match result {
        Err(InferenceError::AssetNotApproved(message)) => {
            assert!(message.contains("erb_dec.onnx"), "{message}");
            assert!(message.contains("SHA-256"), "{message}");
        }
        Err(other) => panic!("expected AssetNotApproved, got {other}"),
        Ok(_) => panic!("a tampered graph must not load"),
    }
}

#[test]
fn load_stateful_accepts_only_the_expected_digests() {
    let Some(source) = repo_stateful_dir() else {
        eprintln!("models/stateful not present; skipping");
        return;
    };
    // The repository graphs verify against the built-in digests (reaching the runtime stage
    // means verification passed: without OpenVINO the error is an execution error, not a
    // digest rejection).
    let result = OpenVINOBackend::load_stateful(&source, "CPU", "dfn3", "x");
    assert!(
        !matches!(result, Err(InferenceError::AssetNotApproved(_))),
        "approved graphs were rejected"
    );

    // A caller-supplied set that does not match is rejected, naming the first mismatching file.
    let wrong = StatefulDigests {
        enc: "0000000000000000000000000000000000000000000000000000000000000000",
        ..realtime_noise_accelerators::openvino::APPROVED_STATEFUL_DIGESTS
    };
    let result = OpenVINOBackend::load_stateful_verified(&source, "CPU", "dfn3", "x", &wrong);
    assert!(matches!(result, Err(InferenceError::AssetNotApproved(m)) if m.contains("enc.onnx")));
}

#[test]
fn approved_digests_match_the_repository_graphs() {
    let Some(source) = repo_stateful_dir() else {
        return;
    };
    for (file, expected) in [
        (
            "enc.onnx",
            realtime_noise_accelerators::openvino::APPROVED_STATEFUL_DIGESTS.enc,
        ),
        (
            "erb_dec.onnx",
            realtime_noise_accelerators::openvino::APPROVED_STATEFUL_DIGESTS.erb_dec,
        ),
        (
            "df_dec.onnx",
            realtime_noise_accelerators::openvino::APPROVED_STATEFUL_DIGESTS.df_dec,
        ),
    ] {
        let bytes = std::fs::read(source.join(file)).unwrap();
        assert_eq!(
            realtime_noise_accelerators::openvino::sha256_hex(&bytes),
            expected,
            "{file}"
        );
    }
}
