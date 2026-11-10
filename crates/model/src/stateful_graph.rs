//! Generic executor for stateful causal graphs (one 10 ms hop per call, `S = 1`).
//!
//! One code path for every runtime (tract, `OpenVINO` CPU/GPU/NPU, ONNX Runtime): a runtime only
//! has to implement [`GraphSession`] (compile one ONNX graph with every dynamic dimension pinned to
//! 1, run it with inputs and outputs in port order). Everything else is resolved **by name** when
//! the executor is built:
//!
//! - **Features**: `feat_erb` (`[1, 1, 1, 32]`) and `feat_spec` (`[1, 2, 1, 96]`, real then imaginary
//!   parts), produced by [`crate::DspPipeline`].
//! - **Conditioning**: `speaker_embedding` (`[1, 192]`, ECAPA, L2-normalised) and `speaker_present`
//!   (`[1]`) for the pDFNet3 Pro v3 full graph; zeros / `0.0` without a profile. The legacy `FiLM`
//!   graphs (`gamma`/`beta`, `[1, 1, 256]`) are still fed (identity without a profile), the encoder
//!   vectors to a graph named `enc*`, the deep-filter ones to a graph named `df*`.
//! - **State**: every `<name>_out` output is fed back into `<name>_in` (or `<name>`, the legacy
//!   naming) of the same graph, plus any explicit pair of the manifest. Zeros at start and after
//!   [`StatefulGraphExecutor::reset`]; an unpaired `*_in` input is a load error (fail closed).
//!   The new state is committed only after every graph of the hop ran without error.
//! - **Graph-to-graph** values (a split export, e.g. `enc` → `emb`, `e0`.. → `erb_dec`): any other
//!   input is taken from the output of the same name of another graph, which fixes the execution
//!   order (topological; a cycle or a missing producer is a load error).
//! - **Outputs**: `m` (32 ERB gains), `coefs` (96 bins x 5 taps x re/im) and `lsnr`.
//!
//! Every graph runs on **every** hop and every state advances on every hop (the training forward
//! of the Pro v3): unlike libDF's `DfTract`, the decoders are never skipped by local SNR. The
//! local-SNR stage selection only decides what [`crate::DspPipeline`] applies to the spectrum.

use std::fmt::Write as _;

use deep_filter::Complex32;
use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, SAMPLE_RATE_HZ};

use crate::dsp_pipeline::{DEFAULT_DF_ORDER, DEFAULT_NB_DF, DEFAULT_NB_ERB};
use crate::speaker_conditioning::{
    SPEAKER_EMBEDDING_LEN as SPEAKER_EMBEDDING_DIM, SpeakerConditioning,
};
use crate::{
    ALGORITHM_LATENCY_SAMPLES, BackendDescriptor, BandGains, DspPipeline, DspPipelineConfig,
    ExtractedFeatures, FILM_HIDDEN_DIM, FiLMVectors, InferenceBackend, InferenceError,
    ModelFrameOutput, ProcessedFrame, SpectralModelBackend, VoiceProfile, spectral_eq,
};

/// Input carrying the ERB features.
pub const INPUT_FEAT_ERB: &str = "feat_erb";
/// Input carrying the normalised complex spectrum features.
pub const INPUT_FEAT_SPEC: &str = "feat_spec";
/// Conditioning input of the Pro v3 full graph (`[1, 192]`).
pub const INPUT_SPEAKER_EMBEDDING: &str = "speaker_embedding";
/// Conditioning gate of the Pro v3 full graph (`[1]`, `1.0` with a profile, `0.0` without).
pub const INPUT_SPEAKER_PRESENT: &str = "speaker_present";
/// Legacy `FiLM` inputs.
pub const INPUT_FILM_GAMMA: &str = "gamma";
pub const INPUT_FILM_BETA: &str = "beta";
/// ERB gains output.
pub const OUTPUT_MASK: &str = "m";
/// Deep-filter coefficients output.
pub const OUTPUT_COEFS: &str = "coefs";
/// Local SNR output.
pub const OUTPUT_LSNR: &str = "lsnr";

/// RMS below which a hop is digital silence: the output is zero and neither the STFT nor the
/// model advances (the same shortcut as the virtual microphone's filter).
pub const DIGITAL_SILENCE_RMS: f32 = 1e-7;

const FEAT_SPEC_LEN: usize = 2 * DEFAULT_NB_DF;
const COEFS_LEN: usize = DEFAULT_NB_DF * DEFAULT_DF_ORDER * 2;

/// Name and shape of one graph port. `-1` marks a dimension the runtime could not make static.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorSpec {
    pub name: String,
    pub shape: Vec<i64>,
}

impl TensorSpec {
    #[must_use]
    pub fn new(name: impl Into<String>, shape: Vec<i64>) -> Self {
        Self {
            name: name.into(),
            shape,
        }
    }

    /// Number of values when every dimension is static.
    #[must_use]
    pub fn static_len(&self) -> Option<usize> {
        self.shape.iter().try_fold(1usize, |acc, &dim| {
            usize::try_from(dim)
                .ok()
                .and_then(|dim| acc.checked_mul(dim))
        })
    }
}

/// Every dynamic dimension (`None`, `-1`, a symbol) pinned to `1`: batch 1, `S = 1` hop.
#[must_use]
pub fn pin_dynamic_dims(dims: &[Option<i64>]) -> Vec<i64> {
    dims.iter()
        .map(|dim| match dim {
            Some(value) if *value > 0 => *value,
            _ => 1,
        })
        .collect()
}

