//! Streaming DeepFilterNet3 sessions against the real OpenVINO runtime and the stateful assets in
//! `models/stateful`. Every test skips gracefully when either is missing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_cmp)]

use std::path::{Path, PathBuf};

use realtime_noise_runtime_openvino::dfn3::{NB_DF, NB_ERB};
use realtime_noise_runtime_openvino::{
    Dfn3Output, Dfn3Session, OpenVinoCore, OpenVinoError, available_devices, fallback_order,
    is_openvino_available,
};

fn model_dir() -> Option<PathBuf> {
    if !is_openvino_available() {
        eprintln!("OpenVINO runtime not installed; skipping");
        return None;
    }
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../models/stateful");
    if dir.join("enc.onnx").exists() {
        Some(dir)
    } else {
        eprintln!("models/stateful not present; skipping");
        None
    }
}

/// Deterministic pseudo-features (xorshift) in `[-0.5, 0.5]`.
struct Features(u32);

impl Features {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) - 0.5
    }

    fn frame(&mut self) -> ([f32; NB_ERB], [[f32; NB_DF]; 2]) {
        let mut erb = [0.0; NB_ERB];
        erb.iter_mut().for_each(|v| *v = self.next());
        let mut spec = [[0.0; NB_DF]; 2];
        spec.iter_mut().flatten().for_each(|v| *v = self.next());
        (erb, spec)
    }
}

fn run_sequence(session: &mut Dfn3Session, frames: usize) -> Vec<Dfn3Output> {
    let mut features = Features(0x1234_5678);
    let mut out = Dfn3Output::new();
    (0..frames)
        .map(|_| {
            let (erb, spec) = features.frame();
            session.run_frame(&erb, &spec, &mut out).unwrap();
            out.clone()
        })
        .collect()
}

fn max_lsnr_diff(a: &[Dfn3Output], b: &[Dfn3Output]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x.lsnr - y.lsnr).abs())
        .fold(0.0, f32::max)
}

#[test]
fn streaming_encoder_matches_batched_execution_on_cpu() {
    let Some(dir) = model_dir() else {
        return;
    };
    const T: usize = 8;
    let mut session = Dfn3Session::load(&dir, "CPU").unwrap();
    let streamed = run_sequence(&mut session, T);

    // Reference: the same encoder graph executed once over T frames with zero initial state.
    let core = OpenVinoCore::new().unwrap();
    let mut model = core
        .read_model_from_file(dir.join("enc.onnx"), None::<&str>)
        .unwrap();
    model
        .reshape_input("feat_erb", &[1, 1, T as i64, 32])
        .unwrap();
    model
        .reshape_input("feat_spec", &[1, 2, T as i64, 96])
        .unwrap();
    let compiled = core
        .compile_model_with_properties(&model, "CPU", &[("INFERENCE_PRECISION_HINT", "f32")])
        .unwrap();
    let mut request = compiled.create_infer_request().unwrap();

    let mut features = Features(0x1234_5678);
    let mut erb = vec![0.0f32; T * NB_ERB];
    let mut spec = vec![0.0f32; 2 * T * NB_DF];
    for t in 0..T {
        let (e, s) = features.frame();
        erb[t * NB_ERB..(t + 1) * NB_ERB].copy_from_slice(&e);
        // layout [1, 2, T, 96]: channel-major
        spec[t * NB_DF..(t + 1) * NB_DF].copy_from_slice(&s[0]);
        spec[(T + t) * NB_DF..(T + t + 1) * NB_DF].copy_from_slice(&s[1]);
    }
    let lib = core.library().clone();
    let t_erb = realtime_noise_runtime_openvino::OpenVinoTensor::from_slice_f32(
        &[1, 1, T as i64, 32],
        &erb,
        lib.clone(),
    )
    .unwrap();
    let t_spec = realtime_noise_runtime_openvino::OpenVinoTensor::from_slice_f32(
        &[1, 2, T as i64, 96],
        &spec,
        lib.clone(),
    )
    .unwrap();
    let zero = |shape: &[i64]| {
        let n: i64 = shape.iter().product();
        realtime_noise_runtime_openvino::OpenVinoTensor::from_slice_f32(
            shape,
            &vec![0.0; n as usize],
            lib.clone(),
        )
        .unwrap()
    };
    let (h, erb_buf, spec_buf) = (
        zero(&[1, 1, 256]),
        zero(&[1, 1, 2, 32]),
        zero(&[1, 2, 2, 96]),
    );
    request.set_tensor("feat_erb", &t_erb).unwrap();
    request.set_tensor("feat_spec", &t_spec).unwrap();
    request.set_tensor("h_in", &h).unwrap();
    request.set_tensor("feat_erb_buf", &erb_buf).unwrap();
    request.set_tensor("feat_spec_buf", &spec_buf).unwrap();
    request.infer().unwrap();
    let lsnr_tensor = request.get_tensor("lsnr").unwrap();
    let batched = lsnr_tensor.as_slice_f32().unwrap();
    assert_eq!(batched.len(), T);

    for (t, out) in streamed.iter().enumerate() {
        let diff = (out.lsnr - batched[t]).abs();
        assert!(
            diff < 1e-3,
            "frame {t}: streamed lsnr {} vs batched {} (diff {diff})",
            out.lsnr,
            batched[t]
        );
    }
}

