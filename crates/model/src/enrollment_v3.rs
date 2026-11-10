//! Enrollment for the pDFNet3 Pro v3: audio in, ECAPA speaker embedding out.
//!
//! The v3 `enrollment.onnx` takes `audio` (`f32`, `[1, N]`, 16 kHz mono, `N` in 96 000..=192 000,
//! i.e. 6-12 s) and returns `speaker_embedding` (`f32`, `[1, 192]`, L2-normalized).
//!
//! # Aggregation rule (identical to the training one, `handoff/enrollment-v3/manifest.json`)
//!
//! 1. every stored sample at 16 kHz, in recording order;
//! 2. concatenated with 250 ms of zeros (4 000 samples) between samples;
//! 3. truncated to 192 000 samples (12 s);
//! 4. fewer than 96 000 samples (6 s) is refused (`TooShort`: ask for more speech);
//! 5. the model runs ONCE on the result. No averaging of per-sample embeddings.
//!
//! Steps 1-3 live in [`aggregate_samples`], the single place to change if the rule changes.

use crate::enrollment::{
    ENROLLMENT_MAX_PEAK, ENROLLMENT_MIN_SAMPLES, ENROLLMENT_SAMPLE_RATE_HZ, EnrollmentAudio,
    EnrollmentError, ProfileMetadata, validate_enrollment_audio,
};
use crate::enrollment_builder::BuildError;
use crate::microphone_eq::{MicrophoneEqConfig, SpeechTargetCurve, estimate_microphone_eq};
use crate::resample::decimate_48k_to_16k;
use crate::voice_profile_v2::{SpeakerEmbedding, VoiceProfileV2};

/// Name of the ONNX output that carries the embedding.
pub const EMBEDDING_OUTPUT_NAME: &str = "speaker_embedding";
/// Zeros between two samples: 250 ms at 16 kHz.
pub const SAMPLE_GAP_SAMPLES_16K: usize = 4_000;
/// Longest audio the model accepts (12 s); anything beyond is dropped.
pub const MAX_ENROLLMENT_SAMPLES_16K: usize = 192_000;
/// Shortest audio the model accepts (6 s).
pub const MIN_ENROLLMENT_SAMPLES_16K: usize = ENROLLMENT_MIN_SAMPLES;
/// Longest joined speech (48 kHz) used to estimate the microphone EQ.
const MAX_EQ_SAMPLES: usize = 90 * 48_000;
const JOINED_SAMPLE_RATE_HZ: u32 = 48_000;
/// A 16 kHz copy above this peak (the decimation low-pass can overshoot the stored 48 kHz peak)
/// is scaled down to `SAFE_PEAK`; audio at or below it is used exactly as stored.
const SAFE_PEAK_TRIGGER: f32 = ENROLLMENT_MAX_PEAK;
const SAFE_PEAK: f32 = 0.97;

/// Speaker embedding extraction: 16 kHz mono audio in, the raw 192 floats out.
pub trait EmbeddingExtractor {
    fn extract_embedding(&mut self, samples_16khz: &[f32]) -> Result<Vec<f32>, EnrollmentError>;
}

/// THE aggregation rule: samples (16 kHz, recording order) joined with 250 ms of zeros between
/// them and truncated to 12 s. Pure; the result may be shorter than 6 s (the caller refuses it).
#[must_use]
pub fn aggregate_samples(samples_16k: &[Vec<f32>]) -> Vec<f32> {
    let mut out: Vec<f32> = Vec::new();
    for (index, sample) in samples_16k.iter().enumerate() {
        if index > 0 {
            out.resize(out.len() + SAMPLE_GAP_SAMPLES_16K, 0.0);
        }
        out.extend_from_slice(sample);
        if out.len() >= MAX_ENROLLMENT_SAMPLES_16K {
            break;
        }
    }
    out.truncate(MAX_ENROLLMENT_SAMPLES_16K);
    out
}

fn wipe(samples: &mut [f32]) {
    samples.fill(0.0);
    std::hint::black_box(samples);
}

