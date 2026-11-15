//! pDFNet3 Pro v7 (A2-300k) in streaming, with the silence gate C: the isolation model used
//! while a voice profile is active (keep the enrolled voice, cut the others).
//!
//! Contract: `docs/pdfnet3-pro-v7-stream.md` (copy of the exporter's `SPEC-v7-stream.md`).
//! Summary:
//!
//! - six graphs: `enroll_tokens` once per profile; per analysed hop `enc_conv` (current frame),
//!   `detector` (1 frame behind, it needs 120 samples of the next frame for the 16 kHz level
//!   feature), `enc_post`, `erb_dec`, `df_dec` (21 frames behind: 20 frames of detector
//!   lookahead + 1);
//! - a FIFO of the last 22 `enc_conv` outputs and one of 21 analysis spectra: the mask and the
//!   deep filter of frame `t' = t - 21` are applied to the spectrum of `t'` with the usual
//!   `DfTract` rings ([`DspPipeline::synthesize_delayed`]);
//! - the level feature: input resampled to 16 kHz by the training FIR v1 (241 taps, archive member
//!   `resampler-48k-16k-v1.coeffs.f32`, NOT [`crate::resample`]) and a causal recurrence;
//! - stage ZERO when `lsnr < -10` (the decoders do not run, their state does not advance), else
//!   ERB mask + deep filter; the gate C multiplies the output spectrum;
//! - digital silence (RMS < 1e-7) is not analysed: zeros out, nothing advances;
//! - [`V7Stream::flush`] emits the last 21 frames at the end of a stream (offline use).
//!
//! Latency: 1 440 samples (STFT + lookahead, as the base model) + 21 frames = 11 520 samples
//! (240 ms).
//!
//! Any runtime implementing [`GraphSession`] runs it (tract, ONNX Runtime).

use std::collections::VecDeque;

use deep_filter::{Complex32, DFState};
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ};

use crate::dsp_pipeline::{
    DEFAULT_DF_ORDER, DEFAULT_FFT_SIZE, DEFAULT_MIN_NB_ERB_FREQS, DEFAULT_NB_DF, DEFAULT_NB_ERB,
    DEFAULT_NORM_ALPHA, N_FREQS, StagePolicy,
};
use crate::stateful_asset::StatefulModelFiles;
use crate::stateful_graph::{GraphSession, check_stream_contract};
use crate::{
    ALGORITHM_LATENCY_SAMPLES, DspPipeline, DspPipelineConfig, InferenceError, ModelFrameOutput,
};

/// `model` the manifest of a v7 archive declares.
pub const V7_MODEL_ID: &str = "pdfnet3-pro-v7-a2-300k";
/// Archive member with the 241 FIR taps (48 → 16 kHz) of the level feature.
pub const V7_FIR_MEMBER: &str = "resampler-48k-16k-v1.coeffs.f32";
/// Taps of the level FIR.
pub const V7_FIR_TAPS: usize = 241;
/// Speaker tokens produced by `enroll_tokens` (`[1, 48, 128]`).
pub const V7_TOKENS_LEN: usize = 48 * 128;
/// Speaker embedding length (`[1, 192]`).
pub const V7_EMBEDDING_LEN: usize = 192;
/// Detector delay (frames): it waits for 120 samples of the next frame.
pub const DETECTOR_DELAY: usize = 1;
/// Detector lookahead (frames).
pub const DETECTOR_LOOKAHEAD: usize = 20;
/// Frames between the analysed frame and the frame whose output is produced.
pub const POST_DELAY: usize = DETECTOR_DELAY + DETECTOR_LOOKAHEAD;
/// Total algorithmic latency of the v7 stream (240 ms).
#[allow(clippy::cast_possible_truncation)]
pub const V7_LATENCY_SAMPLES: u32 = ALGORITHM_LATENCY_SAMPLES + (POST_DELAY * HOP_SAMPLES) as u32;
/// Local SNR (dB) below which the frame is the ZERO stage.
pub const ZERO_BELOW_LSNR_DB: f32 = -10.0;
/// RMS below which a hop is digital silence (not analysed).
pub const DIGITAL_SILENCE_RMS: f32 = 1e-7;
/// Frames per enrollment chunk, and chunks.
pub const ENROLL_CHUNK_FRAMES: usize = 64;
pub const ENROLL_CHUNKS: usize = 4;
/// Longest enrollment audio (12 s at 48 kHz).
pub const ENROLL_MAX_SAMPLES_48K: usize = 12 * 48_000;

const ENC_OUT: [&str; 6] = ["x", "e0", "e1", "e2", "e3", "c0"];
const X_LEN: usize = 512;

fn contract(message: impl Into<String>) -> InferenceError {
    InferenceError::InputContract(message.into())
}

fn corrupt(message: impl Into<String>) -> InferenceError {
    InferenceError::ModelCorruption(message.into())
}

fn execution(message: impl Into<String>) -> InferenceError {
    InferenceError::InferenceExecution(message.into())
}

/// One compiled graph with its input/output buffers, ports resolved by name. Every input named
/// `<s>_in` is a state fed back from the output `<s>_out` of the same graph after each run
/// (zeros at start and after [`Graph::reset`]).
struct Graph<S> {
    name: &'static str,
    session: S,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
    states: Vec<(usize, usize)>,
}

