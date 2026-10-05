#![forbid(unsafe_code)]

use std::collections::VecDeque;

use deep_filter::{Complex32, DFState, post_filter};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ};

use crate::{
    ALGORITHM_LATENCY_SAMPLES, BackendDescriptor, InferenceBackend, InferenceError, ProcessedFrame,
    VoiceProfile,
};

pub const DEFAULT_FFT_SIZE: usize = 960;
pub const DEFAULT_HOP_SIZE: usize = HOP_SAMPLES;
pub const DEFAULT_NB_ERB: usize = 32;
pub const DEFAULT_NB_DF: usize = 96;
pub const DEFAULT_DF_ORDER: usize = 5;
pub const DEFAULT_LOOKAHEAD: usize = 2;
pub const DEFAULT_MIN_NB_ERB_FREQS: usize = 2;
pub const DEFAULT_NORM_ALPHA: f32 = 0.99;
pub const N_FREQS: usize = DEFAULT_FFT_SIZE / 2 + 1; // 481

/// Extracted spectral and ERB features ready for model inference.
#[derive(Debug, Clone)]
pub struct ExtractedFeatures {
    /// ERB log-power features per band. Shape: [32].
    pub feat_erb: [f32; DEFAULT_NB_ERB],
    /// Normalized complex spectrum features for the lower frequencies.
    /// Real parts in index 0, imaginary parts in index 1. Shape: [2][96].
    pub feat_spec: [[f32; DEFAULT_NB_DF]; 2],
    /// Current noisy complex spectrum. Length: 481.
    pub spec_noisy: Vec<Complex32>,
}

/// Output predictions from a neural network spectral model.
#[derive(Debug, Clone)]
pub struct ModelFrameOutput {
    /// Local SNR estimate in dB.
    pub lsnr: f32,
    /// Stage 1 ERB band gains in range [0.0, 1.0]. Shape: [32].
    pub erb_gains: Option<[f32; DEFAULT_NB_ERB]>,
    /// Stage 2 Complex FIR filter coefficients: 96 frequency bins x 5 temporal taps.
    pub df_coefs: Option<Vec<[Complex32; DEFAULT_DF_ORDER]>>,
}

/// Agnostic interface for neural network inference models operating in the spectral domain.
///
/// Any accelerator runtime (`OpenVINO`, `TensorRT`, `CoreML`, `DirectML`, `Vulkan`, `Tract`)
/// can implement this trait to plug into the common [`DspPipeline`].
pub trait SpectralModelBackend: Send {
    /// Name and version descriptor for the inference backend.
    fn descriptor(&self) -> BackendDescriptor;

    /// Runs model inference for a single hop given the extracted DSP features.
    fn process_spectrum(
        &mut self,
        features: &ExtractedFeatures,
    ) -> Result<ModelFrameOutput, InferenceError>;

    /// Resets the internal temporal state (GRUs, convolution delay rings) to zero.
    fn reset(&mut self) -> Result<(), InferenceError>;
}

/// Configuration options for the DSP pipeline.
///
/// The defaults equal `RuntimeParams::default()` of the libDF `DfTract` (post-filter off, no
/// attenuation limit, thresholds -10/30/20 dB); `tests/dsp_pipeline_parity.rs` proves the output
/// matches `DfTract::process` for them.
#[derive(Debug, Clone)]
pub struct DspPipelineConfig {
    pub min_db_thresh: f32,
    pub max_db_erb_thresh: f32,
    pub max_db_df_thresh: f32,
    /// Post-filter strength; `0.0` disables the post-filter (the default, as in libDF).
    pub post_filter_beta: f32,
    pub atten_lim_db: Option<f32>,
    pub alpha: f32,
}

impl Default for DspPipelineConfig {
    fn default() -> Self {
        Self {
            min_db_thresh: -10.0,
            max_db_erb_thresh: 30.0,
            max_db_df_thresh: 20.0,
            post_filter_beta: 0.0,
            atten_lim_db: None,
            alpha: DEFAULT_NORM_ALPHA,
        }
    }
}