/// One compiled graph of a runtime, with its inputs pinned to static shapes.
pub trait GraphSession: Send {
    /// Inputs in port order, with the static shapes the session was compiled for.
    fn inputs(&self) -> &[TensorSpec];
    /// Outputs in port order (shapes informative; the run reports the real sizes).
    fn outputs(&self) -> &[TensorSpec];
    /// Runs one hop. `inputs` follow [`Self::inputs`] (each slice holds exactly the static
    /// number of values); every `outputs[i]` is overwritten (resized as needed) with output `i`.
    fn run(&mut self, inputs: &[&[f32]], outputs: &mut [Vec<f32>]) -> Result<(), InferenceError>;
}

/// What conditions the loaded graphs on a voice profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditioningKind {
    /// `speaker_embedding` + `speaker_present` (pDFNet3 Pro v3 full).
    SpeakerEmbedding,
    /// Legacy `gamma`/`beta` `FiLM` vectors (pDFNet3 Pro v2/v3 `FiLM` exports).
    Film,
    /// No conditioning input: only the neutral profile is accepted.
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilmStage {
    Enc,
    Df,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Source {
    FeatErb,
    FeatSpec,
    Embedding,
    Present,
    Gamma(FilmStage),
    Beta(FilmStage),
    State(usize),
    Upstream { node: usize, output: usize },
}

struct Node<S> {
    name: String,
    session: S,
    sources: Vec<Source>,
    inputs: Vec<Vec<f32>>,
    outputs: Vec<Vec<f32>>,
}

struct StateSlot {
    node: usize,
    output: usize,
    value: Vec<f32>,
}

struct FilmBuffers {
    enc_gamma: Vec<f32>,
    enc_beta: Vec<f32>,
    df_gamma: Vec<f32>,
    df_beta: Vec<f32>,
}

impl FilmBuffers {
    fn identity() -> Self {
        Self {
            enc_gamma: vec![1.0; FILM_HIDDEN_DIM],
            enc_beta: vec![0.0; FILM_HIDDEN_DIM],
            df_gamma: vec![1.0; FILM_HIDDEN_DIM],
            df_beta: vec![0.0; FILM_HIDDEN_DIM],
        }
    }
}

fn contract(message: impl Into<String>) -> InferenceError {
    InferenceError::InputContract(message.into())
}

fn execution(message: impl Into<String>) -> InferenceError {
    InferenceError::InferenceExecution(message.into())
}

/// The state output paired with `input` of one graph: an explicit pair first, then
/// `<base>_in` ↔ `<base>_out`, then the legacy `<base>` ↔ `<base>_out`.
fn paired_output(
    input: &str,
    outputs: &[TensorSpec],
    explicit: &[(String, String)],
) -> Option<usize> {
    let position = |name: &str| outputs.iter().position(|o| o.name == name);
    if let Some(index) = explicit
        .iter()
        .find(|(state_in, _)| state_in == input)
        .and_then(|(_, state_out)| position(state_out))
    {
        return Some(index);
    }
    if let Some(base) = input.strip_suffix("_in")
        && let Some(index) = position(&format!("{base}_out"))
    {
        return Some(index);
    }
    position(&format!("{input}_out"))
}

/// Executes a set of stateful graphs hop by hop (see the module documentation).
pub struct StatefulGraphExecutor<S> {
    nodes: Vec<Node<S>>,
    states: Vec<StateSlot>,
    mask: (usize, usize),
    coefs: (usize, usize),
    lsnr: (usize, usize),
    kind: ConditioningKind,
    embedding: Vec<f32>,
    present: [f32; 1],
    film: FilmBuffers,
    frames: u64,
}

impl<S: GraphSession> StatefulGraphExecutor<S> {
    /// Resolves every input of `graphs` (name, session) and the execution order.
    /// `state_pairs` are the manifest's explicit `(input, output)` pairs (may be empty).
    #[allow(clippy::too_many_lines)] // linear pipeline; splitting it before the release risks behaviour changes
    pub fn new(
        graphs: Vec<(String, S)>,
        state_pairs: &[(String, String)],
    ) -> Result<Self, InferenceError> {
        if graphs.is_empty() {
            return Err(contract("stateful model has no graph"));
        }
        // Per graph: which outputs are state outputs (fed back) and which inputs are states.
        let mut state_outputs: Vec<Vec<bool>> = Vec::with_capacity(graphs.len());
        let mut state_inputs: Vec<Vec<Option<usize>>> = Vec::with_capacity(graphs.len());
        for (_, session) in &graphs {
            let outputs = session.outputs();
            let mut is_state_out = vec![false; outputs.len()];
            let pairs: Vec<Option<usize>> = session
                .inputs()
                .iter()
                .map(|input| {
                    let paired = paired_output(&input.name, outputs, state_pairs);
                    if let Some(index) = paired {
                        is_state_out[index] = true;
                    }
                    paired
                })
                .collect();
            state_outputs.push(is_state_out);
            state_inputs.push(pairs);
        }

        let mut film_seen = false;
        let mut embedding_seen = false;
        let mut order: Vec<usize> = Vec::with_capacity(graphs.len());
        let mut placed = vec![false; graphs.len()];
        let mut resolved: Vec<Option<Vec<Source>>> = vec![None; graphs.len()];
        let mut errors: Vec<String> = Vec::new();
        while order.len() < graphs.len() {
            let mut progressed = false;
            errors.clear();
            for (g, (graph_name, session)) in graphs.iter().enumerate() {
                if placed[g] {
                    continue;
                }
                let stage = if graph_name.starts_with("df") {
                    FilmStage::Df
                } else {
                    FilmStage::Enc
                };
                let mut sources = Vec::with_capacity(session.inputs().len());
                let mut missing = None;
                for (i, input) in session.inputs().iter().enumerate() {
                    let source = match input.name.as_str() {
                        INPUT_FEAT_ERB => Some(Source::FeatErb),
                        INPUT_FEAT_SPEC => Some(Source::FeatSpec),
                        INPUT_SPEAKER_EMBEDDING => Some(Source::Embedding),
                        INPUT_SPEAKER_PRESENT => Some(Source::Present),
                        INPUT_FILM_GAMMA => Some(Source::Gamma(stage)),
                        INPUT_FILM_BETA => Some(Source::Beta(stage)),
                        name => {
                            if state_inputs[g][i].is_some() {
                                // Placeholder; the slot index is assigned once the order is fixed.
                                Some(Source::State(usize::MAX))
                            } else {
                                // Producer: a non-state output of the same name of a placed graph.
                                let producers: Vec<(usize, usize)> = order
                                    .iter()
                                    .filter_map(|&p| {
                                        graphs[p].1.outputs().iter().enumerate().find_map(
                                            |(o, spec)| {
                                                (spec.name == name && !state_outputs[p][o])
                                                    .then_some((p, o))
                                            },
                                        )
                                    })
                                    .collect();
                                match producers.as_slice() {
                                    [(node, output)] => Some(Source::Upstream {
                                        node: *node,
                                        output: *output,
                                    }),
                                    [] => None,
                                    _ => {
                                        return Err(contract(format!(
                                            "input `{name}` of graph `{graph_name}` has several producers"
                                        )));
                                    }
                                }
                            }
                        }
                    };
                    if let Some(source) = source {
                        sources.push(source);
                    } else {
                        missing = Some(input.name.clone());
                        break;
                    }
                }
                if let Some(name) = missing {
                    errors.push(format!("`{name}` of graph `{graph_name}`"));
                    continue;
                }
                if !matches!(
                    (
                        sources.contains(&Source::Gamma(stage)),
                        sources.contains(&Source::Beta(stage))
                    ),
                    (true, true) | (false, false)
                ) {
                    return Err(contract(format!(
                        "graph `{graph_name}` declares only one of the FiLM inputs gamma/beta"
                    )));
                }
                film_seen |= sources.contains(&Source::Gamma(stage));
                embedding_seen |=
                    sources.contains(&Source::Embedding) || sources.contains(&Source::Present);
                resolved[g] = Some(sources);
                placed[g] = true;
                order.push(g);
                progressed = true;
            }
            if !progressed {
                return Err(contract(format!(
                    "stateful graph inputs without a producer: {}",
                    errors.join(", ")
                )));
            }
        }
        if film_seen && embedding_seen {
            return Err(contract(
                "graphs mix FiLM (gamma/beta) and speaker_embedding conditioning",
            ));
        }
        let kind = if embedding_seen {
            ConditioningKind::SpeakerEmbedding
        } else if film_seen {
            ConditioningKind::Film
        } else {
            ConditioningKind::None
        };

        // Re-index everything in execution order.
        let mut new_index = vec![0usize; graphs.len()];
        for (position, &g) in order.iter().enumerate() {
            new_index[g] = position;
        }
        let mut slots: Vec<Option<(String, S)>> = graphs.into_iter().map(Some).collect();
        let mut nodes = Vec::with_capacity(order.len());
        let mut states = Vec::new();
        for &g in &order {
            let (name, session) = slots[g]
                .take()
                .ok_or_else(|| contract("graph placed twice"))?;
            let mut sources = resolved[g].take().unwrap_or_default();
            let mut inputs = Vec::with_capacity(sources.len());
            for (i, source) in sources.iter_mut().enumerate() {
                let spec = &session.inputs()[i];
                let len = spec.static_len().ok_or_else(|| {
                    contract(format!(
                        "input `{}` of graph `{name}` has no static shape {:?}",
                        spec.name, spec.shape
                    ))
                })?;
                let expected = match source {
                    Source::FeatErb => Some(DEFAULT_NB_ERB),
                    Source::FeatSpec => Some(FEAT_SPEC_LEN),
                    Source::Embedding => Some(SPEAKER_EMBEDDING_DIM),
                    Source::Present => Some(1),
                    Source::Gamma(_) | Source::Beta(_) => Some(FILM_HIDDEN_DIM),
                    Source::State(slot) => {
                        let output = state_inputs[g][i].unwrap_or_default();
                        *slot = states.len();
                        states.push(StateSlot {
                            node: nodes.len(),
                            output,
                            value: vec![0.0; len],
                        });
                        None
                    }
                    Source::Upstream { node, .. } => {
                        *node = new_index[*node];
                        None
                    }
                };
                if let Some(expected) = expected
                    && expected != len
                {
                    return Err(contract(format!(
                        "input `{}` of graph `{name}` holds {len} values, the contract says {expected}",
                        spec.name
                    )));
                }
                inputs.push(vec![0.0; len]);
            }
            let outputs = vec![Vec::new(); session.outputs().len()];
            nodes.push(Node {
                name,
                session,
                sources,
                inputs,
                outputs,
            });
        }
        let find_output = |wanted: &str| -> Result<(usize, usize), InferenceError> {
            let found: Vec<(usize, usize)> = nodes
                .iter()
                .enumerate()
                .flat_map(|(n, node)| {
                    node.session
                        .outputs()
                        .iter()
                        .enumerate()
                        .filter(move |(_, spec)| spec.name == wanted)
                        .map(move |(o, _)| (n, o))
                })
                .collect();
            match found.as_slice() {
                [one] => Ok(*one),
                [] => Err(contract(format!("no graph outputs `{wanted}`"))),
                _ => Err(contract(format!("several graphs output `{wanted}`"))),
            }
        };
        let mask = find_output(OUTPUT_MASK)?;
        let coefs = find_output(OUTPUT_COEFS)?;
        let lsnr = find_output(OUTPUT_LSNR)?;
        Ok(Self {
            nodes,
            states,
            mask,
            coefs,
            lsnr,
            kind,
            embedding: vec![0.0; SPEAKER_EMBEDDING_DIM],
            present: [0.0],
            film: FilmBuffers::identity(),
            frames: 0,
        })
    }

    /// How the loaded graphs are conditioned on a voice profile.
    #[must_use]
    pub const fn conditioning_kind(&self) -> ConditioningKind {
        self.kind
    }

    /// Graph names in execution order.
    #[must_use]
    pub fn graph_names(&self) -> Vec<&str> {
        self.nodes.iter().map(|node| node.name.as_str()).collect()
    }

    /// Number of fed-back state tensors.
    #[must_use]
    pub const fn state_count(&self) -> usize {
        self.states.len()
    }

    /// Hops run since the last reset.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.frames
    }

    /// Feeds `speaker_embedding` and `speaker_present` (the raw contract; the profile API is
    /// [`StatefulGraphBackend::set_voice_profile`]). `None` writes zeros. Graphs without these
    /// inputs reject anything but `(None, 0.0)`.
    pub fn set_speaker_conditioning(
        &mut self,
        embedding: Option<&[f32]>,
        present: f32,
    ) -> Result<(), InferenceError> {
        if self.kind != ConditioningKind::SpeakerEmbedding {
            if embedding.is_none() && present == 0.0 {
                return Ok(());
            }
            return Err(InferenceError::UnsupportedFeature(
                "the loaded graphs have no speaker_embedding input".to_owned(),
            ));
        }
        if !present.is_finite() || !(0.0..=1.0).contains(&present) {
            return Err(contract("speaker_present must be within [0, 1]"));
        }
        match embedding {
            Some(values) => {
                if values.len() != SPEAKER_EMBEDDING_DIM || values.iter().any(|v| !v.is_finite()) {
                    return Err(contract(format!(
                        "speaker embedding needs {SPEAKER_EMBEDDING_DIM} finite values"
                    )));
                }
                self.embedding.copy_from_slice(values);
            }
            None => self.embedding.fill(0.0),
        }
        self.present = [present];
        Ok(())
    }

    /// Feeds the legacy `FiLM` vectors (identity with `None`).
    pub fn set_film(&mut self, film: Option<&FiLMVectors>) -> Result<(), InferenceError> {
        if self.kind != ConditioningKind::Film {
            if film.is_none_or(FiLMVectors::is_identity) {
                return Ok(());
            }
            return Err(InferenceError::UnsupportedFeature(
                "the loaded graphs have no FiLM (gamma/beta) inputs".to_owned(),
            ));
        }
        match film {
            Some(vectors) => {
                vectors.validate()?;
                self.film = FilmBuffers {
                    enc_gamma: vectors.gamma_enc.clone(),
                    enc_beta: vectors.beta_enc.clone(),
                    df_gamma: vectors.gamma_df.clone(),
                    df_beta: vectors.beta_df.clone(),
                };
            }
            None => self.film = FilmBuffers::identity(),
        }
        Ok(())
    }

    /// Zeroes every recurrent state (conditioning is kept).
    pub fn reset(&mut self) {
        for state in &mut self.states {
            state.value.fill(0.0);
        }
        self.frames = 0;
    }

    /// Runs every graph for one hop and commits the new state only when all succeeded.
    pub fn run_hop(
        &mut self,
        feat_erb: &[f32; DEFAULT_NB_ERB],
        feat_spec: &[[f32; DEFAULT_NB_DF]; 2],
    ) -> Result<ModelFrameOutput, InferenceError> {
        if feat_erb
            .iter()
            .chain(feat_spec.iter().flatten())
            .any(|value| !value.is_finite())
        {
            return Err(contract("feature frame contains a non-finite value"));
        }
        for index in 0..self.nodes.len() {
            let (done, rest) = self.nodes.split_at_mut(index);
            let node = &mut rest[0];
            for (buffer, source) in node.inputs.iter_mut().zip(&node.sources) {
                match source {
                    Source::FeatErb => buffer.copy_from_slice(feat_erb),
                    Source::FeatSpec => {
                        buffer[..DEFAULT_NB_DF].copy_from_slice(&feat_spec[0]);
                        buffer[DEFAULT_NB_DF..].copy_from_slice(&feat_spec[1]);
                    }
                    Source::Embedding => buffer.copy_from_slice(&self.embedding),
                    Source::Present => buffer.copy_from_slice(&self.present),
                    Source::Gamma(FilmStage::Enc) => buffer.copy_from_slice(&self.film.enc_gamma),
                    Source::Beta(FilmStage::Enc) => buffer.copy_from_slice(&self.film.enc_beta),
                    Source::Gamma(FilmStage::Df) => buffer.copy_from_slice(&self.film.df_gamma),
                    Source::Beta(FilmStage::Df) => buffer.copy_from_slice(&self.film.df_beta),
                    Source::State(slot) => buffer.copy_from_slice(&self.states[*slot].value),
                    Source::Upstream { node, output } => {
                        let value = &done[*node].outputs[*output];
                        if value.len() != buffer.len() {
                            return Err(execution(format!(
                                "`{}` produced {} values for an input of {}",
                                done[*node].session.outputs()[*output].name,
                                value.len(),
                                buffer.len()
                            )));
                        }
                        buffer.copy_from_slice(value);
                    }
                }
            }
            let refs: Vec<&[f32]> = node.inputs.iter().map(Vec::as_slice).collect();
            node.session.run(&refs, &mut node.outputs)?;
        }
        let output = |(node, index): (usize, usize)| self.nodes[node].outputs[index].as_slice();
        let mask = output(self.mask);
        let coefs = output(self.coefs);
        let lsnr = output(self.lsnr);
        if mask.len() != DEFAULT_NB_ERB || coefs.len() != COEFS_LEN || lsnr.len() != 1 {
            return Err(execution(format!(
                "model outputs m/coefs/lsnr hold {}/{}/{} values, expected {DEFAULT_NB_ERB}/{COEFS_LEN}/1",
                mask.len(),
                coefs.len(),
                lsnr.len()
            )));
        }
        if mask
            .iter()
            .chain(coefs)
            .chain(lsnr)
            .any(|value| !value.is_finite())
        {
            return Err(InferenceError::NonFiniteOutput);
        }
        let mut erb_gains = [0.0f32; DEFAULT_NB_ERB];
        erb_gains.copy_from_slice(mask);
        let df_coefs: Vec<[Complex32; DEFAULT_DF_ORDER]> = coefs
            .chunks_exact(DEFAULT_DF_ORDER * 2)
            .map(|bin| std::array::from_fn(|tap| Complex32::new(bin[2 * tap], bin[2 * tap + 1])))
            .collect();
        let prediction = ModelFrameOutput {
            lsnr: lsnr[0],
            erb_gains: Some(erb_gains),
            df_coefs: Some(df_coefs),
        };
        // Commit: every graph succeeded.
        for state in &self.states {
            let produced = &self.nodes[state.node].outputs[state.output];
            if produced.len() != state.value.len() {
                return Err(execution(format!(
                    "state output `{}` holds {} values, its input {}",
                    self.nodes[state.node].session.outputs()[state.output].name,
                    produced.len(),
                    state.value.len()
                )));
            }
        }
        for state in &mut self.states {
            state
                .value
                .copy_from_slice(&self.nodes[state.node].outputs[state.output]);
        }
        self.frames = self.frames.saturating_add(1);
        Ok(prediction)
    }

    /// Fails unless every `(graph, input, producer, output)` of the manifest's routing is the
    /// producer the name resolution chose (the manifest and the graphs must agree).
    pub fn check_routing(
        &self,
        routing: &[(String, String, String, String)],
    ) -> Result<(), InferenceError> {
        for (graph, input, producer, output) in routing {
            let resolved = self
                .nodes
                .iter()
                .find(|node| &node.name == graph)
                .and_then(|node| {
                    let index = node
                        .session
                        .inputs()
                        .iter()
                        .position(|i| &i.name == input)?;
                    match node.sources[index] {
                        Source::Upstream { node: p, output: o } => Some((
                            self.nodes[p].name.as_str(),
                            self.nodes[p].session.outputs()[o].name.as_str(),
                        )),
                        _ => None,
                    }
                });
            if resolved != Some((producer.as_str(), output.as_str())) {
                return Err(contract(format!(
                    "manifest routes `{graph}.{input}` from `{producer}.{output}`, the graphs do not"
                )));
            }
        }
        Ok(())
    }

    /// One-line description of the resolved graphs (diagnostics).
    #[must_use]
    pub fn describe(&self) -> String {
        let mut text = String::new();
        for node in &self.nodes {
            let _ = write!(
                text,
                "{}({} in/{} out) ",
                node.name,
                node.inputs.len(),
                node.outputs.len()
            );
        }
        let _ = write!(
            text,
            "states={} conditioning={:?}",
            self.states.len(),
            self.kind
        );
        text
    }
}