impl<S: GraphSession> Graph<S> {
    fn new(name: &'static str, session: S) -> Result<Self, InferenceError> {
        let inputs = session
            .inputs()
            .iter()
            .map(|spec| {
                spec.static_len()
                    .map(|len| vec![0.0; len])
                    .ok_or_else(|| corrupt(format!("`{name}.{}` has no static shape", spec.name)))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut states = Vec::new();
        for (index, spec) in session.inputs().iter().enumerate() {
            if let Some(stem) = spec.name.strip_suffix("_in") {
                let out = format!("{stem}_out");
                let output = session
                    .outputs()
                    .iter()
                    .position(|o| o.name == out)
                    .ok_or_else(|| corrupt(format!("`{name}.{}` has no `{out}`", spec.name)))?;
                states.push((index, output));
            }
        }
        let outputs = vec![Vec::new(); session.outputs().len()];
        Ok(Self {
            name,
            session,
            inputs,
            outputs,
            states,
        })
    }

    fn input(&self, port: &str) -> Result<usize, InferenceError> {
        self.session
            .inputs()
            .iter()
            .position(|spec| spec.name == port)
            .ok_or_else(|| corrupt(format!("graph `{}` has no input `{port}`", self.name)))
    }

    fn output(&self, port: &str) -> Result<usize, InferenceError> {
        self.session
            .outputs()
            .iter()
            .position(|spec| spec.name == port)
            .ok_or_else(|| corrupt(format!("graph `{}` has no output `{port}`", self.name)))
    }

    fn set(&mut self, index: usize, values: &[f32]) -> Result<(), InferenceError> {
        let buffer = &mut self.inputs[index];
        if buffer.len() != values.len() {
            return Err(execution(format!(
                "`{}.{}` takes {} values, got {}",
                self.name,
                self.session.inputs()[index].name,
                buffer.len(),
                values.len()
            )));
        }
        buffer.copy_from_slice(values);
        Ok(())
    }

    fn run(&mut self) -> Result<(), InferenceError> {
        let refs: Vec<&[f32]> = self.inputs.iter().map(Vec::as_slice).collect();
        self.session.run(&refs, &mut self.outputs)?;
        if self.outputs.iter().flatten().any(|v| !v.is_finite()) {
            return Err(InferenceError::NonFiniteOutput);
        }
        for &(input, output) in &self.states {
            if self.outputs[output].len() != self.inputs[input].len() {
                return Err(execution(format!(
                    "state `{}` of `{}` changed size",
                    self.session.inputs()[input].name,
                    self.name
                )));
            }
        }
        for &(input, output) in &self.states {
            let (inputs, outputs) = (&mut self.inputs, &self.outputs);
            inputs[input].copy_from_slice(&outputs[output]);
        }
        Ok(())
    }

    fn out(&self, index: usize) -> &[f32] {
        &self.outputs[index]
    }

    fn reset(&mut self) {
        for &(input, _) in &self.states {
            self.inputs[input].fill(0.0);
        }
    }
}

/// Verified v7 archive: the six graphs and the level FIR.
#[derive(Debug, Clone)]
pub struct V7Asset {
    pub files: StatefulModelFiles,
    pub fir: Vec<f32>,
}

impl V7Asset {
    /// Checks model id, stream contract, the six graphs and the FIR member.
    pub fn from_files(files: StatefulModelFiles) -> Result<Self, InferenceError> {
        if files.manifest.model != V7_MODEL_ID {
            return Err(corrupt(format!(
                "archive model `{}` is not `{V7_MODEL_ID}`",
                files.manifest.model
            )));
        }
        check_stream_contract(files.manifest.hop, files.manifest.sample_rate)?;
        for graph in [
            "enroll_tokens",
            "enc_conv",
            "detector",
            "enc_post",
            "erb_dec",
            "df_dec",
        ] {
            if !files.graphs.iter().any(|(name, _)| name == graph) {
                return Err(corrupt(format!("v7 archive has no `{graph}` graph")));
            }
        }
        let fir = files
            .f32_member(V7_FIR_MEMBER)
            .ok_or_else(|| corrupt(format!("v7 archive has no `{V7_FIR_MEMBER}`")))?
            .to_vec();
        if fir.len() != V7_FIR_TAPS || fir.iter().any(|v| !v.is_finite()) {
            return Err(corrupt("v7 level FIR is not 241 finite taps"));
        }
        Ok(Self { files, fir })
    }

    /// Reads and verifies a v7 `.tar.gz` (the caller checked its whole-file SHA-256).
    pub fn from_archive_bytes(bytes: &[u8]) -> Result<Self, InferenceError> {
        Self::from_files(crate::stateful_asset::read_manifest_archive(bytes)?)
    }

    fn graph(&self, name: &str) -> Result<&[u8], InferenceError> {
        self.files
            .graphs
            .iter()
            .find(|(graph, _)| graph == name)
            .map(|(_, bytes)| bytes.as_slice())
            .ok_or_else(|| corrupt(format!("v7 archive has no `{name}` graph")))
    }

    /// Compiles `enroll_tokens` with `load` (a runtime's [`GraphSession`] constructor).
    pub fn enroll_model<S, F>(&self, mut load: F) -> Result<V7EnrollModel<S>, InferenceError>
    where
        S: GraphSession,
        F: FnMut(&str, &[u8]) -> Result<S, InferenceError>,
    {
        V7EnrollModel::new(load("enroll_tokens", self.graph("enroll_tokens")?)?)
    }

    /// Compiles the five per-hop graphs with `load` and builds a stream.
    pub fn stream<S, F>(&self, mut load: F) -> Result<V7Stream<S>, InferenceError>
    where
        S: GraphSession,
        F: FnMut(&str, &[u8]) -> Result<S, InferenceError>,
    {
        let mut graph = |name: &'static str| -> Result<Graph<S>, InferenceError> {
            Graph::new(name, load(name, self.graph(name)?)?)
        };
        let graphs = Graphs {
            enc_conv: graph("enc_conv")?,
            detector: graph("detector")?,
            enc_post: graph("enc_post")?,
            erb_dec: graph("erb_dec")?,
            df_dec: graph("df_dec")?,
        };
        V7Stream::new(graphs, &self.fir)
    }
}

// ---------------------------------------------------------------------------------------------
// Level feature (§5)
// ---------------------------------------------------------------------------------------------

/// FIR 48 → 16 kHz v1 + the causal level recurrence, over the analysed frames.
#[derive(Debug, Clone)]
struct LevelFeature {
    fir: Vec<f32>,
    /// `x48` from absolute sample `base` on (only the tail still needed is kept).
    x48: Vec<f32>,
    base: usize,
    m: f64,
    pk: f64,
    frames: usize,
}

impl LevelFeature {
    const HALF: usize = 120;

