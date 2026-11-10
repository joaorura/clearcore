//! The neural core of the filter: STFT/features → engine → per-hop output selection → profile EQ
//! → iSTFT, plus the digital-silence shortcut.
//!
//! The DSP half is `realtime_noise_model::DspPipeline` (the same code the daemon runs). Its stage
//! selection (`DspPipeline::apply_stages` + `step_synthesis`) is exactly the training-time
//! `emulate_gating_offline` of `clearcore-train/src/cctrain/eval/runtime_path.py:342-359` with
//! `DEFAULT_THRESHOLDS = (-10, 30, 20)` (`runtime_path.py:36`):
//!
//! | `lsnr`              | training (`runtime_path.py`)              | output of this core          |
//! | :------------------ | :---------------------------------------- | :--------------------------- |
//! | `< -10`             | `zero` (l.346, l.353)                     | zero spectrum                |
//! | `> 30`              | `none` → `spec_noisy` (l.347, l.355)      | unprocessed input spectrum   |
//! | `20 < lsnr <= 30`   | `erb_only` → `spec_masked` (l.348, l.354) | ERB mask only                |
//! | otherwise           | `spec_enhanced`                           | ERB mask + deep filter       |
//!
//! The graphs themselves run on every hop regardless of the regime (see `crate::engine`); the
//! unit tests below pin the four regimes.

use realtime_noise_contracts::AudioFrame;
use realtime_noise_model::spectral_eq::bin_factors;
use realtime_noise_model::{BandGains, DspPipeline, DspPipelineConfig, InferenceError};

use crate::engine::{Conditioning, FallbackEngine};

/// RMS below which a hop is digital silence: the output is zeros and neither the STFT nor the
/// model advances (decision of 2026-11-09, same rule as the daemon).
pub const SILENCE_RMS: f32 = 1e-7;

/// Which pDFNet3 Pro graphs the core runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    /// v3 FULL: `speaker_embedding` + `speaker_present`.
    V3Full,
    /// v2 stateful: `FiLM`.
    V2Film,
}

impl ModelKind {
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::V3Full => "pdfnet3-pro-v3-full",
            Self::V2Film => "pdfnet3-pro-v2-stateful",
        }
    }
}

pub struct NeuralCore {
    pub(crate) kind: ModelKind,
    pipeline: DspPipeline,
    pub(crate) engine: FallbackEngine,
    eq: Option<BandGains>,
    conditioning: Conditioning,
    silent_hops: u64,
}

/// `true` when the hop's RMS is below [`SILENCE_RMS`].
#[must_use]
pub fn is_digital_silence(frame: &AudioFrame) -> bool {
    let energy: f64 = frame.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
    #[allow(clippy::cast_precision_loss)]
    let rms = (energy / frame.len() as f64).sqrt();
    rms < f64::from(SILENCE_RMS)
}

impl NeuralCore {
    pub fn new(kind: ModelKind, engine: FallbackEngine) -> Result<Self, InferenceError> {
        Ok(Self {
            kind,
            pipeline: DspPipeline::new(DspPipelineConfig::default())?,
            engine,
            eq: None,
            conditioning: Conditioning::default(),
            silent_hops: 0,
        })
    }

    /// One 480-sample hop.
    pub fn process(&mut self, input: &AudioFrame) -> Result<AudioFrame, InferenceError> {
        if input.iter().any(|s| !s.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains a non-finite sample".to_owned(),
            ));
        }
        if is_digital_silence(input) {
            self.silent_hops = self.silent_hops.saturating_add(1);
            return Ok([0.0; 480]);
        }
        let output = self.pipeline.process_frame(input, &mut self.engine)?;
        if output.iter().all(|s| s.is_finite()) {
            Ok(output)
        } else {
            Err(InferenceError::NonFiniteOutput)
        }
    }

    /// Sets (or clears) the profile EQ: 32 ERB gains interpolated per bin, applied to the enhanced
    /// spectrum right before the iSTFT (zero added latency). Validated before anything changes.
    pub fn set_eq(&mut self, eq: Option<BandGains>) -> Result<(), InferenceError> {
        let factors = match &eq {
            Some(gains) if !gains.is_neutral() => Some(
                bin_factors(gains, self.pipeline.erb_band_widths())
                    .map_err(|e| InferenceError::InputContract(format!("{e:?}")))?,
            ),
            _ => None,
        };
        self.pipeline.set_spectral_eq_factors(factors)?;
        self.eq = eq.filter(|g| !g.is_neutral());
        Ok(())
    }

    /// Replaces the speaker conditioning (recurrent state untouched).
    pub fn set_conditioning(&mut self, conditioning: Conditioning) -> Result<(), InferenceError> {
        self.engine
            .set_conditioning(conditioning.clone())
            .map_err(InferenceError::InputContract)?;
        self.conditioning = conditioning;
        Ok(())
    }

    #[must_use]
    pub const fn conditioning(&self) -> &Conditioning {
        &self.conditioning
    }

    #[must_use]
    pub fn eq_active(&self) -> bool {
        self.eq.is_some()
    }

    #[must_use]
    pub fn speaker_active(&self) -> bool {
        match self.kind {
            ModelKind::V3Full => self.conditioning.embedding.is_some(),
            ModelKind::V2Film => self
                .conditioning
                .film
                .as_ref()
                .is_some_and(|f| !f.is_identity()),
        }
    }

    #[must_use]
    pub const fn silent_hops(&self) -> u64 {
        self.silent_hops
    }

    /// Committed recurrent state of the active engine (diagnostics and tests).
    #[must_use]
    pub fn engine_state(&self) -> Option<Vec<Vec<f32>>> {
        self.engine.export_state()
    }

    /// Zeroes the model state only (the STFT memories stay): tests of the state feedback.
    pub fn zero_model_state(&mut self) {
        let _ = realtime_noise_model::SpectralModelBackend::reset(&mut self.engine);
    }

    pub fn reset(&mut self) -> Result<(), InferenceError> {
        self.pipeline.reset();
        realtime_noise_model::SpectralModelBackend::reset(&mut self.engine)
    }
}