/// Agnostic DSP pipeline decoupling STFT analysis, ERB filterbanks, and DF complex synthesis.
///
/// Manages temporal analysis/synthesis overlap memory, rolling noisy/enhanced
/// spectra rings, feature normalization, and deep filter convolution.
pub struct DspPipeline {
    config: DspPipelineConfig,
    state: DFState,
    rolling_spec_x: VecDeque<Vec<Complex32>>,
    rolling_spec_y: VecDeque<Vec<Complex32>>,
    spec_buf: Vec<Complex32>,
}

impl DspPipeline {
    /// Creates a new DSP pipeline with the specified configuration.
    pub fn new(config: DspPipelineConfig) -> Result<Self, InferenceError> {
        let mut state = DFState::new(
            SAMPLE_RATE_HZ as usize,
            DEFAULT_FFT_SIZE,
            DEFAULT_HOP_SIZE,
            DEFAULT_NB_ERB,
            DEFAULT_MIN_NB_ERB_FREQS,
        );
        state.init_norm_states(DEFAULT_NB_DF);

        let delay_x = DEFAULT_LOOKAHEAD.max(DEFAULT_DF_ORDER);
        let delay_y = DEFAULT_DF_ORDER + DEFAULT_LOOKAHEAD;

        let mut rolling_spec_x = VecDeque::with_capacity(delay_x);
        for _ in 0..delay_x {
            rolling_spec_x.push_back(vec![Complex32::default(); N_FREQS]);
        }

        let mut rolling_spec_y = VecDeque::with_capacity(delay_y);
        for _ in 0..delay_y {
            rolling_spec_y.push_back(vec![Complex32::default(); N_FREQS]);
        }

        let spec_buf = vec![Complex32::default(); N_FREQS];

        Ok(Self {
            config,
            state,
            rolling_spec_x,
            rolling_spec_y,
            spec_buf,
        })
    }

    /// Resets all internal DSP states (memories, normalizers, rolling buffers).
    pub fn reset(&mut self) {
        self.state.reset();
        self.state.init_norm_states(DEFAULT_NB_DF);
        for buf in &mut self.rolling_spec_x {
            buf.fill(Complex32::default());
        }
        for buf in &mut self.rolling_spec_y {
            buf.fill(Complex32::default());
        }
        self.spec_buf.fill(Complex32::default());
    }

    /// Step 1: Forward STFT and feature extraction from a time-domain frame.
    pub fn step_analysis(
        &mut self,
        input: &AudioFrame,
    ) -> Result<ExtractedFeatures, InferenceError> {
        if input.iter().any(|s| !s.is_finite()) {
            return Err(InferenceError::InputContract(
                "input frame contains non-finite samples".to_owned(),
            ));
        }

        let mut spec_current = vec![Complex32::default(); N_FREQS];
        self.state.analysis(input.as_slice(), &mut spec_current);

        // Update rolling spectra buffers
        self.rolling_spec_x.pop_front();
        self.rolling_spec_x.push_back(spec_current.clone());

        self.rolling_spec_y.pop_front();
        self.rolling_spec_y.push_back(spec_current.clone());

        // Extract ERB band energy features
        let mut feat_erb = [0.0f32; DEFAULT_NB_ERB];
        self.state
            .feat_erb(&spec_current, self.config.alpha, &mut feat_erb);

        // Extract normalized complex features for the first 96 bins. `feat_cplx` (interleaved
        // re/im) is the path libDF's `DfTract` feeds the encoder; `feat_cplx_t` must not be used
        // here: it only divides its output in place and never copies the input, so it would hand
        // the model an all-zero spectrum.
        let sub_spec = spec_current.get(..DEFAULT_NB_DF).ok_or_else(|| {
            InferenceError::InferenceExecution("spectrum slice out of bounds".to_owned())
        })?;
        let mut normalized = [Complex32::default(); DEFAULT_NB_DF];
        self.state
            .feat_cplx(sub_spec, self.config.alpha, &mut normalized);

        let mut feat_spec = [[0.0f32; DEFAULT_NB_DF]; 2];
        for (bin, value) in normalized.iter().enumerate() {
            feat_spec[0][bin] = value.re;
            feat_spec[1][bin] = value.im;
        }

        Ok(ExtractedFeatures {
            feat_erb,
            feat_spec,
            spec_noisy: spec_current,
        })
    }