/// [`StatefulGraphExecutor`] as the neural half of the shared [`DspPipeline`].
pub struct StatefulSpectralModel<S> {
    executor: StatefulGraphExecutor<S>,
    descriptor: BackendDescriptor,
}

impl<S: GraphSession> StatefulSpectralModel<S> {
    #[must_use]
    pub const fn new(executor: StatefulGraphExecutor<S>, descriptor: BackendDescriptor) -> Self {
        Self {
            executor,
            descriptor,
        }
    }

    #[must_use]
    pub const fn executor(&self) -> &StatefulGraphExecutor<S> {
        &self.executor
    }

    pub const fn executor_mut(&mut self) -> &mut StatefulGraphExecutor<S> {
        &mut self.executor
    }
}

impl<S: GraphSession> SpectralModelBackend for StatefulSpectralModel<S> {
    fn descriptor(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn process_spectrum(
        &mut self,
        features: &ExtractedFeatures,
    ) -> Result<ModelFrameOutput, InferenceError> {
        self.executor
            .run_hop(&features.feat_erb, &features.feat_spec)
    }

    fn reset(&mut self) -> Result<(), InferenceError> {
        self.executor.reset();
        Ok(())
    }
}

/// The resolved conditioning, validated before anything is applied.
struct ResolvedConditioning {
    embedding: Option<Vec<f32>>,
    present: f32,
    film: Option<FiLMVectors>,
    eq_factors: Option<Vec<f32>>,
}

/// A complete [`InferenceBackend`]: [`DspPipeline`] + [`StatefulGraphExecutor`] on any runtime.
///
/// Adds the speaker conditioning (Pro v3 full embedding, or legacy `FiLM` profile), the profile's
/// spectral EQ applied right before the iSTFT, and the digital-silence shortcut.
pub struct StatefulGraphBackend<S> {
    pipeline: DspPipeline,
    model: StatefulSpectralModel<S>,
    active_profile: Option<VoiceProfile>,
    active_conditioning: Option<SpeakerConditioning>,
}

impl<S: GraphSession> StatefulGraphBackend<S> {
    /// Wraps `executor` with the default DSP configuration (that of libDF's `DfTract`).
    pub fn new(
        executor: StatefulGraphExecutor<S>,
        descriptor: BackendDescriptor,
    ) -> Result<Self, InferenceError> {
        Ok(Self {
            pipeline: DspPipeline::new(DspPipelineConfig::default())?,
            model: StatefulSpectralModel::new(executor, descriptor),
            active_profile: None,
            active_conditioning: None,
        })
    }