/// Builds the sealed v2 profile.
///
/// - `joined_48k`: the group's trimmed, level-matched, crossfaded 48 kHz speech (at most 90 s),
///   used only for the microphone EQ.
/// - `stored_48k`: the stored samples exactly as they are on disk (clean mix, untrimmed), in
///   recording order; they are decimated to 16 kHz and aggregated by [`aggregate_samples`].
///
/// The aggregated audio goes through the enrollment audio gates (6-12 s, finite, peak, level,
/// speech fraction) and the model runs once. Every 16 kHz copy is zeroed whatever the outcome.
pub fn build_profile_v2<M: EmbeddingExtractor>(
    model: &mut M,
    joined_48k: &[f32],
    stored_48k: &[Vec<f32>],
    metadata: &ProfileMetadata,
) -> Result<VoiceProfileV2, BuildError> {
    if joined_48k.len() > MAX_EQ_SAMPLES {
        return Err(BuildError::TooMuchSpeech);
    }
    if metadata.id.trim().is_empty() {
        return Err(BuildError::Enroll(EnrollmentError::InvalidMetadata(
            "id is empty",
        )));
    }
    if metadata.name.trim().is_empty() {
        return Err(BuildError::Enroll(EnrollmentError::InvalidMetadata(
            "name is empty",
        )));
    }
    let eq = estimate_microphone_eq(
        joined_48k,
        JOINED_SAMPLE_RATE_HZ,
        &SpeechTargetCurve::default(),
        &MicrophoneEqConfig::default(),
    )
    .map_err(BuildError::Eq)?;

    let mut parts_16k: Vec<Vec<f32>> = stored_48k
        .iter()
        .map(|part| decimate_48k_to_16k(part))
        .collect();
    let mut audio = aggregate_samples(&parts_16k);
    for part in &mut parts_16k {
        wipe(part);
    }
    let outcome = embed(model, &mut audio);
    wipe(&mut audio);
    let embedding = outcome.map_err(BuildError::Enroll)?;
    VoiceProfileV2::new(
        metadata.id.clone(),
        metadata.name.clone(),
        metadata.created_at_utc.clone(),
        &embedding,
        Some(eq),
    )
    .map_err(|e| BuildError::Enroll(EnrollmentError::InvalidConditioning(e)))
}

fn embed<M: EmbeddingExtractor>(
    model: &mut M,
    audio: &mut [f32],
) -> Result<SpeakerEmbedding, EnrollmentError> {
    let peak = audio.iter().fold(0.0_f32, |acc, s| acc.max(s.abs()));
    if peak > SAFE_PEAK_TRIGGER && peak.is_finite() {
        let scale = SAFE_PEAK / peak;
        for v in audio.iter_mut() {
            *v *= scale;
        }
    }
    validate_enrollment_audio(&EnrollmentAudio {
        samples: audio,
        sample_rate: ENROLLMENT_SAMPLE_RATE_HZ,
    })?;
    let raw = model.extract_embedding(audio)?;
    SpeakerEmbedding::from_raw_normalizing(&raw).map_err(EnrollmentError::InvalidConditioning)
}

#[cfg(feature = "tract")]
pub use onnx::OnnxEmbeddingModel;

#[cfg(feature = "tract")]
mod onnx {
    use std::io::Cursor;

    use tract_onnx::prelude::*;

    use super::{EMBEDDING_OUTPUT_NAME, EmbeddingExtractor};
    use crate::enrollment::{EnrollmentError, OnnxEnrollmentModel};
    use crate::voice_profile_v2::SPEAKER_EMBEDDING_DIM;

    /// Runs the v3 `enrollment.onnx` through tract: input `audio` `[1, N]`, output
    /// `speaker_embedding` `[1, 192]`.
    pub struct OnnxEmbeddingModel {
        onnx_bytes: Vec<u8>,
    }

    impl OnnxEmbeddingModel {
        pub fn from_onnx_bytes(onnx_bytes: Vec<u8>) -> Result<Self, EnrollmentError> {
            // Parse once so a broken asset fails at load time, and require the v3 output: a v1
            // asset (gamma/beta outputs) is refused here instead of failing at the first take.
            let model = onnx()
                .model_for_read(&mut Cursor::new(&onnx_bytes))
                .map_err(|e| EnrollmentError::Model(e.to_string()))?;
            let outlets = model
                .output_outlets()
                .map_err(|e| EnrollmentError::Model(e.to_string()))?
                .to_vec();
            if !outlets
                .into_iter()
                .any(|o| model.outlet_label(o) == Some(EMBEDDING_OUTPUT_NAME))
            {
                return Err(EnrollmentError::Model(format!(
                    "model has no output {EMBEDDING_OUTPUT_NAME} (not a v3 enrollment asset)"
                )));
            }
            Ok(Self { onnx_bytes })
        }