    fn new(fir: &[f32]) -> Self {
        Self {
            fir: fir.to_vec(),
            x48: Vec::with_capacity(4 * HOP_SAMPLES),
            base: 0,
            m: 0.0,
            pk: 0.0,
            frames: 0,
        }
    }

    fn reset(&mut self) {
        self.x48.clear();
        self.base = 0;
        self.m = 0.0;
        self.pk = 0.0;
        self.frames = 0;
    }

    fn push(&mut self, samples: &[f32]) {
        self.x48.extend_from_slice(samples);
    }

    fn sample(&self, index: isize) -> f32 {
        if index < 0 {
            return 0.0;
        }
        #[allow(clippy::cast_sign_loss)]
        let index = index as usize;
        index
            .checked_sub(self.base)
            .and_then(|i| self.x48.get(i))
            .copied()
            .unwrap_or(0.0)
    }

    /// Level of the next frame (`frames`, in order); needs `x48` up to `480·t + 599`.
    fn next(&mut self) -> [f32; 3] {
        let t = self.frames;
        let mut energy = 0.0f64;
        for i in 0..HOP_SAMPLES / 3 {
            let n = 160 * t + i;
            #[allow(clippy::cast_possible_wrap)]
            let center = (3 * n + Self::HALF) as isize;
            let mut y = 0.0f32;
            for (k, h) in self.fir.iter().enumerate() {
                #[allow(clippy::cast_possible_wrap)]
                let x = self.sample(center - k as isize);
                y += h * x;
            }
            energy += f64::from(y) * f64::from(y);
        }
        #[allow(clippy::cast_possible_truncation)]
        let e = (10.0 * (energy / 160.0 + 1e-12).log10()) as f32;
        let w = ((e + 75.0) / 45.0).clamp(0.0, 1.0);
        let a = 1.0 - (-0.01f64 / 2.0).exp();
        let dec = 3.0 * 0.01;
        let e64 = f64::from(e);
        if t == 0 {
            self.m = e64;
            self.pk = e64;
        }
        self.m += a * f64::from(w) * (e64 - self.m);
        self.pk = e64.max(self.pk - dec);
        self.frames += 1;
        // Drop what no later frame needs (frame t+1 starts reading at 480(t+1) - 120).
        let keep_from = (HOP_SAMPLES * self.frames).saturating_sub(Self::HALF);
        if keep_from > self.base {
            let drop = (keep_from - self.base).min(self.x48.len());
            self.x48.drain(..drop);
            self.base += drop;
        }
        #[allow(clippy::cast_possible_truncation)]
        [
            ((e64 - self.m) / 20.0) as f32,
            ((e64 - self.pk) / 20.0) as f32,
            w,
        ]
    }
}

// ---------------------------------------------------------------------------------------------
// Gate C (§7)
// ---------------------------------------------------------------------------------------------

/// Silence gate C: floor -25 dB, hangover 200 ms, release 50 ms, `q >= 0.5` or `lsnr >= -3`.
#[derive(Debug, Clone, Copy)]
pub struct GateC {
    g: f32,
    hang: u32,
}

impl Default for GateC {
    fn default() -> Self {
        Self { g: 1.0, hang: 0 }
    }
}

impl GateC {
    const HANG_FRAMES: u32 = 20;

    /// One output frame: the gain for it.
    pub fn step(&mut self, q: f32, lsnr: f32) -> f32 {
        let floor = 10f32.powf(-25.0 / 20.0);
        let a = 1.0 - (-10.0f32 / 50.0).exp();
        let speech = q >= 0.5 || lsnr >= -3.0;
        self.hang = if speech {
            Self::HANG_FRAMES
        } else {
            self.hang.saturating_sub(1)
        };
        let target = if speech || self.hang > 0 { 1.0 } else { floor };
        self.g = if target > self.g {
            target
        } else {
            self.g + a * (target - self.g)
        };
        self.g
    }
}

// ---------------------------------------------------------------------------------------------
// Stream (§4)
// ---------------------------------------------------------------------------------------------

struct Graphs<S> {
    enc_conv: Graph<S>,
    detector: Graph<S>,
    enc_post: Graph<S>,
    erb_dec: Graph<S>,
    df_dec: Graph<S>,
}

/// Port indices resolved once.
#[derive(Debug, Clone, Copy)]
struct Ports {
    conv_erb: usize,
    conv_spec: usize,
    conv_emb: usize,
    conv_out: [usize; 6],
    det_x: usize,
    det_level: usize,
    det_tokens: usize,
    det_emb: usize,
    det_probs: usize,
    det_o: usize,
    post_x: usize,
    post_o: usize,
    post_emb: usize,
    post_emb_out: usize,
    post_emb_erb: usize,
    post_erb_gain: usize,
    post_lsnr: usize,
    post_gamma: usize,
    post_beta: usize,
    erb_emb: usize,
    erb_e: [usize; 4],
    erb_gain: usize,
    erb_m: usize,
    df_emb: usize,
    df_c0: usize,
    df_gamma: usize,
    df_beta: usize,
    df_coefs: usize,
}

/// One `enc_conv` output set: `x, e0, e1, e2, e3, c0`.
type EncOut = [Vec<f32>; 6];

/// Diagnostics of one output frame (tests and the status).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct V7FrameTrace {
    pub lsnr: f32,
    pub q: f32,
    pub gain: f32,
    pub zero_stage: bool,
}

/// The v7 streaming runtime: 480-sample hops in, 480-sample hops out, 240 ms behind.
pub struct V7Stream<S> {
    graphs: Graphs<S>,
    ports: Ports,
    pipeline: DspPipeline,
    level: LevelFeature,
    enc_fifo: VecDeque<EncOut>,
    spare: Vec<EncOut>,
    spec_fifo: VecDeque<Vec<Complex32>>,
    probs: [f32; 3],
    o: Vec<f32>,
    gate: GateC,
    gate_enabled: bool,
    analysed: usize,
    profile: bool,
    trace: Option<Vec<V7FrameTrace>>,
}