    #[must_use]
    pub const fn executor(&self) -> &StatefulGraphExecutor<S> {
        self.model.executor()
    }

    /// Raw access for tests and diagnostics (bypasses the profile bookkeeping).
    pub const fn executor_mut(&mut self) -> &mut StatefulGraphExecutor<S> {
        self.model.executor_mut()
    }

    /// The legacy (`FiLM`) profile applied by [`Self::set_voice_profile`], if any.
    #[must_use]
    pub const fn active_voice_profile(&self) -> Option<&VoiceProfile> {
        self.active_profile.as_ref()
    }

    /// The speaker conditioning applied by [`Self::set_speaker_conditioning`], if any.
    #[must_use]
    pub const fn active_speaker_conditioning(&self) -> Option<&SpeakerConditioning> {
        self.active_conditioning.as_ref()
    }

    /// Whether the graphs take a speaker conditioning (embedding or legacy `FiLM`).
    #[must_use]
    pub fn supports_speaker_conditioning(&self) -> bool {
        self.model.executor().conditioning_kind() != ConditioningKind::None
    }

    /// Whether the graphs take the Pro v3 full `speaker_embedding`.
    #[must_use]
    pub fn supports_speaker_embedding(&self) -> bool {
        self.model.executor().conditioning_kind() == ConditioningKind::SpeakerEmbedding
    }