        pub fn from_file(path: &std::path::Path) -> Result<Self, EnrollmentError> {
            Self::from_enrollment_model(OnnxEnrollmentModel::from_file(path)?)
        }

        /// Reuses the loaders of [`OnnxEnrollmentModel`] (verified asset, development archive,
        /// file): they only differ in how the `enrollment.onnx` bytes are obtained.
        pub fn from_enrollment_model(model: OnnxEnrollmentModel) -> Result<Self, EnrollmentError> {
            Self::from_onnx_bytes(model.into_onnx_bytes())
        }
    }

    impl EmbeddingExtractor for OnnxEmbeddingModel {
        fn extract_embedding(
            &mut self,
            samples_16khz: &[f32],
        ) -> Result<Vec<f32>, EnrollmentError> {
            let fail = |e: &dyn std::fmt::Display| EnrollmentError::Model(e.to_string());
            let mut model = onnx()
                .model_for_read(&mut Cursor::new(&self.onnx_bytes))
                .map_err(|e| fail(&e))?;
            let mut names = Vec::new();
            for outlet in model.output_outlets().map_err(|e| fail(&e))?.to_vec() {
                names.push(model.outlet_label(outlet).map(str::to_owned));
            }
            model
                .set_input_fact(
                    0,
                    InferenceFact::dt_shape(f32::datum_type(), tvec!(1, samples_16khz.len())),
                )
                .map_err(|e| fail(&e))?;
            let runnable = model
                .into_optimized()
                .map_err(|e| fail(&e))?
                .into_runnable()
                .map_err(|e| fail(&e))?;
            let input = Tensor::from_shape(&[1, samples_16khz.len()], samples_16khz)
                .map_err(|e| fail(&e))?;
            let outputs = runnable.run(tvec!(input.into())).map_err(|e| fail(&e))?;
            let position = names
                .iter()
                .position(|n| n.as_deref() == Some(EMBEDDING_OUTPUT_NAME))
                .ok_or_else(|| {
                    EnrollmentError::Model(format!("model has no output {EMBEDDING_OUTPUT_NAME}"))
                })?;
            let values = outputs
                .get(position)
                .ok_or_else(|| EnrollmentError::Model("model returned no embedding".to_owned()))?
                .as_slice::<f32>()
                .map_err(|e| fail(&e))?
                .to_vec();
            if values.len() != SPEAKER_EMBEDDING_DIM {
                return Err(EnrollmentError::Model(format!(
                    "embedding has {} values, expected {SPEAKER_EMBEDDING_DIM}",
                    values.len()
                )));
            }
            Ok(values)
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::suboptimal_flops
)]
mod tests {
    use super::*;
    use crate::voice_profile_v2::SPEAKER_EMBEDDING_DIM;
    use std::cell::RefCell;
    use std::rc::Rc;

    const RATE_48K: usize = 48_000;

    fn speech_48k(seconds: usize, f0: f32) -> Vec<f32> {
        (0..seconds * RATE_48K)
            .map(|n| {
                #[allow(clippy::cast_precision_loss)]
                let t = n as f32 / 48_000.0;
                let envelope = 0.6 + 0.4 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
                let tau = 2.0 * std::f32::consts::PI * f0 * t;
                0.3 * envelope
                    * (0.5 * tau.sin() + 0.3 * (2.0 * tau).sin() + 0.2 * (3.0 * tau).sin())
            })
            .collect()
    }

    fn metadata() -> ProfileMetadata {
        ProfileMetadata {
            id: "p-1".to_owned(),
            name: "Voz".to_owned(),
            created_at_utc: "2026-11-09T12:00:00Z".to_owned(),
        }
    }

    struct RecordingModel {
        seen: Rc<RefCell<Vec<Vec<f32>>>>,
    }

    impl EmbeddingExtractor for RecordingModel {
        fn extract_embedding(&mut self, samples: &[f32]) -> Result<Vec<f32>, EnrollmentError> {
            self.seen.borrow_mut().push(samples.to_vec());
            let mut v = vec![0.0; SPEAKER_EMBEDDING_DIM];
            v[5] = 3.0; // not unit: the pipeline normalizes
            Ok(v)
        }
    }