#[cfg(test)]
mod tests {
    use deep_filter::Complex32;
    use realtime_noise_model::dsp_pipeline::{DEFAULT_DF_ORDER, DEFAULT_NB_DF, DEFAULT_NB_ERB};
    use realtime_noise_model::{
        ALGORITHM_LATENCY_SAMPLES, BackendDescriptor, ExtractedFeatures, ModelFrameOutput,
    };

    use super::*;
    use crate::engine::{Engine, LatencyPolicy};

    /// Engine returning a fixed prediction and counting its hops.
    struct Fixed {
        lsnr: f32,
        mask: f32,
        df_tap: Option<usize>,
        hops: std::sync::Arc<std::sync::atomic::AtomicU64>,
    }

    impl Engine for Fixed {
        fn label(&self) -> String {
            "fixed".to_owned()
        }
        fn run(&mut self, _f: &ExtractedFeatures) -> Result<ModelFrameOutput, String> {
            self.hops.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let coefs = (0..DEFAULT_NB_DF)
                .map(|_| {
                    std::array::from_fn::<_, DEFAULT_DF_ORDER, _>(|k| {
                        Complex32::new(if Some(k) == self.df_tap { 1.0 } else { 0.0 }, 0.0)
                    })
                })
                .collect();
            Ok(ModelFrameOutput {
                lsnr: self.lsnr,
                erb_gains: Some([self.mask; DEFAULT_NB_ERB]),
                df_coefs: Some(coefs),
            })
        }
        fn reset(&mut self) -> Result<(), String> {
            Ok(())
        }
        fn set_conditioning(&mut self, _c: &Conditioning) -> Result<(), String> {
            Ok(())
        }
        fn supports_speaker(&self) -> bool {
            false
        }
    }

    fn core(
        lsnr: f32,
        mask: f32,
        df_tap: Option<usize>,
    ) -> (NeuralCore, std::sync::Arc<std::sync::atomic::AtomicU64>) {
        let hops = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let engine = FallbackEngine::new(
            BackendDescriptor {
                backend: "fixed",
                backend_version: "1",
                runtime: "mock",
                runtime_version: "1",
                asset_id: "mock".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "any",
            },
            Box::new(Fixed {
                lsnr,
                mask,
                df_tap,
                hops: hops.clone(),
            }),
            None,
            LatencyPolicy::from_env(),
        );
        (
            NeuralCore::new(ModelKind::V3Full, engine).expect("core"),
            hops,
        )
    }

    fn sine(frames: usize) -> Vec<AudioFrame> {
        (0..frames)
            .map(|f| {
                std::array::from_fn(|i| {
                    #[allow(clippy::cast_precision_loss)]
                    let n = (f * 480 + i) as f32;
                    0.3 * (std::f32::consts::TAU * 440.0 * n / 48_000.0).sin()
                        + 0.2 * (std::f32::consts::TAU * 9_000.0 * n / 48_000.0).sin()
                })
            })
            .collect()
    }

    fn run(core: &mut NeuralCore, input: &[AudioFrame]) -> Vec<f32> {
        input
            .iter()
            .flat_map(|f| core.process(f).expect("hop"))
            .collect()
    }

    /// Max |output - input delayed by the algorithmic latency| after the first `skip` hops.
    fn error_vs_delayed(input: &[AudioFrame], output: &[f32], skip: usize, gain: f32) -> f32 {
        let delay = ALGORITHM_LATENCY_SAMPLES as usize;
        let flat: Vec<f32> = input.iter().flatten().copied().collect();
        (skip * 480..output.len())
            .map(|n| (output[n] - gain * flat[n - delay]).abs())
            .fold(0.0, f32::max)
    }