impl<S: GraphSession> V7Stream<S> {
    #[allow(clippy::many_single_char_names)] // one letter per graph keeps the port table readable
    fn new(graphs: Graphs<S>, fir: &[f32]) -> Result<Self, InferenceError> {
        let c = &graphs.enc_conv;
        let d = &graphs.detector;
        let p = &graphs.enc_post;
        let e = &graphs.erb_dec;
        let f = &graphs.df_dec;
        let mut conv_out = [0; 6];
        for (slot, name) in conv_out.iter_mut().zip(ENC_OUT) {
            *slot = c.output(name)?;
        }
        let ports = Ports {
            conv_erb: c.input("feat_erb")?,
            conv_spec: c.input("feat_spec")?,
            conv_emb: c.input("speaker_embedding")?,
            conv_out,
            det_x: d.input("x_det")?,
            det_level: d.input("level")?,
            det_tokens: d.input("tokens")?,
            det_emb: d.input("speaker_embedding")?,
            det_probs: d.output("probs")?,
            det_o: d.output("o")?,
            post_x: p.input("x_post")?,
            post_o: p.input("o")?,
            post_emb: p.input("speaker_embedding")?,
            post_emb_out: p.output("emb")?,
            post_emb_erb: p.output("emb_erb")?,
            post_erb_gain: p.output("erb_gain")?,
            post_lsnr: p.output("lsnr")?,
            post_gamma: p.output("df_gamma")?,
            post_beta: p.output("df_beta")?,
            erb_emb: e.input("emb_erb")?,
            erb_e: [
                e.input("e0")?,
                e.input("e1")?,
                e.input("e2")?,
                e.input("e3")?,
            ],
            erb_gain: e.input("erb_gain")?,
            erb_m: e.output("m")?,
            df_emb: f.input("emb")?,
            df_c0: f.input("c0")?,
            df_gamma: f.input("df_gamma")?,
            df_beta: f.input("df_beta")?,
            df_coefs: f.output("coefs")?,
        };
        if c.inputs[ports.conv_erb].len() != DEFAULT_NB_ERB
            || c.inputs[ports.conv_spec].len() != 2 * DEFAULT_NB_DF
            || d.inputs[ports.det_tokens].len() != V7_TOKENS_LEN
            || d.inputs[ports.det_x].len() != X_LEN
            || d.inputs[ports.det_level].len() != 3
        {
            return Err(corrupt("v7 graph shapes differ from the contract"));
        }
        let o_len = p.inputs[ports.post_o].len();
        Ok(Self {
            graphs,
            ports,
            pipeline: DspPipeline::new(DspPipelineConfig::default())?,
            level: LevelFeature::new(fir),
            enc_fifo: VecDeque::with_capacity(POST_DELAY + 1),
            spare: Vec::with_capacity(POST_DELAY + 1),
            spec_fifo: VecDeque::with_capacity(POST_DELAY + 1),
            probs: [0.0; 3],
            o: vec![0.0; o_len],
            gate: GateC::default(),
            gate_enabled: true,
            analysed: 0,
            profile: false,
            trace: None,
        })
    }

    /// Conditions every graph on the profile (`speaker_embedding` `[1,192]`, `tokens`
    /// `[1,48,128]`). The stream state is kept.
    pub fn set_profile(&mut self, embedding: &[f32], tokens: &[f32]) -> Result<(), InferenceError> {
        if embedding.len() != V7_EMBEDDING_LEN || tokens.len() != V7_TOKENS_LEN {
            return Err(contract(format!(
                "v7 profile needs {V7_EMBEDDING_LEN} embedding values and {V7_TOKENS_LEN} tokens"
            )));
        }
        if embedding.iter().chain(tokens).any(|v| !v.is_finite()) {
            return Err(contract("v7 profile holds a non-finite value"));
        }
        let ports = self.ports;
        self.graphs.enc_conv.set(ports.conv_emb, embedding)?;
        self.graphs.detector.set(ports.det_emb, embedding)?;
        self.graphs.detector.set(ports.det_tokens, tokens)?;
        self.graphs.enc_post.set(ports.post_emb, embedding)?;
        self.profile = true;
        Ok(())
    }

    /// Whether [`Self::set_profile`] was called (the v7 has no neutral mode).
    #[must_use]
    pub const fn has_profile(&self) -> bool {
        self.profile
    }

    /// Turns the gate C on (default) or off (`g ≡ 1`).
    pub const fn set_gate(&mut self, enabled: bool) {
        self.gate_enabled = enabled;
    }

    /// Per-bin EQ factors applied before the iSTFT (the profile's microphone EQ), or `None`.
    pub fn set_spectral_eq_factors(
        &mut self,
        factors: Option<Vec<f32>>,
    ) -> Result<(), InferenceError> {
        self.pipeline.set_spectral_eq_factors(factors)
    }

    /// FFT bins per ERB band (for [`crate::spectral_eq::bin_factors`]).
    #[must_use]
    pub fn erb_band_widths(&self) -> &[usize] {
        self.pipeline.erb_band_widths()
    }

    /// Records a [`V7FrameTrace`] per output frame from now on (tests, diagnostics).
    pub fn record_trace(&mut self) {
        self.trace = Some(Vec::new());
    }

    /// The recorded traces.
    #[must_use]
    pub fn trace(&self) -> &[V7FrameTrace] {
        self.trace.as_deref().unwrap_or(&[])
    }

    /// Analysed frames since the start or the last reset.
    #[must_use]
    pub const fn analysed_frames(&self) -> usize {
        self.analysed
    }