    fn model() -> (RecordingModel, Rc<RefCell<Vec<Vec<f32>>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        (
            RecordingModel {
                seen: Rc::clone(&seen),
            },
            seen,
        )
    }

    fn five_samples(seconds: usize) -> (Vec<f32>, Vec<Vec<f32>>) {
        let parts: Vec<Vec<f32>> = (0_u8..5)
            .map(|i| speech_48k(seconds, 140.0 + 10.0 * f32::from(i)))
            .collect();
        let refs: Vec<&[f32]> = parts.iter().map(Vec::as_slice).collect();
        (crate::speech_trim::join_crossfade(&refs, 48_000, 20), parts)
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact equality is the assertion: samples are copied verbatim
    fn aggregation_puts_250_ms_of_zeros_between_samples_in_order() {
        let samples = vec![vec![1.0_f32; 10], vec![2.0; 20], vec![3.0; 5]];
        let out = aggregate_samples(&samples);
        assert_eq!(out.len(), 10 + 4_000 + 20 + 4_000 + 5);
        assert!(out[..10].iter().all(|v| *v == 1.0));
        assert!(out[10..4_010].iter().all(|v| *v == 0.0));
        assert!(out[4_010..4_030].iter().all(|v| *v == 2.0));
        assert!(out[4_030..8_030].iter().all(|v| *v == 0.0));
        assert!(out[8_030..].iter().all(|v| *v == 3.0));
        // No trailing or leading gap, and a single sample is returned unchanged.
        assert_eq!(aggregate_samples(&[vec![0.5; 7]]), vec![0.5; 7]);
        assert!(aggregate_samples(&[]).is_empty());
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact equality is the assertion: samples are copied verbatim
    fn aggregation_truncates_to_192000_samples() {
        let samples = vec![
            vec![1.0_f32; 100_000],
            vec![2.0; 100_000],
            vec![3.0; 100_000],
        ];
        let out = aggregate_samples(&samples);
        assert_eq!(out.len(), MAX_ENROLLMENT_SAMPLES_16K);
        assert!(out[..100_000].iter().all(|v| *v == 1.0));
        assert_eq!(out[100_000], 0.0);
        assert!(out[104_000..].iter().all(|v| *v == 2.0));
        // The third sample never makes it in.
        assert!(!out.contains(&3.0));
    }

    #[test]
    fn the_model_runs_once_on_the_concatenation_with_gaps() {
        // Five 2 s samples: 5 * 32 000 + 4 * 4 000 = 176 000 samples (11 s), under the cap.
        let (joined, parts) = five_samples(2);
        let (mut m, seen) = model();
        let profile = build_profile_v2(&mut m, &joined, &parts, &metadata()).unwrap();
        let seen = seen.borrow();
        assert_eq!(seen.len(), 1, "one call, no per-sample averaging");
        assert_eq!(seen[0].len(), 5 * 32_000 + 4 * SAMPLE_GAP_SAMPLES_16K);
        // The gaps are real zeros between the samples.
        assert!(seen[0][32_000..36_000].iter().all(|v| *v == 0.0));
        let e = profile.speaker_embedding().unwrap();
        assert!((e.as_slice()[5] - 1.0).abs() < 1e-6);
        assert!(profile.eq.is_some());
        profile.verify_integrity().unwrap();
        assert_eq!(profile.model_id, "pdfnet3-pro-v3-full");
    }

    #[test]
    fn long_groups_are_truncated_to_twelve_seconds() {
        let (joined, parts) = five_samples(4); // 5 x 4 s + gaps = 21 s
        let (mut m, seen) = model();
        build_profile_v2(&mut m, &joined, &parts, &metadata()).unwrap();
        assert_eq!(seen.borrow()[0].len(), MAX_ENROLLMENT_SAMPLES_16K);
    }

    #[test]
    fn under_six_seconds_is_refused_before_the_model_runs() {
        // Four 1 s samples + 3 gaps = 64 000 + 12 000 = 76 000 samples (4.75 s).
        let (_, parts) = five_samples(1);
        let parts = &parts[..4];
        let joined: Vec<f32> = parts.concat();
        let total = 4 * 16_000 + 3 * SAMPLE_GAP_SAMPLES_16K;
        assert!(total < MIN_ENROLLMENT_SAMPLES_16K);
        let (mut m, seen) = model();
        let result = build_profile_v2(&mut m, &joined, parts, &metadata());
        assert!(
            matches!(
                result,
                Err(BuildError::Enroll(EnrollmentError::TooShort { samples })) if samples == total
            ),
            "{result:?}"
        );
        assert!(seen.borrow().is_empty());
    }

    #[test]
    fn exactly_six_seconds_is_accepted() {
        // One 6 s sample.
        let part = speech_48k(6, 150.0);
        let (mut m, seen) = model();
        build_profile_v2(&mut m, &part, std::slice::from_ref(&part), &metadata()).unwrap();
        assert_eq!(seen.borrow()[0].len(), 96_000);
    }

    #[test]
    fn too_much_speech_and_bad_metadata_never_reach_the_model() {
        let (mut m, seen) = model();
        let long = vec![0.1_f32; 90 * RATE_48K + 1];
        assert!(matches!(
            build_profile_v2(&mut m, &long, &[], &metadata()),
            Err(BuildError::TooMuchSpeech)
        ));
        let mut bad = metadata();
        bad.name = "  ".to_owned();
        let (joined, parts) = five_samples(2);
        assert!(matches!(
            build_profile_v2(&mut m, &joined, &parts, &bad),
            Err(BuildError::Enroll(EnrollmentError::InvalidMetadata(_)))
        ));
        assert!(seen.borrow().is_empty());
    }

    struct WrongSizeModel;
    impl EmbeddingExtractor for WrongSizeModel {
        fn extract_embedding(&mut self, _: &[f32]) -> Result<Vec<f32>, EnrollmentError> {
            Ok(vec![0.0; 10])
        }
    }

    #[test]
    fn a_model_that_returns_the_wrong_size_is_rejected() {
        let (joined, parts) = five_samples(2);
        let result = build_profile_v2(&mut WrongSizeModel, &joined, &parts, &metadata());
        assert!(matches!(
            result,
            Err(BuildError::Enroll(EnrollmentError::InvalidConditioning(_)))
        ));
    }

    #[cfg(feature = "tract")]
    #[test]
    fn a_v1_enrollment_asset_is_refused_at_load() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/enrollment-contract-test.onnx");
        let result = OnnxEmbeddingModel::from_file(&path);
        assert!(
            matches!(result, Err(EnrollmentError::Model(m)) if m.contains("speaker_embedding"))
        );
    }

    #[cfg(feature = "tract")]
    #[test]
    fn synthetic_onnx_contract_model_end_to_end() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/enrollment-v3-contract-test.onnx");
        let mut model = OnnxEmbeddingModel::from_file(&path).expect("load");
        let (joined, parts) = five_samples(2);
        let profile = build_profile_v2(&mut model, &joined, &parts, &metadata()).expect("build");
        profile.verify_integrity().unwrap();
        assert_eq!(profile.speaker_embedding().unwrap().as_slice().len(), 192);
        // Different audio gives a different embedding (the model is not a constant).
        let a = model.extract_embedding(&vec![0.1_f32; 96_000]).unwrap();
        let b = model.extract_embedding(&vec![0.4_f32; 96_000]).unwrap();
        assert_ne!(a, b);
        let norm: f32 = a.iter().map(|v| v * v).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "{norm}");
    }

    /// Runs the real v3 enrollment asset when `CLEARCORE_ENROLL_V3_ONNX` names it (the 84 MB
    /// asset is not in the repository). Checks the manifest parity: the embedding is unit-norm
    /// and the same take gives the same embedding twice.
    #[cfg(feature = "tract")]
    #[test]
    fn real_v3_asset_when_available() {
        let Some(path) = std::env::var_os("CLEARCORE_ENROLL_V3_ONNX") else {
            eprintln!("CLEARCORE_ENROLL_V3_ONNX not set: real asset test skipped");
            return;
        };
        let mut model = OnnxEmbeddingModel::from_file(std::path::Path::new(&path)).expect("load");
        let (joined, parts) = five_samples(2);
        let a = build_profile_v2(&mut model, &joined, &parts, &metadata()).expect("build");
        let b = build_profile_v2(&mut model, &joined, &parts, &metadata()).expect("build");
        assert_eq!(
            a.speaker_embedding().unwrap(),
            b.speaker_embedding().unwrap()
        );
        let norm: f32 = a
            .speaker_embedding()
            .unwrap()
            .as_slice()
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            .sqrt();
        assert!((norm - 1.0).abs() < 1e-3);
    }
}