#[test]
fn reset_restores_the_initial_state_and_state_actually_matters() {
    let Some(dir) = model_dir() else {
        return;
    };
    let mut session = Dfn3Session::load(&dir, "CPU").unwrap();
    let first = run_sequence(&mut session, 12);
    assert_eq!(session.frames_processed(), 12);

    // Without reset the recurrent state carries over, so the same inputs give other outputs.
    let continued = run_sequence(&mut session, 12);
    assert!(
        max_lsnr_diff(&first, &continued) > 1e-3,
        "state had no effect"
    );

    session.reset().unwrap();
    assert_eq!(session.frames_processed(), 0);
    let replay = run_sequence(&mut session, 12);
    assert_eq!(max_lsnr_diff(&first, &replay), 0.0);
    for (a, b) in first.iter().zip(&replay) {
        assert_eq!(a.erb_mask, b.erb_mask);
        assert_eq!(a.df_coefs, b.df_coefs);
    }
}

#[test]
fn rejects_non_finite_features_without_touching_state() {
    let Some(dir) = model_dir() else {
        return;
    };
    let mut session = Dfn3Session::load(&dir, "CPU").unwrap();
    let mut out = Dfn3Output::new();
    let mut erb = [0.0f32; NB_ERB];
    erb[3] = f32::NAN;
    let err = session
        .run_frame(&erb, &[[0.0; NB_DF]; 2], &mut out)
        .unwrap_err();
    assert!(matches!(err, OpenVinoError::InvalidInput(_)));
    assert_eq!(session.frames_processed(), 0);
}

#[test]
fn every_available_device_runs_finite_and_close_to_cpu() {
    let Some(dir) = model_dir() else {
        return;
    };
    let mut cpu = Dfn3Session::load(&dir, "CPU").unwrap();
    let reference = run_sequence(&mut cpu, 24);

    for device in fallback_order(&available_devices()) {
        let mut session = match Dfn3Session::load(&dir, &device) {
            Ok(session) => session,
            Err(err) => {
                eprintln!("device {device}: not usable, skipping ({err})");
                continue;
            }
        };
        let outputs = run_sequence(&mut session, 24);
        for out in &outputs {
            assert!(out.lsnr.is_finite());
            assert!(out.erb_mask.iter().all(|v| v.is_finite()));
            assert!(out.df_coefs.iter().all(|v| v.is_finite()));
        }
        let diff = max_lsnr_diff(&reference, &outputs);
        eprintln!(
            "device {device}: precision {}, max |lsnr - cpu| over 24 hops = {diff:.4} dB",
            session.precision().as_str()
        );
        // FP16 on GPU/NPU is allowed to drift from the FP32 CPU reference, but not wildly.
        assert!(diff < 2.0, "{device} diverges from CPU by {diff} dB");
    }
}

#[test]
fn fallback_skips_unusable_devices_and_reports_failures() {
    let Some(dir) = model_dir() else {
        return;
    };
    let session =
        Dfn3Session::load_with_fallback(&dir, &["NO_SUCH_DEVICE".to_owned(), "CPU".to_owned()])
            .unwrap();
    assert_eq!(session.device(), "CPU");

    let err = Dfn3Session::load_with_fallback(&dir, &["NO_SUCH_DEVICE".to_owned()]).unwrap_err();
    assert!(matches!(&err, OpenVinoError::DeviceNotFound(m) if m.contains("NO_SUCH_DEVICE")));
    assert!(matches!(
        Dfn3Session::load_with_fallback(&dir, &[]),
        Err(OpenVinoError::DeviceNotFound(_))
    ));

    let missing = Dfn3Session::load(Path::new("/nonexistent/stateful"), "CPU").unwrap_err();
    assert!(
        matches!(missing, OpenVinoError::ModelReadFailed(_)),
        "{missing:?}"
    );
}