    /// Back to the start of a stream: every state, FIFO, level and gate (profile kept).
    pub fn reset(&mut self) {
        self.graphs.enc_conv.reset();
        self.graphs.detector.reset();
        self.graphs.enc_post.reset();
        self.graphs.erb_dec.reset();
        self.graphs.df_dec.reset();
        self.pipeline.reset();
        self.level.reset();
        while let Some(entry) = self.enc_fifo.pop_front() {
            self.spare.push(entry);
        }
        self.spec_fifo.clear();
        self.probs = [0.0; 3];
        self.o.fill(0.0);
        self.gate = GateC::default();
        self.analysed = 0;
        if let Some(trace) = &mut self.trace {
            trace.clear();
        }
    }

    /// One 480-sample hop (§4).
    pub fn process(&mut self, input: &AudioFrame) -> Result<AudioFrame, InferenceError> {
        if !self.profile {
            return Err(contract("the v7 stream runs only with a voice profile"));
        }
        if input.iter().any(|s| !s.is_finite()) {
            return Err(contract("input frame contains a non-finite sample"));
        }
        let energy: f64 = input.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        #[allow(clippy::cast_precision_loss)]
        if (energy / HOP_SAMPLES as f64).sqrt() < f64::from(DIGITAL_SILENCE_RMS) {
            return Ok([0.0; HOP_SAMPLES]);
        }
        let features = self.pipeline.analyze_detached(input)?;
        let mut feat_spec = [0.0f32; 2 * DEFAULT_NB_DF];
        feat_spec[..DEFAULT_NB_DF].copy_from_slice(&features.feat_spec[0]);
        feat_spec[DEFAULT_NB_DF..].copy_from_slice(&features.feat_spec[1]);
        self.level.push(input);
        self.step(&features.feat_erb, &feat_spec, features.spec_noisy, true)
    }

    /// End of stream: 21 more iterations with zero features to emit the last 21 analysed
    /// frames (§4 "flush"). The stream is reset afterwards.
    pub fn flush(&mut self) -> Result<Vec<AudioFrame>, InferenceError> {
        let mut frames = Vec::with_capacity(POST_DELAY);
        if self.analysed > 0 {
            self.level.push(&[0.0; HOP_SAMPLES]);
            for index in 0..POST_DELAY {
                let out = self.step(
                    &[0.0; DEFAULT_NB_ERB],
                    &[0.0; 2 * DEFAULT_NB_DF],
                    vec![Complex32::default(); N_FREQS],
                    index == 0,
                )?;
                if self.analysed > POST_DELAY {
                    frames.push(out);
                }
            }
        }
        self.reset();
        Ok(frames)
    }

    #[allow(clippy::too_many_lines)] // the §4 sequence of one hop, kept in one place
    fn step(
        &mut self,
        feat_erb: &[f32],
        feat_spec: &[f32],
        spec: Vec<Complex32>,
        run_detector: bool,
    ) -> Result<AudioFrame, InferenceError> {
        let ports = self.ports;
        let t = self.analysed;
        // 1. enc_conv on the current frame -> FIFO (t-21 .. t).
        let conv = &mut self.graphs.enc_conv;
        conv.set(ports.conv_erb, feat_erb)?;
        conv.set(ports.conv_spec, feat_spec)?;
        conv.run()?;
        if self.enc_fifo.len() > POST_DELAY {
            let old = self.enc_fifo.pop_front().unwrap_or_default();
            self.spare.push(old);
        }
        let mut slot = self.spare.pop().unwrap_or_default();
        for (buffer, &index) in slot.iter_mut().zip(&ports.conv_out) {
            buffer.clear();
            buffer.extend_from_slice(self.graphs.enc_conv.out(index));
        }
        self.enc_fifo.push_back(slot);
        self.spec_fifo.push_back(spec);
        // 2. detector on t-1.
        if t >= DETECTOR_DELAY && run_detector {
            let level = self.level.next();
            let x_prev = &self.enc_fifo[self.enc_fifo.len() - 2][0];
            let det = &mut self.graphs.detector;
            det.set(ports.det_x, x_prev)?;
            det.set(ports.det_level, &level)?;
            det.run()?;
            let probs = det.out(ports.det_probs);
            if probs.len() != 3 {
                return Err(execution("detector probs is not 3 values"));
            }
            self.probs.copy_from_slice(probs);
            let o = det.out(ports.det_o);
            if o.len() != self.o.len() {
                return Err(execution("detector `o` changed size"));
            }
            self.o.copy_from_slice(o);
        }
        self.analysed = t + 1;
        // 3. output frame t' = t - 21.
        if t < POST_DELAY {
            return Ok([0.0; HOP_SAMPLES]);
        }
        let spec_out = self
            .spec_fifo
            .pop_front()
            .ok_or_else(|| execution("v7 spectrum FIFO is empty"))?;
        let entry = &self.enc_fifo[0];
        let post = &mut self.graphs.enc_post;
        post.set(ports.post_x, &entry[0])?;
        post.set(ports.post_o, &self.o)?;
        post.run()?;
        let lsnr = *post
            .out(ports.post_lsnr)
            .first()
            .ok_or_else(|| execution("enc_post returned no lsnr"))?;
        let q = self.probs[1] + self.probs[2];
        let gain = if self.gate_enabled {
            self.gate.step(q, lsnr)
        } else {
            1.0
        };
        let zero_stage = lsnr < ZERO_BELOW_LSNR_DB;
        let prediction = if zero_stage {
            ModelFrameOutput {
                lsnr,
                erb_gains: None,
                df_coefs: None,
            }
        } else {
            let post = &self.graphs.enc_post;
            let erb = &mut self.graphs.erb_dec;
            erb.set(ports.erb_emb, post.out(ports.post_emb_erb))?;
            erb.set(ports.erb_gain, post.out(ports.post_erb_gain))?;
            for (k, &port) in ports.erb_e.iter().enumerate() {
                erb.set(port, &entry[1 + k])?;
            }
            erb.run()?;
            let df = &mut self.graphs.df_dec;
            df.set(ports.df_emb, post.out(ports.post_emb_out))?;
            df.set(ports.df_c0, &entry[5])?;
            df.set(ports.df_gamma, post.out(ports.post_gamma))?;
            df.set(ports.df_beta, post.out(ports.post_beta))?;
            df.run()?;
            let mask = self.graphs.erb_dec.out(ports.erb_m);
            let coefs = self.graphs.df_dec.out(ports.df_coefs);
            if mask.len() != DEFAULT_NB_ERB || coefs.len() != DEFAULT_NB_DF * DEFAULT_DF_ORDER * 2 {
                return Err(execution(
                    "erb_dec/df_dec output sizes differ from the contract",
                ));
            }
            let mut erb_gains = [0.0f32; DEFAULT_NB_ERB];
            erb_gains.copy_from_slice(mask);
            let df_coefs = coefs
                .chunks_exact(DEFAULT_DF_ORDER * 2)
                .map(|bin| {
                    std::array::from_fn(|tap| Complex32::new(bin[2 * tap], bin[2 * tap + 1]))
                })
                .collect();
            ModelFrameOutput {
                lsnr,
                erb_gains: Some(erb_gains),
                df_coefs: Some(df_coefs),
            }
        };
        if let Some(trace) = &mut self.trace {
            trace.push(V7FrameTrace {
                lsnr,
                q,
                gain,
                zero_stage,
            });
        }
        self.pipeline
            .synthesize_delayed(&spec_out, &prediction, StagePolicy::ZeroOrFull, gain)
    }
}