    /// Evaluates which stages to execute based on local SNR.
    /// Returns (`apply_gains`, `apply_gain_zeros`, `apply_df`).
    pub fn apply_stages(&self, lsnr: f32) -> (bool, bool, bool) {
        if lsnr < self.config.min_db_thresh {
            (false, true, false)
        } else if lsnr > self.config.max_db_erb_thresh {
            (false, false, false)
        } else if lsnr > self.config.max_db_df_thresh {
            (true, false, false)
        } else {
            (true, false, true)
        }
    }

    /// Step 2: Synthesis, mask interpolation, DF filtering, and iSTFT backward transformation.
    pub fn step_synthesis(
        &mut self,
        output_pred: &ModelFrameOutput,
    ) -> Result<AudioFrame, InferenceError> {
        self.validate_prediction(output_pred)?;
        let (apply_gains, apply_gain_zeros, apply_df) = self.apply_stages(output_pred.lsnr);

        // Spectrum to enhance is at index (DEFAULT_DF_ORDER - 1) in rolling_spec_y
        let target_idx = DEFAULT_DF_ORDER.saturating_sub(1);
        let spec_target = self.rolling_spec_y.get_mut(target_idx).ok_or_else(|| {
            InferenceError::InferenceExecution("rolling_spec_y index out of bounds".to_owned())
        })?;

        // Apply ERB mask stage (presence of the gains was validated above)
        if apply_gains {
            if let Some(gains) = &output_pred.erb_gains {
                self.state
                    .apply_mask(spec_target.as_mut_slice(), gains.as_slice());
            }
        } else if apply_gain_zeros {
            let zero_gains = [0.0f32; DEFAULT_NB_ERB];
            self.state
                .apply_mask(spec_target.as_mut_slice(), zero_gains.as_slice());
        }

        // Copy enhanced spectrum to spec_buf
        self.spec_buf.copy_from_slice(spec_target);

        // Apply Stage 2 Deep Filtering (order-5 complex FIR filter)
        if let (true, Some(coefs)) = (apply_df, &output_pred.df_coefs) {
            self.apply_df_filter(coefs);
        }

        // Retrieve noisy reference spectrum for post-filtering / attenuation limiting
        let noisy_idx = (DEFAULT_LOOKAHEAD.max(DEFAULT_DF_ORDER))
            .saturating_sub(DEFAULT_LOOKAHEAD)
            .saturating_sub(1);
        let spec_noisy = self.rolling_spec_x.get(noisy_idx).ok_or_else(|| {
            InferenceError::InferenceExecution("rolling_spec_x index out of bounds".to_owned())
        })?;

        // Optional post-filter
        if (apply_gains || apply_df) && self.config.post_filter_beta > 0.0 {
            post_filter(spec_noisy, &mut self.spec_buf, self.config.post_filter_beta);
        }

        // Optional attenuation limit
        if let Some(lim) = self.attenuation_mix() {
            for (enh, &noisy) in self.spec_buf.iter_mut().zip(spec_noisy.iter()) {
                *enh = *enh * (1.0 - lim) + noisy * lim;
            }
        }

        // Inverse STFT synthesis
        let mut out_frame = [0.0f32; HOP_SAMPLES];
        self.state.synthesis(&mut self.spec_buf, &mut out_frame);

        Ok(out_frame)
    }

    /// Fraction of the noisy spectrum mixed back in, or `None` when there is no limit. Same rule as
    /// libDF's `set_atten_lim`: >= 100 dB means unlimited, < 0.01 dB means no reduction at all.
    fn attenuation_mix(&self) -> Option<f32> {
        let lim_db = self.config.atten_lim_db?.abs();
        if lim_db >= 100.0 {
            None
        } else if lim_db < 0.01 {
            Some(1.0)
        } else {
            Some(10.0f32.powf(-lim_db / 20.0))
        }
    }

