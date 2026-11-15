//! Parity of the pDFNet3 Pro v7 streaming runtime (`pdfnet3_v7`) against the exporter's vectors
//! (`tests/fixtures/pdfnet3_v7/<case>/`, Common Voice pt CC0 + synthetic pink noise, no user
//! voice), on tract and on ONNX Runtime, with and without the gate C.
//!
//! Tolerances (justified by `tests/fixtures/pdfnet3_v7/parity.json` and the exporter's ORT x
//! tract per-call replay):
//! - the reference (PyTorch offline x ORT streaming) already differs by up to 2.0e-5 dB in lsnr,
//!   5.4e-7 in q, 0 in the gate and 2.7e-8 in the audio (SNR >= 126 dB); ORT x tract differ by up
//!   to 6.2e-6 per call. Through 500 recurrent frames we accept `|lsnr| <= 1e-3 dB`,
//!   `|q| <= 1e-3`, `|gate| <= 1e-3` (the exporter's suggested target) and an audio SNR
//!   >= 60 dB against `expected_out_*.wav`;
//! - enrollment features are f16-rounded: identical inputs up to one f16 ulp (`<= 2e-3`
//!   relative) and the same activity; tokens within 1e-4 of `tokens.f32`.

#![cfg(any(feature = "tract", feature = "ort"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::print_stdout
)]

use std::path::{Path, PathBuf};

use realtime_noise_model::GraphSession;
use realtime_noise_model::InferenceError;
use realtime_noise_model::pdfnet3_v7::{POST_DELAY, V7Asset, enrollment_inputs};

const CASES: [&str; 3] = ["alvo_interf_sir0_snr10", "alvo_snr0", "so_interf_snr20"];
const HOP: usize = 480;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn case_dir(case: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdfnet3_v7")
        .join(case)
}

fn read_f32(path: &Path) -> Vec<f32> {
    std::fs::read(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// Mono IEEE float WAV (any chunk layout).
fn read_wav_f32(path: &Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    assert_eq!(&bytes[..4], b"RIFF");
    let mut at = 12;
    let mut format = 0u16;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = u32::from_le_bytes(bytes[at + 4..at + 8].try_into().unwrap()) as usize;
        let body = &bytes[at + 8..at + 8 + size];
        if id == b"fmt " {
            format = u16::from_le_bytes([body[0], body[1]]);
            assert_eq!(u16::from_le_bytes([body[2], body[3]]), 1, "mono");
            assert_eq!(u32::from_le_bytes(body[4..8].try_into().unwrap()), 48_000);
        }
        if id == b"data" {
            assert!(format == 3 || format == 0xfffe, "float WAV");
            return body
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
        }
        at += 8 + size + (size & 1);
    }
    panic!("{}: no data chunk", path.display());
}

fn asset() -> V7Asset {
    let bytes = std::fs::read(
        repo().join("models/pdfnet3/pdfnet3-pro-v7-a2-300k-stateful-asset-v1.tar.gz"),
    )
    .unwrap();
    V7Asset::from_archive_bytes(&bytes).unwrap()
}

fn max_abs(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0, f32::max)
}

fn snr_db(reference: &[f32], test: &[f32]) -> f64 {
    let signal: f64 = reference.iter().map(|v| f64::from(*v).powi(2)).sum();
    let noise: f64 = reference
        .iter()
        .zip(test)
        .map(|(r, t)| (f64::from(*r) - f64::from(*t)).powi(2))
        .sum();
    10.0 * (signal / noise.max(1e-30)).log10()
}

struct Worst {
    lsnr: f32,
    q: f32,
    gate: f32,
    audio_abs: f32,
    snr: f64,
}

fn run_stream<S, F>(asset: &V7Asset, load: F, case: &str, gate: bool) -> Worst
where
    S: GraphSession,
    F: FnMut(&str, &[u8]) -> Result<S, InferenceError>,
{
    let dir = case_dir(case);
    let tag = if gate { "gateC" } else { "nogate" };
    let mut stream = asset.stream(load).unwrap();
    stream
        .set_profile(
            &read_f32(&dir.join("speaker_embedding.f32")),
            &read_f32(&dir.join("tokens.f32")),
        )
        .unwrap();
    stream.set_gate(gate);
    stream.record_trace();
    let input = read_wav_f32(&dir.join("input.wav"));
    let mut output = Vec::with_capacity(input.len() + POST_DELAY * HOP);
    for chunk in input.chunks_exact(HOP) {
        let frame: [f32; HOP] = chunk.try_into().unwrap();
        output.extend_from_slice(&stream.process(&frame).unwrap());
    }
    let trace = stream.trace().to_vec();
    for frame in stream.flush().unwrap() {
        output.extend_from_slice(&frame);
    }
    let lsnr: Vec<f32> = trace.iter().map(|t| t.lsnr).collect();
    let q: Vec<f32> = trace.iter().map(|t| t.q).collect();
    let gain: Vec<f32> = trace.iter().map(|t| t.gain).collect();
    let expected_lsnr = read_f32(&dir.join(format!("lsnr_{tag}.f32")));
    let expected_q = read_f32(&dir.join(format!("q_{tag}.f32")));
    // Traces cover the frames emitted before the flush; the flush emits the rest.
    let n = lsnr.len();
    assert_eq!(n + POST_DELAY, expected_lsnr.len(), "{case}: output frames");
    let expected = read_wav_f32(&dir.join(format!("expected_out_{tag}.wav")));
    let aligned = &output[POST_DELAY * HOP..];
    assert_eq!(aligned.len(), expected.len(), "{case}: output length");
    let gate_err = if gate {
        max_abs(&gain, &read_f32(&dir.join("gate_gain.f32"))[..n])
    } else {
        assert!(gain.iter().all(|g| *g == 1.0));
        0.0
    };
    Worst {
        lsnr: max_abs(&lsnr, &expected_lsnr[..n]),
        q: max_abs(&q, &expected_q[..n]),
        gate: gate_err,
        audio_abs: max_abs(aligned, &expected),
        snr: snr_db(&expected, aligned),
    }
}