// ---------------------------------------------------------------------------------------------
// Enrollment (§6)
// ---------------------------------------------------------------------------------------------

/// Round to the nearest `f16` (ties to even) and back, as the training stores features.
#[must_use]
pub fn round_f16(value: f32) -> f32 {
    if !value.is_finite() || value == 0.0 {
        return value;
    }
    let bits = value.to_bits();
    let sign = bits & 0x8000_0000;
    let abs = f32::from_bits(bits & 0x7fff_ffff);
    // Overflow to infinity (largest f16 is 65504).
    if abs >= 65520.0 {
        return f32::from_bits(sign | 0x7f80_0000);
    }
    #[allow(clippy::cast_possible_wrap)]
    let exp = ((bits >> 23) & 0xff) as i32 - 127;
    // f16 spacing: normal numbers keep 10 mantissa bits; subnormals have step 2^-24.
    let step_exp = if exp < -14 { -24 } else { exp - 10 };
    let step = 2f64.powi(step_exp);
    let scaled = f64::from(abs) / step;
    let rounded = scaled.round_ties_even() * step;
    #[allow(clippy::cast_possible_truncation)]
    let rounded = rounded as f32;
    f32::from_bits(sign | rounded.to_bits())
}

/// `enroll_tokens` inputs computed from the enrollment audio (§6.1-6.4).
#[derive(Debug, Clone, PartialEq)]
pub struct EnrollInputs {
    /// `[1, 4, 64, 32]`.
    pub feat_erb: Vec<f32>,
    /// `[1, 4, 64, 96, 2]` (re, im).
    pub feat_spec: Vec<f32>,
    /// `[1, 4, 64]` in {0, 1}.
    pub act: Vec<f32>,
    /// Chosen chunk starts (frames).
    pub starts: [usize; ENROLL_CHUNKS],
}