    /// Fail-closed contract with the model: every stage the local SNR calls for must come with
    /// its data, otherwise the noisy spectrum would silently reach the output.
    fn validate_prediction(&self, prediction: &ModelFrameOutput) -> Result<(), InferenceError> {
        let missing = |what: &str| {
            InferenceError::InferenceExecution(format!(
                "model returned no {what} for a stage that requires them"
            ))
        };
        if !prediction.lsnr.is_finite() {
            return Err(InferenceError::InferenceExecution(
                "model returned a non-finite local SNR".to_owned(),
            ));
        }
        let (apply_gains, _, apply_df) = self.apply_stages(prediction.lsnr);
        if apply_gains && prediction.erb_gains.is_none() {
            return Err(missing("ERB gains"));
        }
        if apply_df {
            match &prediction.df_coefs {
                Some(coefs) if coefs.len() >= DEFAULT_NB_DF => {}
                Some(_) => {
                    return Err(InferenceError::InferenceExecution(
                        "model returned fewer deep-filter coefficient bins than the DF range"
                            .to_owned(),
                    ));
                }
                None => return Err(missing("deep-filter coefficients")),
            }
        }
        Ok(())
    }

    fn apply_df_filter(&mut self, coefs: &[[Complex32; DEFAULT_DF_ORDER]]) {
        for (f, coef_taps) in coefs.iter().take(DEFAULT_NB_DF).enumerate() {
            let mut sum_filtered = Complex32::default();
            for (k, tap) in coef_taps.iter().enumerate().take(DEFAULT_DF_ORDER) {
                let sample_opt = self
                    .rolling_spec_x
                    .get(k)
                    .and_then(|past_spec| past_spec.get(f));
                if let Some(&sample) = sample_opt {
                    sum_filtered += sample * tap;
                }
            }
            if let Some(out_bin) = self.spec_buf.get_mut(f) {
                *out_bin = sum_filtered;
            }
        }
    }

    /// End-to-end frame processing using any decoupled spectral model backend.
    pub fn process_frame<B: SpectralModelBackend + ?Sized>(
        &mut self,
        input: &AudioFrame,
        backend: &mut B,
    ) -> Result<AudioFrame, InferenceError> {
        let features = self.step_analysis(input)?;
        let prediction = backend.process_spectrum(&features)?;
        self.step_synthesis(&prediction)
    }
}

/// A complete [`InferenceBackend`] wrapping an agnostic [`DspPipeline`] with any [`SpectralModelBackend`].
pub struct AgnosticDspBackend<B: SpectralModelBackend> {
    pipeline: DspPipeline,
    model: B,
}

impl<B: SpectralModelBackend> AgnosticDspBackend<B> {
    pub fn new(model: B, config: DspPipelineConfig) -> Result<Self, InferenceError> {
        let pipeline = DspPipeline::new(config)?;
        Ok(Self { pipeline, model })
    }

    pub fn reset(&mut self) -> Result<(), InferenceError> {
        self.pipeline.reset();
        self.model.reset()
    }
}

impl<B: SpectralModelBackend> InferenceBackend for AgnosticDspBackend<B> {
    fn descriptor(&self) -> BackendDescriptor {
        self.model.descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        let samples = self.pipeline.process_frame(input, &mut self.model)?;
        ProcessedFrame::checked(samples, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }

    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        crate::reject_unsupported_voice_profile(profile)
    }