    // runtime_path.py:346,353 — lsnr < -10: zero.
    #[test]
    fn regime_zero_below_minus_10_db() {
        let input = sine(30);
        let (mut c, hops) = core(-10.5, 1.0, Some(2));
        let out = run(&mut c, &input);
        assert!(out[8 * 480..].iter().all(|s| s.abs() < 1e-6));
        assert_eq!(
            hops.load(std::sync::atomic::Ordering::Relaxed),
            30,
            "model ran every hop"
        );
    }

    // runtime_path.py:347,355 — lsnr > 30: the unprocessed input spectrum, even with a zero mask.
    #[test]
    fn regime_none_above_30_db_passes_the_input_through() {
        let input = sine(30);
        let (mut c, _) = core(30.5, 0.0, None);
        let out = run(&mut c, &input);
        assert!(error_vs_delayed(&input, &out, 8, 1.0) < 1e-3);
    }

    // runtime_path.py:348,354 — 20 < lsnr <= 30: ERB mask only (DF coefs ignored).
    #[test]
    fn regime_erb_only_between_20_and_30_db() {
        let input = sine(30);
        let (mut c, _) = core(30.0, 0.5, None);
        let out = run(&mut c, &input);
        assert!(
            error_vs_delayed(&input, &out, 8, 0.5) < 1e-3,
            "mask 0.5 only, DF zero coefs ignored"
        );
    }

    // runtime_path.py:353 — otherwise: ERB mask + deep filter (DF replaces the 96 low bins).
    #[test]
    fn regime_erb_and_df_at_or_below_20_db() {
        let input = sine(30);
        // Mask 0.5 everywhere, DF = unit tap 2 (aligned) on the 96 low bins: the 440 Hz partial
        // (DF band) keeps full gain, the 9 kHz partial (mask only) is halved.
        let (mut c, _) = core(20.0, 0.5, Some(2));
        let out = run(&mut c, &input);
        let (mut erb_only, _) = core(25.0, 0.5, Some(2));
        let reference = run(&mut erb_only, &input);
        let diff = out
            .iter()
            .zip(&reference)
            .skip(8 * 480)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(diff > 0.05, "DF must change the low band ({diff})");
        let delay = ALGORITHM_LATENCY_SAMPLES as usize;
        let flat: Vec<f32> = input.iter().flatten().copied().collect();
        let expected = |n: usize| {
            #[allow(clippy::cast_precision_loss)]
            let t = (n - delay) as f32;
            0.3 * (std::f32::consts::TAU * 440.0 * t / 48_000.0).sin()
                + 0.1 * (std::f32::consts::TAU * 9_000.0 * t / 48_000.0).sin()
        };
        let err = (10 * 480..out.len())
            .map(|n| (out[n] - expected(n)).abs())
            .fold(0.0f32, f32::max);
        assert!(err < 2e-2, "ERB+DF output deviates by {err}");
        let _ = flat;
    }

    #[test]
    fn digital_silence_returns_zeros_without_advancing_the_model_or_the_stft() {
        let input = sine(20);
        let (mut a, hops_a) = core(10.0, 0.7, Some(2));
        let plain = run(&mut a, &input);
        let (mut b, hops_b) = core(10.0, 0.7, Some(2));
        let mut gapped = Vec::new();
        for (i, frame) in input.iter().enumerate() {
            if i == 10 {
                for _ in 0..5 {
                    assert_eq!(b.process(&[1e-9; 480]).expect("silent hop"), [0.0; 480]);
                }
            }
            gapped.extend(b.process(frame).expect("hop"));
        }
        assert_eq!(
            plain, gapped,
            "silent hops leave no trace in STFT or model state"
        );
        assert_eq!(hops_a.load(std::sync::atomic::Ordering::Relaxed), 20);
        assert_eq!(hops_b.load(std::sync::atomic::Ordering::Relaxed), 20);
        assert_eq!(b.silent_hops(), 5);
        assert!(!is_digital_silence(&[1e-6; 480]));
    }

    #[test]
    fn profile_eq_scales_the_output_before_the_istft() {
        let input = sine(30);
        let (mut flat, _) = core(35.0, 1.0, None);
        let reference = run(&mut flat, &input);
        let (mut eq, _) = core(35.0, 1.0, None);
        eq.set_eq(Some(BandGains::from_array([-6.0; 32]).expect("gains")))
            .expect("eq");
        assert!(eq.eq_active());
        let out = run(&mut eq, &input);
        let gain = 10f32.powf(-6.0 / 20.0);
        let worst = reference
            .iter()
            .zip(&out)
            .map(|(r, o)| (gain * r - o).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-5, "flat -6 dB EQ is a pure gain ({worst})");
        assert!(
            eq.set_eq(Some(BandGains {
                gains_db: [40.0; 32]
            }))
            .is_err()
        );
        eq.set_eq(None).expect("clear");
        assert!(!eq.eq_active());
    }
}