    /// Zeroes the DSP and model state (the conditioning stays applied).
    pub fn reset(&mut self) -> Result<(), InferenceError> {
        self.pipeline.reset();
        self.model.reset()
    }

    fn eq_factors(&self, eq: Option<&BandGains>) -> Result<Option<Vec<f32>>, InferenceError> {
        match eq {
            Some(gains) if !gains.is_neutral() => Ok(Some(spectral_eq::bin_factors(
                gains,
                self.pipeline.erb_band_widths(),
            )?)),
            Some(gains) => {
                gains.validate()?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    const fn neutral(eq_factors: Option<Vec<f32>>) -> ResolvedConditioning {
        ResolvedConditioning {
            embedding: None,
            present: 0.0,
            film: None,
            eq_factors,
        }
    }

    fn resolve_profile(
        &self,
        profile: Option<&VoiceProfile>,
    ) -> Result<ResolvedConditioning, InferenceError> {
        let Some(profile) = profile else {
            return Ok(Self::neutral(None));
        };
        profile.verify_integrity()?;
        let eq_factors = self.eq_factors(profile.eq.as_ref())?;
        if profile.film.is_identity() {
            return Ok(Self::neutral(eq_factors));
        }
        match self.model.executor().conditioning_kind() {
            ConditioningKind::Film => Ok(ResolvedConditioning {
                film: Some(profile.film.clone()),
                ..Self::neutral(eq_factors)
            }),
            ConditioningKind::SpeakerEmbedding => Err(InferenceError::UnsupportedFeature(
                "voice profile carries FiLM vectors; this model takes a speaker embedding \
                 (enroll again)"
                    .to_owned(),
            )),
            ConditioningKind::None => Err(InferenceError::UnsupportedFeature(
                "voice profile conditioning".to_owned(),
            )),
        }
    }

    fn apply(&mut self, resolved: ResolvedConditioning) -> Result<(), InferenceError> {
        let executor = self.model.executor_mut();
        match executor.conditioning_kind() {
            ConditioningKind::SpeakerEmbedding => {
                executor
                    .set_speaker_conditioning(resolved.embedding.as_deref(), resolved.present)?;
            }
            ConditioningKind::Film => executor.set_film(resolved.film.as_ref())?,
            ConditioningKind::None => {}
        }
        self.pipeline.set_spectral_eq_factors(resolved.eq_factors)
    }

    /// Sets (or clears) the spectral EQ directly, without changing the active conditioning; the
    /// next profile/conditioning call replaces it.
    pub fn set_spectral_eq(&mut self, eq: Option<&BandGains>) -> Result<(), InferenceError> {
        let factors = self.eq_factors(eq)?;
        self.pipeline.set_spectral_eq_factors(factors)
    }

    /// Activates a legacy (`FiLM`) `profile`, or removes any conditioning with `None`, between two
    /// hops. On a Pro v3 full graph only a neutral-`FiLM` profile (EQ only) is accepted. Everything
    /// is validated first: on error the previous conditioning stays active. State is preserved.
    pub fn set_voice_profile(
        &mut self,
        profile: Option<&VoiceProfile>,
    ) -> Result<(), InferenceError> {
        let resolved = self.resolve_profile(profile)?;
        self.apply(resolved)?;
        self.active_profile = profile.cloned();
        self.active_conditioning = None;
        Ok(())
    }

    /// Conditions the Pro v3 full graph on a speaker (`speaker_embedding`, `speaker_present` 1.0,
    /// or 0.0 at intensity 0 — binary, never fractional — and the profile's EQ), or returns to the neutral voice with `None` (zeros,
    /// `0.0`, no EQ). Validated first; on error the previous conditioning stays. State is kept.
    pub fn set_speaker_conditioning(
        &mut self,
        conditioning: Option<&SpeakerConditioning>,
    ) -> Result<(), InferenceError> {
        let resolved = match conditioning {
            None => Self::neutral(None),
            Some(conditioning) => {
                if !self.supports_speaker_embedding() {
                    return Err(InferenceError::UnsupportedFeature(
                        "the loaded model has no speaker_embedding input".to_owned(),
                    ));
                }
                conditioning.validate()?;
                ResolvedConditioning {
                    embedding: Some(conditioning.speaker_embedding.to_vec()),
                    present: conditioning.speaker_present(),
                    film: None,
                    eq_factors: self.eq_factors(conditioning.eq.as_ref())?,
                }
            }
        };
        self.apply(resolved)?;
        self.active_conditioning = conditioning.cloned();
        self.active_profile = None;
        Ok(())
    }
}

/// Root mean square of one hop.
#[must_use]
pub fn frame_rms(frame: &AudioFrame) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let len = frame.len() as f32;
    (frame.iter().map(|s| s * s).sum::<f32>() / len).sqrt()
}

impl<S: GraphSession> InferenceBackend for StatefulGraphBackend<S> {
    fn descriptor(&self) -> BackendDescriptor {
        self.model.descriptor()
    }

    fn process(&mut self, input: &AudioFrame) -> Result<ProcessedFrame, InferenceError> {
        if input.iter().any(|sample| !sample.is_finite()) {
            return Err(contract("input frame contains a non-finite sample"));
        }
        let samples = if frame_rms(input) < DIGITAL_SILENCE_RMS {
            [0.0; HOP_SAMPLES]
        } else {
            self.pipeline.process_frame(input, &mut self.model)?
        };
        ProcessedFrame::checked(samples, ALGORITHM_LATENCY_SAMPLES, self.descriptor())
    }

    fn algorithmic_latency_samples(&self) -> u32 {
        ALGORITHM_LATENCY_SAMPLES
    }

    fn set_voice_profile(&mut self, profile: Option<&VoiceProfile>) -> Result<(), InferenceError> {
        Self::set_voice_profile(self, profile)
    }

    fn supports_voice_profile(&self) -> bool {
        self.supports_speaker_conditioning()
    }

    fn set_speaker_conditioning(
        &mut self,
        conditioning: Option<&SpeakerConditioning>,
    ) -> Result<(), InferenceError> {
        Self::set_speaker_conditioning(self, conditioning)
    }

    fn supports_speaker_embedding(&self) -> bool {
        Self::supports_speaker_embedding(self)
    }
}

/// Checks the hop and sample rate a manifest declares against the frozen DSP contract.
pub fn check_stream_contract(hop: usize, sample_rate: u32) -> Result<(), InferenceError> {
    if hop != HOP_SAMPLES || sample_rate != SAMPLE_RATE_HZ {
        return Err(contract(format!(
            "model declares hop {hop} at {sample_rate} Hz, the DSP contract is {HOP_SAMPLES} at {SAMPLE_RATE_HZ} Hz"
        )));
    }
    Ok(())
}

/// Compiles every graph of a verified manifest export with `load` (a runtime's
/// [`GraphSession`] constructor) and resolves the executor; checks the stream contract and the
/// manifest routing.
pub fn build_executor<S, F>(
    files: &crate::stateful_asset::StatefulModelFiles,
    mut load: F,
) -> Result<StatefulGraphExecutor<S>, InferenceError>
where
    S: GraphSession,
    F: FnMut(&str, &[u8]) -> Result<S, InferenceError>,
{
    check_stream_contract(files.manifest.hop, files.manifest.sample_rate)?;
    let graphs = files
        .graphs
        .iter()
        .map(|(name, bytes)| Ok((name.clone(), load(name, bytes)?)))
        .collect::<Result<Vec<_>, InferenceError>>()?;
    let executor = StatefulGraphExecutor::new(graphs, &files.manifest.state_pairs)?;
    executor.check_routing(&files.manifest.routing)?;
    Ok(executor)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    type RunFn = Box<dyn FnMut(&[&[f32]], &mut [Vec<f32>]) + Send>;

    struct Mock {
        inputs: Vec<TensorSpec>,
        outputs: Vec<TensorSpec>,
        body: RunFn,
        runs: Arc<AtomicUsize>,
    }

    impl GraphSession for Mock {
        fn inputs(&self) -> &[TensorSpec] {
            &self.inputs
        }
        fn outputs(&self) -> &[TensorSpec] {
            &self.outputs
        }
        fn run(
            &mut self,
            inputs: &[&[f32]],
            outputs: &mut [Vec<f32>],
        ) -> Result<(), InferenceError> {
            self.runs.fetch_add(1, Ordering::SeqCst);
            (self.body)(inputs, outputs);
            Ok(())
        }
    }

    fn spec(name: &str, shape: &[i64]) -> TensorSpec {
        TensorSpec::new(name, shape.to_vec())
    }

    fn descriptor() -> BackendDescriptor {
        BackendDescriptor {
            backend: "mock",
            backend_version: "1",
            runtime: "mock",
            runtime_version: "1",
            asset_id: "mock".to_owned(),
            asset_sha256: "0".repeat(64),
            cpu_profile: "any",
        }
    }

    /// A split export like the Pro v3 full: `enc` (state `h`, outputs `emb`, `lsnr`) and a decoder
    /// `dec` (state `d`, input `emb`, outputs `m`, `coefs`). `lsnr` is fixed; `h`/`d` count hops.
    #[allow(clippy::suboptimal_flops)] // keep the reference arithmetic; `mul_add` changes rounding in the mock graph
    fn split(lsnr: f32, runs: &Arc<AtomicUsize>, decoder_first: bool) -> Vec<(String, Mock)> {
        let enc = Mock {
            inputs: vec![
                spec("feat_erb", &[1, 1, 1, 32]),
                spec("feat_spec", &[1, 2, 1, 96]),
                spec("speaker_embedding", &[1, 192]),
                spec("speaker_present", &[1]),
                spec("h_in", &[1, 4]),
            ],
            outputs: vec![
                spec("emb", &[1, 8]),
                spec("lsnr", &[1, 1, 1]),
                spec("h_out", &[1, 4]),
            ],
            body: Box::new(move |inputs, outputs| {
                let present = inputs[3][0];
                outputs[0] = vec![inputs[0][0] + present * inputs[2][0]; 8];
                outputs[1] = vec![lsnr];
                outputs[2] = inputs[4].iter().map(|h| h + 1.0).collect();
            }),
            runs: Arc::clone(runs),
        };
        let dec = Mock {
            inputs: vec![spec("emb", &[1, 8]), spec("d_in", &[2])],
            outputs: vec![
                spec("m", &[1, 1, 1, 32]),
                spec("coefs", &[1, 1, 96, 10]),
                spec("d_out", &[2]),
            ],
            body: Box::new(|inputs, outputs| {
                outputs[0] = vec![0.5; 32];
                outputs[1] = vec![0.0; 960];
                outputs[2] = inputs[1].iter().map(|d| d + 1.0).collect();
            }),
            runs: Arc::clone(runs),
        };
        if decoder_first {
            vec![("dec".to_owned(), dec), ("enc".to_owned(), enc)]
        } else {
            vec![("enc".to_owned(), enc), ("dec".to_owned(), dec)]
        }
    }

    #[test]
    fn graphs_are_ordered_by_data_flow_and_states_paired_by_name() {
        let runs = Arc::new(AtomicUsize::new(0));
        let executor = StatefulGraphExecutor::new(split(0.0, &runs, true), &[]).unwrap();
        assert_eq!(executor.graph_names(), vec!["enc", "dec"]);
        assert_eq!(executor.state_count(), 2);
        assert_eq!(
            executor.conditioning_kind(),
            ConditioningKind::SpeakerEmbedding
        );
        executor
            .check_routing(&[(
                "dec".to_owned(),
                "emb".to_owned(),
                "enc".to_owned(),
                "emb".to_owned(),
            )])
            .unwrap();
        assert!(
            executor
                .check_routing(&[(
                    "dec".to_owned(),
                    "emb".to_owned(),
                    "enc".to_owned(),
                    "lsnr".to_owned(),
                )])
                .is_err()
        );
    }

    #[test]
    fn unfed_state_and_missing_producer_fail_closed() {
        let runs = Arc::new(AtomicUsize::new(0));
        let mut graphs = split(0.0, &runs, false);
        graphs[0].1.outputs.pop(); // `h_out` gone: `h_in` has no pair and no producer.
        let error = StatefulGraphExecutor::new(graphs, &[]).err().unwrap();
        assert!(error.to_string().contains("h_in"), "{error}");
        let mut graphs = split(0.0, &runs, false);
        graphs.remove(0); // no encoder: `emb` has no producer.
        assert!(StatefulGraphExecutor::new(graphs, &[]).is_err());
    }

    #[test]
    fn every_graph_runs_and_every_state_advances_on_every_hop_whatever_the_lsnr() {
        for lsnr in [-20.0, 35.0, 25.0, 10.0] {
            let runs = Arc::new(AtomicUsize::new(0));
            let mut executor = StatefulGraphExecutor::new(split(lsnr, &runs, false), &[]).unwrap();
            for _ in 0..5 {
                executor.run_hop(&[0.0; 32], &[[0.0; 96]; 2]).unwrap();
            }
            assert_eq!(runs.load(Ordering::SeqCst), 10, "lsnr {lsnr}");
            assert!(
                executor
                    .states
                    .iter()
                    .all(|s| s.value.iter().all(|&v| v == 5.0))
            );
        }
    }

    /// The stage selection replicates `emulate_gating_offline`
    /// (`clearcore-train/src/cctrain/eval/runtime_path.py:342-360`, thresholds
    /// `DEFAULT_THRESHOLDS = (-10, 30, 20)` at line 36): `lsnr < -10` → ZERO, `lsnr > 30` → the
    /// unprocessed spectrum, `20 < lsnr <= 30` → ERB mask only, otherwise ERB + DF.
    #[test]
    fn stage_selection_matches_emulate_gating_offline() {
        let pipeline = DspPipeline::new(DspPipelineConfig::default()).unwrap();
        // (apply_gains, apply_gain_zeros, apply_df)
        assert_eq!(pipeline.apply_stages(-20.0), (false, true, false)); // ZERO
        assert_eq!(pipeline.apply_stages(35.0), (false, false, false)); // NONE (noisy)
        assert_eq!(pipeline.apply_stages(25.0), (true, false, false)); // ERB_ONLY
        assert_eq!(pipeline.apply_stages(10.0), (true, false, true)); // ERB_DF
        // Boundaries exactly as the Python comparisons (`<` lo, `>` hi_erb, `>` hi_df).
        assert_eq!(pipeline.apply_stages(-10.0), (true, false, true));
        assert_eq!(pipeline.apply_stages(30.0), (true, false, false));
        assert_eq!(pipeline.apply_stages(20.0), (true, false, true));
    }

    fn tone(frame: usize) -> AudioFrame {
        #[allow(clippy::cast_precision_loss)]
        std::array::from_fn(|i| {
            0.3 * (2.0 * std::f32::consts::PI * 300.0 * (frame * HOP_SAMPLES + i) as f32 / 48_000.0)
                .sin()
        })
    }

    #[test]
    fn zero_stage_silences_the_output_while_the_model_keeps_running() {
        let runs = Arc::new(AtomicUsize::new(0));
        let executor = StatefulGraphExecutor::new(split(-20.0, &runs, false), &[]).unwrap();
        let mut backend = StatefulGraphBackend::new(executor, descriptor()).unwrap();
        let mut energy = 0.0f32;
        for frame in 0..20 {
            let out = backend.process(&tone(frame)).unwrap();
            energy += out.samples.iter().map(|s| s * s).sum::<f32>();
        }
        assert!(energy < 1e-12, "{energy}");
        assert_eq!(runs.load(Ordering::SeqCst), 40);
        // NONE stage (lsnr > 30): the input comes back (delayed by the algorithmic latency).
        let runs = Arc::new(AtomicUsize::new(0));
        let executor = StatefulGraphExecutor::new(split(35.0, &runs, false), &[]).unwrap();
        let mut backend = StatefulGraphBackend::new(executor, descriptor()).unwrap();
        let output: Vec<f32> = (0..20)
            .flat_map(|frame| backend.process(&tone(frame)).unwrap().samples)
            .collect();
        let input: Vec<f32> = (0..20).flat_map(tone).collect();
        let delay = ALGORITHM_LATENCY_SAMPLES as usize;
        let error: f32 = output[delay..]
            .iter()
            .zip(&input)
            .map(|(o, i)| (o - i).powi(2))
            .sum();
        assert!(error < 1e-6, "{error}");
    }
}