    fn supports_voice_profile(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock spectral model backend that passes audio unmodified (high SNR clean speech).
    struct BypassSpectralBackend {
        descriptor: BackendDescriptor,
    }

    impl BypassSpectralBackend {
        fn new() -> Self {
            Self {
                descriptor: BackendDescriptor {
                    backend: "mock-spectral",
                    backend_version: "1.0.0",
                    runtime: "mock",
                    runtime_version: "1.0.0",
                    asset_id: "mock-asset".to_owned(),
                    asset_sha256: "0".repeat(64),
                    cpu_profile: "any",
                },
            }
        }
    }

    impl SpectralModelBackend for BypassSpectralBackend {
        fn descriptor(&self) -> BackendDescriptor {
            self.descriptor.clone()
        }

        fn process_spectrum(
            &mut self,
            _features: &ExtractedFeatures,
        ) -> Result<ModelFrameOutput, InferenceError> {
            Ok(ModelFrameOutput {
                lsnr: 35.0, // Clean speech (> 30 dB threshold)
                erb_gains: Some([1.0; DEFAULT_NB_ERB]),
                df_coefs: None,
            })
        }

        fn reset(&mut self) -> Result<(), InferenceError> {
            Ok(())
        }
    }

    /// Mock spectral backend applying attenuation.
    struct MuteSpectralBackend {
        descriptor: BackendDescriptor,
    }

    impl SpectralModelBackend for MuteSpectralBackend {
        fn descriptor(&self) -> BackendDescriptor {
            self.descriptor.clone()
        }

        fn process_spectrum(
            &mut self,
            _features: &ExtractedFeatures,
        ) -> Result<ModelFrameOutput, InferenceError> {
            Ok(ModelFrameOutput {
                lsnr: -20.0, // Extreme noise (< -10 dB threshold -> zero mask)
                erb_gains: Some([0.0; DEFAULT_NB_ERB]),
                df_coefs: None,
            })
        }

        fn reset(&mut self) -> Result<(), InferenceError> {
            Ok(())
        }
    }

    #[test]
    fn test_pipeline_initialization_and_reset() -> Result<(), InferenceError> {
        let mut dsp = DspPipeline::new(DspPipelineConfig::default())?;
        dsp.reset();
        Ok(())
    }

    #[test]
    fn test_step_analysis_feature_dimensions() -> Result<(), InferenceError> {
        let mut dsp = DspPipeline::new(DspPipelineConfig::default())?;
        let input = [0.1f32; HOP_SAMPLES];
        let features = dsp.step_analysis(&input)?;

        assert_eq!(features.feat_erb.len(), DEFAULT_NB_ERB);
        assert_eq!(features.feat_spec[0].len(), DEFAULT_NB_DF);
        assert_eq!(features.feat_spec[1].len(), DEFAULT_NB_DF);
        assert_eq!(features.spec_noisy.len(), N_FREQS);
        for &val in &features.feat_erb {
            assert!(val.is_finite());
        }
        Ok(())
    }

    #[test]
    fn test_agnostic_backend_passthrough_finite_output() -> Result<(), InferenceError> {
        let backend = BypassSpectralBackend::new();
        let mut agnostic = AgnosticDspBackend::new(backend, DspPipelineConfig::default())?;

        let input = [0.05f32; HOP_SAMPLES];
        for _ in 0..10 {
            let processed = agnostic.process(&input)?;
            assert_eq!(
                processed.algorithmic_latency_samples,
                ALGORITHM_LATENCY_SAMPLES
            );
            for &s in &processed.samples {
                assert!(s.is_finite());
            }
        }
        Ok(())
    }

    fn sine_frames(count: usize, hz: f32) -> Vec<AudioFrame> {
        (0..count)
            .map(|frame| {
                std::array::from_fn(|index| {
                    let n =
                        f32::from(u16::try_from(frame * HOP_SAMPLES + index).unwrap_or(u16::MAX));
                    (2.0 * std::f32::consts::PI * hz * n / 48_000.0).sin() * 0.5
                })
            })
            .collect()
    }

    /// Mock whose prediction is fixed.
    struct FixedBackend(ModelFrameOutput);

    impl SpectralModelBackend for FixedBackend {
        fn descriptor(&self) -> BackendDescriptor {
            BackendDescriptor {
                backend: "fixed",
                backend_version: "1",
                runtime: "mock",
                runtime_version: "1",
                asset_id: "mock".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "any",
            }
        }

        fn process_spectrum(
            &mut self,
            _features: &ExtractedFeatures,
        ) -> Result<ModelFrameOutput, InferenceError> {
            Ok(self.0.clone())
        }

        fn reset(&mut self) -> Result<(), InferenceError> {
            Ok(())
        }
    }

    fn run_fixed(
        prediction: &ModelFrameOutput,
        input: &[AudioFrame],
    ) -> Result<Vec<f32>, InferenceError> {
        let mut pipeline = DspPipeline::new(DspPipelineConfig::default())?;
        let mut backend = FixedBackend(prediction.clone());
        let mut out = Vec::new();
        for frame in input {
            out.extend(pipeline.process_frame(frame, &mut backend)?);
        }
        Ok(out)
    }

    fn max_error_vs_delayed_input(input: &[AudioFrame], output: &[f32], skip_frames: usize) -> f32 {
        let delay = ALGORITHM_LATENCY_SAMPLES as usize;
        let flat: Vec<f32> = input.iter().flatten().copied().collect();
        (skip_frames * HOP_SAMPLES..output.len())
            .map(|n| (output[n] - flat[n - delay]).abs())
            .fold(0.0, f32::max)
    }

    #[test]
    fn apply_stages_thresholds_follow_the_libdf_contract() -> Result<(), InferenceError> {
        let dsp = DspPipeline::new(DspPipelineConfig::default())?;
        // (apply_gains, apply_gain_zeros, apply_df)
        assert_eq!(
            dsp.apply_stages(-10.01),
            (false, true, false),
            "below min: zero mask"
        );
        assert_eq!(
            dsp.apply_stages(-10.0),
            (true, false, true),
            "min is inclusive for stage 1+2"
        );
        assert_eq!(
            dsp.apply_stages(20.0),
            (true, false, true),
            "max_db_df is inclusive"
        );
        assert_eq!(
            dsp.apply_stages(20.01),
            (true, false, false),
            "above df threshold: gains only"
        );
        assert_eq!(
            dsp.apply_stages(30.0),
            (true, false, false),
            "max_db_erb is inclusive"
        );
        assert_eq!(
            dsp.apply_stages(30.01),
            (false, false, false),
            "clean speech: untouched"
        );
        Ok(())
    }

    #[test]
    fn untouched_stages_reconstruct_the_input_after_the_algorithmic_latency()
    -> Result<(), InferenceError> {
        let input = sine_frames(40, 440.0);
        let prediction = ModelFrameOutput {
            lsnr: 35.0,
            erb_gains: None,
            df_coefs: None,
        };
        let output = run_fixed(&prediction, &input)?;
        let error = max_error_vs_delayed_input(&input, &output, 8);
        assert!(
            error < 1e-3,
            "STFT/iSTFT must reconstruct the delayed input (max error {error})"
        );
        Ok(())
    }

    #[test]
    fn df_taps_are_aligned_with_the_enhanced_frame() -> Result<(), InferenceError> {
        // Tap 2 of the 5-frame noisy history is the frame aligned with the enhanced one
        // (rolling_spec_y[df_order - 1]); a unit filter there must be transparent below bin 96.
        let input = sine_frames(40, 440.0); // 440 Hz = bin ~9, inside the DF range
        let unit_at = |tap: usize| -> Vec<[Complex32; DEFAULT_DF_ORDER]> {
            (0..DEFAULT_NB_DF)
                .map(|_| {
                    std::array::from_fn(|k| Complex32::new(if k == tap { 1.0 } else { 0.0 }, 0.0))
                })
                .collect()
        };
        let prediction = |tap| ModelFrameOutput {
            lsnr: 10.0,
            erb_gains: Some([1.0; DEFAULT_NB_ERB]),
            df_coefs: Some(unit_at(tap)),
        };
        let aligned = run_fixed(&prediction(2), &input)?;
        assert!(max_error_vs_delayed_input(&input, &aligned, 8) < 1e-3);
        let misaligned = run_fixed(&prediction(3), &input)?;
        assert!(
            max_error_vs_delayed_input(&input, &misaligned, 8) > 1e-2,
            "a filter tap one frame off must be audibly different"
        );
        Ok(())
    }

    #[test]
    fn reset_makes_the_pipeline_reproducible() -> Result<(), InferenceError> {
        let input = sine_frames(20, 300.0);
        let prediction = ModelFrameOutput {
            lsnr: 35.0,
            erb_gains: None,
            df_coefs: None,
        };
        let mut pipeline = DspPipeline::new(DspPipelineConfig::default())?;
        let mut backend = FixedBackend(prediction);
        let mut first = Vec::new();
        for frame in &input {
            first.extend(pipeline.process_frame(frame, &mut backend)?);
        }
        pipeline.reset();
        let mut second = Vec::new();
        for frame in &input {
            second.extend(pipeline.process_frame(frame, &mut backend)?);
        }
        assert_eq!(first, second);
        Ok(())
    }

    #[test]
    fn non_finite_input_is_rejected_without_advancing_the_state() -> Result<(), InferenceError> {
        let input = sine_frames(12, 300.0);
        let prediction = ModelFrameOutput {
            lsnr: 35.0,
            erb_gains: None,
            df_coefs: None,
        };
        let expected = run_fixed(&prediction, &input)?;

        let mut pipeline = DspPipeline::new(DspPipelineConfig::default())?;
        let mut backend = FixedBackend(prediction);
        let mut poisoned = [0.0_f32; HOP_SAMPLES];
        poisoned[3] = f32::NAN;
        let mut out = Vec::new();
        for (index, frame) in input.iter().enumerate() {
            if index == 5 {
                assert!(matches!(
                    pipeline.process_frame(&poisoned, &mut backend),
                    Err(InferenceError::InputContract(_))
                ));
            }
            out.extend(pipeline.process_frame(frame, &mut backend)?);
        }
        assert_eq!(expected, out, "a rejected frame must leave no trace");
        Ok(())
    }

    #[test]
    fn missing_erb_gains_fail_closed_instead_of_leaking_unprocessed_audio()
    -> Result<(), InferenceError> {
        // lsnr 10 dB requires stage 1; a model that returns no gains must be an error, never a
        // silent pass-through of the noisy spectrum.
        let prediction = ModelFrameOutput {
            lsnr: 10.0,
            erb_gains: None,
            df_coefs: None,
        };
        let mut pipeline = DspPipeline::new(DspPipelineConfig::default())?;
        let mut backend = FixedBackend(prediction);
        let result = pipeline.process_frame(&[0.1; HOP_SAMPLES], &mut backend);
        assert!(
            matches!(result, Err(InferenceError::InferenceExecution(_))),
            "{result:?}"
        );
        Ok(())
    }

    #[test]
    fn missing_or_short_df_coefficients_fail_closed() -> Result<(), InferenceError> {
        let gains = Some([1.0; DEFAULT_NB_ERB]);
        let none = ModelFrameOutput {
            lsnr: 10.0,
            erb_gains: gains,
            df_coefs: None,
        };
        let short = ModelFrameOutput {
            lsnr: 10.0,
            erb_gains: gains,
            df_coefs: Some(vec![
                [Complex32::default(); DEFAULT_DF_ORDER];
                DEFAULT_NB_DF - 1
            ]),
        };
        for prediction in [none, short] {
            let mut pipeline = DspPipeline::new(DspPipelineConfig::default())?;
            let mut backend = FixedBackend(prediction);
            let result = pipeline.process_frame(&[0.1; HOP_SAMPLES], &mut backend);
            assert!(
                matches!(result, Err(InferenceError::InferenceExecution(_))),
                "{result:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn attenuation_limit_of_100_db_or_more_disables_the_limit() -> Result<(), InferenceError> {
        let input = sine_frames(30, 440.0);
        let prediction = ModelFrameOutput {
            lsnr: -20.0,
            erb_gains: Some([0.0; DEFAULT_NB_ERB]),
            df_coefs: None,
        };
        let run = |atten_lim_db: Option<f32>| -> Result<Vec<f32>, InferenceError> {
            let config = DspPipelineConfig {
                atten_lim_db,
                ..DspPipelineConfig::default()
            };
            let mut pipeline = DspPipeline::new(config)?;
            let mut backend = FixedBackend(prediction.clone());
            let mut out = Vec::new();
            for frame in &input {
                out.extend(pipeline.process_frame(frame, &mut backend)?);
            }
            Ok(out)
        };
        // libDF treats >= 100 dB as "no limit": identical to None, bit for bit.
        assert_eq!(run(None)?, run(Some(100.0))?);
        assert_eq!(run(None)?, run(Some(-120.0))?);
        // A real limit lets some of the noisy signal back in.
        let limited = run(Some(12.0))?;
        let energy = |x: &[f32]| x.iter().map(|s| s * s).sum::<f32>();
        assert!(energy(&limited) > energy(&run(None)?) * 10.0);
        Ok(())
    }

    #[test]
    fn feat_spec_layout_is_the_real_plane_then_the_imaginary_plane() -> Result<(), InferenceError> {
        // The encoder consumes `feat_cplx` (interleaved re/im per bin) permuted to [re plane, im plane].
        let input = sine_frames(6, 700.0);
        let mut dsp = DspPipeline::new(DspPipelineConfig::default())?;
        let mut reference = DFState::new(
            SAMPLE_RATE_HZ as usize,
            DEFAULT_FFT_SIZE,
            DEFAULT_HOP_SIZE,
            DEFAULT_NB_ERB,
            DEFAULT_MIN_NB_ERB_FREQS,
        );
        reference.init_norm_states(DEFAULT_NB_DF);
        let mut spec = vec![Complex32::default(); N_FREQS];
        for frame in &input {
            let features = dsp.step_analysis(frame)?;
            reference.analysis(frame.as_slice(), &mut spec);
            let mut interleaved = vec![Complex32::default(); DEFAULT_NB_DF];
            reference.feat_cplx(&spec[..DEFAULT_NB_DF], DEFAULT_NORM_ALPHA, &mut interleaved);
            for (bin, value) in interleaved.iter().enumerate() {
                assert_eq!(features.feat_spec[0][bin].to_bits(), value.re.to_bits());
                assert_eq!(features.feat_spec[1][bin].to_bits(), value.im.to_bits());
            }
            let mut erb = [0.0_f32; DEFAULT_NB_ERB];
            reference.feat_erb(&spec, DEFAULT_NORM_ALPHA, &mut erb);
            assert!(
                features
                    .feat_erb
                    .iter()
                    .zip(&erb)
                    .all(|(a, b)| a.to_bits() == b.to_bits()),
                "feat_erb must come from the same DFState call libDF uses"
            );
        }
        Ok(())
    }

    #[test]
    fn test_agnostic_backend_mute_attenuation() -> Result<(), InferenceError> {
        let backend = MuteSpectralBackend {
            descriptor: BackendDescriptor {
                backend: "mock",
                backend_version: "1",
                runtime: "mock",
                runtime_version: "1",
                asset_id: "mock".to_owned(),
                asset_sha256: "0".repeat(64),
                cpu_profile: "any",
            },
        };
        let mut agnostic = AgnosticDspBackend::new(backend, DspPipelineConfig::default())?;

        let input = [0.5f32; HOP_SAMPLES];
        let mut last_energy = 0.0f32;
        // Run enough frames for pipeline delay memory to fill and attenuate
        for _ in 0..15 {
            let processed = agnostic.process(&input)?;
            last_energy = processed.samples.iter().map(|s| s * s).sum::<f32>();
        }
        // Energy should be significantly attenuated compared to unattenuated input energy
        let in_energy = input.iter().map(|s| s * s).sum::<f32>();
        assert!(last_energy < in_energy * 0.1);
        Ok(())
    }
}