fn chunk_starts(act: &[bool]) -> [usize; ENROLL_CHUNKS] {
    let n = act.len();
    let count = n - ENROLL_CHUNK_FRAMES + 1;
    let mut active = 0usize;
    let mut frac = Vec::with_capacity(count);
    for (i, &a) in act.iter().enumerate() {
        active += usize::from(a);
        if i >= ENROLL_CHUNK_FRAMES {
            active -= usize::from(act[i - ENROLL_CHUNK_FRAMES]);
        }
        if i + 1 >= ENROLL_CHUNK_FRAMES {
            #[allow(clippy::cast_precision_loss)]
            frac.push(active as f64 / ENROLL_CHUNK_FRAMES as f64);
        }
    }
    let mut good: Vec<usize> = (0..count).filter(|&s| frac[s] >= 0.5).collect();
    if good.is_empty() {
        let mut order: Vec<usize> = (0..count).collect();
        order.sort_by(|&a, &b| frac[b].total_cmp(&frac[a]).then(a.cmp(&b)));
        good = order.into_iter().take(ENROLL_CHUNKS).collect();
        good.sort_unstable();
    }
    let last = good.len() - 1;
    std::array::from_fn(|i| {
        // numpy.linspace(0, last, 4) then numpy.round (half to even).
        #[allow(clippy::cast_precision_loss)]
        let position = if i == ENROLL_CHUNKS - 1 {
            last as f64
        } else {
            i as f64 * (last as f64 / (ENROLL_CHUNKS - 1) as f64)
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = position.round_ties_even() as usize;
        good[index.min(last)]
    })
}

/// Features, activity and chunk choice of the enrollment audio (48 kHz, the same audio that
/// goes to the ECAPA). Truncated to 12 s, padded with zeros to at least 64 frames.
#[must_use]
pub fn enrollment_inputs(audio_48k: &[f32]) -> EnrollInputs {
    let audio = &audio_48k[..audio_48k.len().min(ENROLL_MAX_SAMPLES_48K)];
    let frames = audio.len().div_ceil(HOP_SAMPLES).max(ENROLL_CHUNK_FRAMES);
    let mut state = DFState::new(
        SAMPLE_RATE_HZ as usize,
        DEFAULT_FFT_SIZE,
        HOP_SAMPLES,
        DEFAULT_NB_ERB,
        DEFAULT_MIN_NB_ERB_FREQS,
    );
    state.init_norm_states(DEFAULT_NB_DF);
    let mut erb = vec![0.0f32; frames * DEFAULT_NB_ERB];
    let mut cplx = vec![0.0f32; frames * DEFAULT_NB_DF * 2];
    let mut level = vec![0.0f64; frames];
    let mut spec = vec![Complex32::default(); N_FREQS];
    let mut frame = [0.0f32; HOP_SAMPLES];
    let mut normalized = [Complex32::default(); DEFAULT_NB_DF];
    for i in 0..frames {
        frame.fill(0.0);
        let start = (i * HOP_SAMPLES).min(audio.len());
        let end = ((i + 1) * HOP_SAMPLES).min(audio.len());
        frame[..end - start].copy_from_slice(&audio[start..end]);
        let power: f64 = frame.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        #[allow(clippy::cast_precision_loss)]
        let mean = power / HOP_SAMPLES as f64;
        level[i] = 10.0 * (mean + 1e-20).log10();
        state.analysis(&frame, &mut spec);
        state.feat_erb(
            &spec,
            DEFAULT_NORM_ALPHA,
            &mut erb[i * DEFAULT_NB_ERB..(i + 1) * DEFAULT_NB_ERB],
        );
        state.feat_cplx(&spec[..DEFAULT_NB_DF], DEFAULT_NORM_ALPHA, &mut normalized);
        for (bin, value) in normalized.iter().enumerate() {
            let at = (i * DEFAULT_NB_DF + bin) * 2;
            cplx[at] = value.re;
            cplx[at + 1] = value.im;
        }
    }
    let max = level.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let threshold = (max - 30.0).max(-75.0);
    let act: Vec<bool> = level
        .iter()
        .map(|&lv| max > -75.0 && lv >= threshold)
        .collect();
    let starts = chunk_starts(&act);
    let mut inputs = EnrollInputs {
        feat_erb: Vec::with_capacity(ENROLL_CHUNKS * ENROLL_CHUNK_FRAMES * DEFAULT_NB_ERB),
        feat_spec: Vec::with_capacity(ENROLL_CHUNKS * ENROLL_CHUNK_FRAMES * DEFAULT_NB_DF * 2),
        act: Vec::with_capacity(ENROLL_CHUNKS * ENROLL_CHUNK_FRAMES),
        starts,
    };
    for &s in &starts {
        let range = s..s + ENROLL_CHUNK_FRAMES;
        inputs.feat_erb.extend(
            erb[range.start * DEFAULT_NB_ERB..range.end * DEFAULT_NB_ERB]
                .iter()
                .map(|v| round_f16(*v)),
        );
        inputs.feat_spec.extend(
            cplx[range.start * DEFAULT_NB_DF * 2..range.end * DEFAULT_NB_DF * 2]
                .iter()
                .map(|v| round_f16(*v)),
        );
        inputs
            .act
            .extend(act[range].iter().map(|&a| if a { 1.0 } else { 0.0 }));
    }
    inputs
}

/// The compiled `enroll_tokens` graph.
pub struct V7EnrollModel<S> {
    graph: Graph<S>,
    ports: [usize; 4],
    tokens: usize,
}

impl<S: GraphSession> V7EnrollModel<S> {
    fn new(session: S) -> Result<Self, InferenceError> {
        let graph = Graph::new("enroll_tokens", session)?;
        let ports = [
            graph.input("enr_feat_erb")?,
            graph.input("enr_feat_spec")?,
            graph.input("enr_act")?,
            graph.input("speaker_embedding")?,
        ];
        let tokens = graph.output("tokens")?;
        Ok(Self {
            graph,
            ports,
            tokens,
        })
    }

    /// Runs `enroll_tokens` on prepared inputs.
    pub fn tokens_from_inputs(
        &mut self,
        inputs: &EnrollInputs,
        embedding: &[f32],
    ) -> Result<Vec<f32>, InferenceError> {
        if embedding.len() != V7_EMBEDDING_LEN {
            return Err(contract("speaker embedding must hold 192 values"));
        }
        let [erb_port, spec_port, act_port, embedding_port] = self.ports;
        self.graph.set(erb_port, &inputs.feat_erb)?;
        self.graph.set(spec_port, &inputs.feat_spec)?;
        self.graph.set(act_port, &inputs.act)?;
        self.graph.set(embedding_port, embedding)?;
        self.graph.run()?;
        let tokens = self.graph.out(self.tokens).to_vec();
        if tokens.len() != V7_TOKENS_LEN {
            return Err(execution("enroll_tokens did not return 48 x 128 tokens"));
        }
        Ok(tokens)
    }

    /// Tokens of a profile from its enrollment audio (48 kHz) and its ECAPA embedding.
    pub fn tokens(
        &mut self,
        enrollment_48k: &[f32],
        embedding: &[f32],
    ) -> Result<Vec<f32>, InferenceError> {
        self.tokens_from_inputs(&enrollment_inputs(enrollment_48k), embedding)
    }
}

/// An enrollment embedding model plus the v7 `enroll_tokens`: the profiles it builds are v7
/// profiles (embedding + speaker tokens).
pub struct WithV7Tokens<M> {
    inner: M,
    tokens: V7EnrollModel<Box<dyn GraphSession>>,
}

impl<M> WithV7Tokens<M> {
    #[must_use]
    pub const fn new(inner: M, tokens: V7EnrollModel<Box<dyn GraphSession>>) -> Self {
        Self { inner, tokens }
    }
}

impl<M: crate::enrollment_v3::EmbeddingExtractor> crate::enrollment_v3::EmbeddingExtractor
    for WithV7Tokens<M>
{
    fn extract_embedding(
        &mut self,
        samples_16khz: &[f32],
    ) -> Result<Vec<f32>, crate::enrollment::EnrollmentError> {
        self.inner.extract_embedding(samples_16khz)
    }

    fn extract_speaker_tokens(
        &mut self,
        enrollment_48k: &[f32],
        embedding: &[f32],
    ) -> Result<Option<Vec<f32>>, crate::enrollment::EnrollmentError> {
        self.tokens
            .tokens(enrollment_48k, embedding)
            .map(Some)
            .map_err(|e| crate::enrollment::EnrollmentError::Model(e.to_string()))
    }
}

// ---------------------------------------------------------------------------------------------
// Daemon backend: base model without a profile, v7 with one
// ---------------------------------------------------------------------------------------------

/// The isolation backend of the daemon: base model without a profile, v7 with one.
///
/// The bundled base isolation model (pDFNet3 Pro v3 full, neutral voice) runs while no v7
/// profile is applied; a speaker conditioning that carries the v7 tokens (intensity > 0) switches
/// every hop to the [`V7Stream`] (fresh stream, 240 ms of latency). A conditioning without
/// tokens (a profile built for the v3 model) is refused.
pub struct V7IsolationBackend {
    base: Box<dyn crate::InferenceBackend>,
    v7: V7Stream<Box<dyn GraphSession>>,
    v7_active: bool,
    descriptor: crate::BackendDescriptor,
}

impl V7IsolationBackend {
    /// Wraps `base` and the compiled v7 stream; `descriptor` is reported while the v7 runs.
    #[must_use]
    pub fn new(
        base: Box<dyn crate::InferenceBackend>,
        v7: V7Stream<Box<dyn GraphSession>>,
        descriptor: crate::BackendDescriptor,
    ) -> Self {
        Self {
            base,
            v7,
            v7_active: false,
            descriptor,
        }
    }

    /// Whether the v7 stream processes the hops (a v7 profile is applied).
    #[must_use]
    pub const fn v7_active(&self) -> bool {
        self.v7_active
    }
}

impl crate::InferenceBackend for V7IsolationBackend {
    fn descriptor(&self) -> crate::BackendDescriptor {
        if self.v7_active {
            self.descriptor.clone()
        } else {
            self.base.descriptor()
        }
    }

    fn process(&mut self, input: &AudioFrame) -> Result<crate::ProcessedFrame, InferenceError> {
        if self.v7_active {
            let samples = self.v7.process(input)?;
            crate::ProcessedFrame::checked(samples, V7_LATENCY_SAMPLES, self.descriptor.clone())
        } else {
            self.base.process(input)
        }
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        if self.v7_active {
            V7_LATENCY_SAMPLES
        } else {
            self.base.algorithmic_latency_samples()
        }
    }

    fn set_voice_profile(
        &mut self,
        profile: Option<&crate::VoiceProfile>,
    ) -> Result<(), InferenceError> {
        self.base.set_voice_profile(profile)?;
        self.v7_active = false;
        Ok(())
    }

    fn supports_voice_profile(&self) -> bool {
        self.base.supports_voice_profile()
    }

    fn set_speaker_conditioning(
        &mut self,
        conditioning: Option<&crate::SpeakerConditioning>,
    ) -> Result<(), InferenceError> {
        let Some(conditioning) = conditioning else {
            self.base.set_speaker_conditioning(None)?;
            self.v7_active = false;
            return Ok(());
        };
        conditioning.validate()?;
        let Some(tokens) = &conditioning.speaker_tokens else {
            return Err(InferenceError::UnsupportedFeature(
                "voice profile has no pDFNet3 Pro v7 speaker tokens (built for an earlier model); \
                 enroll again"
                    .to_owned(),
            ));
        };
        if conditioning.intensity_percent == 0 {
            // Isolation at 0 %: the base model, neutral voice, with the profile EQ (as before).
            self.base.set_speaker_conditioning(Some(conditioning))?;
            self.v7_active = false;
            return Ok(());
        }
        let eq = match &conditioning.eq {
            Some(gains) if !gains.is_neutral() => Some(crate::spectral_eq::bin_factors(
                gains,
                self.v7.erb_band_widths(),
            )?),
            _ => None,
        };
        self.v7
            .set_profile(&conditioning.speaker_embedding, tokens)?;
        self.v7.set_spectral_eq_factors(eq)?;
        if !self.v7_active {
            self.v7.reset();
        }
        self.v7_active = true;
        Ok(())
    }

    fn supports_speaker_embedding(&self) -> bool {
        true
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::suboptimal_flops)]
    fn f16_rounding_matches_ieee_half() {
        assert_eq!(round_f16(1.0), 1.0);
        assert_eq!(round_f16(0.1), 0.099_975_586);
        assert_eq!(round_f16(-2.718_281_7), -2.71875);
        // Tie to even: 1 + 2^-11 is exactly between 1 and 1 + 2^-10.
        assert_eq!(round_f16(1.0 + 2f32.powi(-11)), 1.0);
        assert_eq!(
            round_f16(1.0 + 3.0 * 2f32.powi(-11)),
            1.0 + 2.0 * 2f32.powi(-10)
        );
        // Subnormal step 2^-24.
        assert_eq!(round_f16(2f32.powi(-25) * 3.0), 2f32.powi(-23));
        assert_eq!(round_f16(1e6), f32::INFINITY);
    }

    #[test]
    fn chunk_choice_follows_linspace_and_round_half_even() {
        // All active, 200 frames: good = 0..=136, linspace(0,136,4) = 0, 45.33, 90.67, 136.
        let act = vec![true; 200];
        assert_eq!(chunk_starts(&act), [0, 45, 91, 136]);
        // Nothing active: the 4 best (all 0) are the 4 smallest starts.
        let act = vec![false; 100];
        assert_eq!(chunk_starts(&act), [0, 1, 2, 3]);
        // Exactly 64 frames: a single start, repeated.
        assert_eq!(chunk_starts(&[true; 64]), [0, 0, 0, 0]);
    }

    #[test]
    fn gate_c_rises_at_once_holds_200_ms_then_releases_to_the_floor() {
        let mut gate = GateC::default();
        assert_eq!(gate.step(0.9, -20.0), 1.0);
        // hang = 20 on speech, then 19 .. 1 keep the gain at 1; the 20th silent frame releases.
        for _ in 0..19 {
            assert_eq!(gate.step(0.0, -20.0), 1.0);
        }
        assert!(gate.step(0.0, -20.0) < 1.0);
        let mut last = 1.0;
        for _ in 0..200 {
            last = gate.step(0.0, -20.0);
        }
        assert!((last - 10f32.powf(-25.0 / 20.0)).abs() < 1e-4);
        assert_eq!(gate.step(0.0, 0.0), 1.0);
    }
}