fn check_backend<S, F>(backend: &str, mut load: F)
where
    S: GraphSession,
    F: FnMut(&str, &[u8]) -> Result<S, InferenceError>,
{
    let asset = asset();
    let mut enroll = asset.enroll_model(&mut load).unwrap();
    for case in CASES {
        let dir = case_dir(case);
        let inputs = enrollment_inputs(&read_wav_f32(&dir.join("enroll.wav")));
        let erb = max_abs(&inputs.feat_erb, &read_f32(&dir.join("enr_feat_erb.f32")));
        let spec = max_abs(&inputs.feat_spec, &read_f32(&dir.join("enr_feat_spec.f32")));
        assert_eq!(
            inputs.act,
            read_f32(&dir.join("enr_act.f32")),
            "{case}: act"
        );
        let embedding = read_f32(&dir.join("speaker_embedding.f32"));
        let tokens = enroll.tokens_from_inputs(&inputs, &embedding).unwrap();
        let tok = max_abs(&tokens, &read_f32(&dir.join("tokens.f32")));
        println!(
            "[{backend}] {case} enroll: starts={:?} |feat_erb|={erb:.3e} |feat_spec|={spec:.3e} |tokens|={tok:.3e}",
            inputs.starts
        );
        assert!(erb <= 2e-3 && spec <= 2e-3, "{case}: enrollment features");
        assert!(tok <= 1e-4, "{case}: tokens {tok}");
        for gate in [false, true] {
            let w = run_stream(&asset, &mut load, case, gate);
            println!(
                "[{backend}] {case} gate={gate}: |lsnr|={:.3e} dB |q|={:.3e} |gate|={:.3e} |audio|={:.3e} SNR={:.1} dB",
                w.lsnr, w.q, w.gate, w.audio_abs, w.snr
            );
            assert!(w.lsnr <= 1e-3, "{case}: lsnr {}", w.lsnr);
            assert!(w.q <= 1e-3, "{case}: q {}", w.q);
            assert!(w.gate <= 1e-3, "{case}: gate {}", w.gate);
            assert!(w.snr >= 60.0, "{case}: audio SNR {}", w.snr);
        }
    }
}

#[cfg(feature = "tract")]
#[test]
fn v7_parity_on_tract() {
    check_backend("tract", |name: &str, bytes: &[u8]| {
        realtime_noise_model::tract_graph::TractGraphSession::load(name, bytes)
    });
}

#[cfg(feature = "ort")]
#[test]
fn v7_parity_on_onnx_runtime() {
    check_backend("ort", |name: &str, bytes: &[u8]| {
        realtime_noise_model::ort_graph::OrtGraphSession::load(name, bytes)
    });
}

/// Real streaming cost: hop by hop, one thread, the 3 vector inputs (15 s of audio), after a
/// warm-up of 50 hops. `cargo test --release ... -- --ignored v7_streaming_cost --nocapture`.
fn streaming_cost<S, F>(backend: &str, load: F) -> f64
where
    S: GraphSession,
    F: FnMut(&str, &[u8]) -> Result<S, InferenceError>,
{
    let asset = asset();
    let mut stream = asset.stream(load).unwrap();
    let dir = case_dir(CASES[0]);
    stream
        .set_profile(
            &read_f32(&dir.join("speaker_embedding.f32")),
            &read_f32(&dir.join("tokens.f32")),
        )
        .unwrap();
    let mut audio = Vec::new();
    for case in CASES {
        audio.extend(read_wav_f32(&case_dir(case).join("input.wav")));
    }
    let frames: Vec<[f32; HOP]> = audio
        .chunks_exact(HOP)
        .map(|c| c.try_into().unwrap())
        .collect();
    for frame in &frames[..50] {
        stream.process(frame).unwrap();
    }
    stream.reset();
    let mut worst = std::time::Duration::ZERO;
    let start = std::time::Instant::now();
    for frame in &frames {
        let t = std::time::Instant::now();
        std::hint::black_box(stream.process(frame).unwrap());
        worst = worst.max(t.elapsed());
    }
    let elapsed = start.elapsed().as_secs_f64();
    #[allow(clippy::cast_precision_loss)]
    let seconds = audio.len() as f64 / 48_000.0;
    let ms_per_s = elapsed * 1000.0 / seconds;
    println!(
        "[{backend}] v7 streaming, 1 thread: {ms_per_s:.1} ms per s of audio ({} hops, {seconds:.1} s, worst hop {:.2} ms)",
        frames.len(),
        worst.as_secs_f64() * 1000.0
    );
    ms_per_s
}

#[test]
#[ignore = "benchmark: run in release with --ignored"]
fn v7_streaming_cost() {
    #[cfg(feature = "tract")]
    {
        let cost = streaming_cost("tract", |name: &str, bytes: &[u8]| {
            realtime_noise_model::tract_graph::TractGraphSession::load(name, bytes)
        });
        assert!(cost < 500.0);
    }
    #[cfg(feature = "ort")]
    {
        let cost = streaming_cost("ort", |name: &str, bytes: &[u8]| {
            realtime_noise_model::ort_graph::OrtGraphSession::load_with_threads(name, bytes, 1)
        });
        assert!(cost < 500.0);
    }
}
