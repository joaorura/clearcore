//! pDFNet3 Pro v3 FULL (speaker memory + cross-attention), driven by its `manifest.json`.
//!
//! The asset is a `.tar.gz` (whole-file SHA-256 pinned) with `manifest.json`, the graphs it lists
//! under `files` (each with its own SHA-256 and size) and optionally `config.ini`. The manifest
//! (`format_version` 1) declares:
//!
//! - `graphs`: `{name: {file, inputs: {name: {shape}}, outputs: {name: {shape}},
//!   state_pairs: [{in, out, shape, init: "zeros"}]}}` — every graph is stateful with `S = 1`;
//! - `routing`: `{graph: {input: "<graph>.<output>" | "runtime"}}` — an input of a decoder takes
//!   the output of the same name of the encoder; graphs are run in dependency order;
//! - `hop` 480, `sample_rate` 48000.
//!
//! Runtime inputs (fed by the filter): `feat_erb` `[1,1,1,32]`, `feat_spec` `[1,2,1,96]`,
//! `speaker_embedding` `[1,192]` (ECAPA, L2-normalised; zeros without a profile) and
//! `speaker_present` `[1]` (1.0 / 0.0). Required outputs (any graph): `lsnr`, the ERB mask `m`
//! (32 values) and `coefs` (`[1,1,96,10]`, read as 96 bins x 5 taps x re/im). Any input that is
//! neither a runtime input, a routed output nor the `in` side of a state pair fails closed.
//!
//! Every graph runs on every hop and every state advances (decision of 2026-11-09); the local SNR
//! only selects the output per hop in the DSP pipeline (see `crate::core`).

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::Path;

use deep_filter::Complex32;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use realtime_noise_model::dsp_pipeline::{DEFAULT_DF_ORDER, DEFAULT_NB_DF, DEFAULT_NB_ERB};
use realtime_noise_model::{ExtractedFeatures, ModelFrameOutput};
use realtime_noise_runtime_openvino::model::OpenVinoCompiledModel;
use realtime_noise_runtime_openvino::{
    OpenVinoCore, OpenVinoInferRequest, OpenVinoTensor, precision_for_device,
};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::engine::{Conditioning, Engine, SPEAKER_EMBEDDING_DIM};

/// `model` identifier the manifest must declare.
pub const V3_MODEL_ID: &str = "pdfnet3-pro-v3-full";
/// File name of the bundled v3 archive under `<models>/pdfnet3/`.
pub const V3_BUNDLED_FILE: &str = "pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz";
/// Whole-archive SHA-256 of the v3 FULL export handed off on 2026-11-09 (enc/erb_dec/df_dec,
/// opset 12, `S = 1`; second hand-off, tract-compatible enc/df_dec).
pub const V3_BUNDLED_SHA256: &str =
    "c1bee5593870bc326913e805803dbcfb19632236f0f42163635a5cc502d256ee";
/// Development override (absolute path of a v3 archive) and its whole-archive SHA-256.
pub const ENV_DEV_V3_ASSET: &str = "CLEARCORE_DEV_PDFNET3_V3_ASSET";
pub const ENV_DEV_V3_SHA256: &str = "CLEARCORE_DEV_PDFNET3_V3_SHA256";

const MAX_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_MEMBER_BYTES: u64 = 128 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

pub const IN_FEAT_ERB: &str = "feat_erb";
pub const IN_FEAT_SPEC: &str = "feat_spec";
pub const IN_EMBEDDING: &str = "speaker_embedding";
pub const IN_PRESENT: &str = "speaker_present";
pub const OUT_LSNR: &str = "lsnr";
pub const OUT_MASK: &str = "m";
pub const OUT_COEFS: &str = "coefs";
const DF_COEFS_LEN: usize = DEFAULT_NB_DF * DEFAULT_DF_ORDER * 2;

#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

fn element_count(shape: &[i64]) -> Result<usize, String> {
    usize::try_from(shape.iter().product::<i64>()).map_err(|_| format!("bad shape {shape:?}"))
}

/// Shape of a manifest tensor entry, every dynamic dimension (`-1`, `0`, symbolic) pinned to 1
/// (`S = 1`, batch 1).
fn shape_of(entry: &Value, name: &str) -> Result<Vec<i64>, String> {
    let dims = entry
        .get("shape")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("`{name}` has no shape"))?;
    if dims.is_empty() {
        return Err(format!("`{name}` has an empty shape"));
    }
    if let Some(dtype) = entry.get("dtype").and_then(Value::as_str)
        && !matches!(dtype, "float32" | "f32" | "float")
    {
        return Err(format!("`{name}`: dtype {dtype} is not float32"));
    }
    Ok(dims
        .iter()
        .map(|d| d.as_i64().filter(|v| *v > 0).unwrap_or(1))
        .collect())
}

/// Where one graph input comes from on each hop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    FeatErb,
    FeatSpec,
    Embedding,
    Present,
    /// Output `fetch` of graph `graph` (an earlier graph in the plan) in this hop.
    Route {
        graph: usize,
        fetch: usize,
    },
    /// Committed recurrent state `index` (global over the plan).
    State(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputPlan {
    pub name: String,
    pub shape: Vec<i64>,
    pub len: usize,
    pub feed: Feed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FetchPlan {
    pub name: String,
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePlan {
    pub graph: usize,
    pub input: String,
    pub output: String,
    /// Index of the `output` in the graph's fetch list.
    pub fetch: usize,
    pub len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPlan {
    pub name: String,
    pub file: String,
    pub inputs: Vec<InputPlan>,
    pub fetch: Vec<FetchPlan>,
}

/// Execution plan of a v3 asset: graphs in dependency order, every feed resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3Plan {
    pub graphs: Vec<GraphPlan>,
    pub states: Vec<StatePlan>,
    /// `(graph, fetch)` of `lsnr`, `m` and `coefs`.
    pub lsnr: (usize, usize),
    pub mask: (usize, usize),
    pub coefs: (usize, usize),
}

struct RawGraph {
    file: String,
    inputs: Vec<(String, Vec<i64>)>,
    outputs: Vec<(String, Vec<i64>)>,
    states: Vec<(String, String, Vec<i64>)>,
}

fn object<'a>(value: Option<&'a Value>, what: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("manifest `{what}` must be an object"))
}

fn raw_graph(name: &str, value: &Value) -> Result<RawGraph, String> {
    let file = value
        .get("file")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("graph `{name}` has no file"))?
        .to_owned();
    let tensors = |field: &str| -> Result<Vec<(String, Vec<i64>)>, String> {
        object(value.get(field), &format!("graphs.{name}.{field}"))?
            .iter()
            .map(|(n, e)| Ok((n.clone(), shape_of(e, n)?)))
            .collect()
    };
    let states = value
        .get("state_pairs")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("graph `{name}` has no state_pairs list"))?
        .iter()
        .map(|pair| {
            let field = |a: &str, b: &str| {
                pair.get(a)
                    .or_else(|| pair.get(b))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or_else(|| format!("graph `{name}`: state pair without `{a}`"))
            };
            if let Some(init) = pair.get("init").and_then(Value::as_str)
                && init != "zeros"
            {
                return Err(format!("graph `{name}`: state init `{init}` is not zeros"));
            }
            let input = field("in", "input")?;
            let shape = shape_of(pair, &input)?;
            Ok((input, field("out", "output")?, shape))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(RawGraph {
        file,
        inputs: tensors("inputs")?,
        outputs: tensors("outputs")?,
        states,
    })
}

/// Parsed and validated v3 manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V3Manifest {
    pub model: String,
    /// `file -> (sha256, size)` of every member the manifest pins.
    pub files: BTreeMap<String, (String, u64)>,
    pub plan: V3Plan,
}

impl V3Manifest {
    pub fn parse(json: &str) -> Result<Self, String> {
        let root: Value = serde_json::from_str(json).map_err(|e| format!("manifest: {e}"))?;
        let model = root
            .get("model")
            .and_then(Value::as_str)
            .ok_or("manifest `model` missing")?
            .to_owned();
        if model != V3_MODEL_ID {
            return Err(format!("manifest model `{model}` is not {V3_MODEL_ID}"));
        }
        let hop = root.get("hop").and_then(Value::as_u64);
        let rate = root.get("sample_rate").and_then(Value::as_u64);
        if hop != Some(480) || rate != Some(48_000) {
            return Err(format!(
                "manifest hop {hop:?} / sample_rate {rate:?}: the filter runs 480 / 48000"
            ));
        }
        let mut files = BTreeMap::new();
        for (file, entry) in object(root.get("files"), "files")? {
            let sha = entry
                .get("sha256")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("files.{file} has no sha256"))?
                .to_ascii_lowercase();
            let size = entry
                .get("size")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("files.{file} has no size"))?;
            files.insert(file.clone(), (sha, size));
        }
        let mut raw = BTreeMap::new();
        for (name, value) in object(root.get("graphs"), "graphs")? {
            let graph = raw_graph(name, value)?;
            if !files.contains_key(&graph.file) {
                return Err(format!(
                    "graph file `{}` is not pinned in `files`",
                    graph.file
                ));
            }
            raw.insert(name.clone(), graph);
        }
        let routing: BTreeMap<String, BTreeMap<String, String>> = match root.get("routing") {
            None | Some(Value::Null) => BTreeMap::new(),
            Some(value) => object(Some(value), "routing")?
                .iter()
                .map(|(graph, inputs)| {
                    let inputs = object(Some(inputs), &format!("routing.{graph}"))?
                        .iter()
                        .map(|(input, source)| {
                            source
                                .as_str()
                                .map(|s| (input.clone(), s.to_owned()))
                                .ok_or_else(|| format!("routing.{graph}.{input} must be a string"))
                        })
                        .collect::<Result<BTreeMap<_, _>, String>>()?;
                    Ok((graph.clone(), inputs))
                })
                .collect::<Result<_, String>>()?,
        };
        for graph in routing.keys() {
            if !raw.contains_key(graph) {
                return Err(format!("routing names unknown graph `{graph}`"));
            }
        }
        Ok(Self {
            model,
            files,
            plan: build_plan(&raw, &routing)?,
        })
    }
}

fn source_graph(source: &str) -> Option<(&str, &str)> {
    source.split_once('.')
}

/// Orders the graphs so every routed source runs first, then resolves every feed.
fn build_plan(
    raw: &BTreeMap<String, RawGraph>,
    routing: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<V3Plan, String> {
    let mut order: Vec<&str> = Vec::new();
    while order.len() < raw.len() {
        let before = order.len();
        for name in raw.keys() {
            if order.contains(&name.as_str()) {
                continue;
            }
            let ready = routing.get(name).is_none_or(|inputs| {
                inputs.values().all(|source| {
                    source == "runtime"
                        || source_graph(source).is_some_and(|(g, _)| order.contains(&g))
                })
            });
            if ready {
                order.push(name);
            }
        }
        if order.len() == before {
            return Err("routing has a cycle or names an unknown source graph".to_owned());
        }
    }

    // Fetch list of each graph: its declared outputs, then the `out` side of its state pairs.
    let fetches: Vec<Vec<FetchPlan>> = order
        .iter()
        .map(|name| {
            let g = &raw[*name];
            g.outputs
                .iter()
                .map(|(n, s)| {
                    Ok(FetchPlan {
                        name: n.clone(),
                        len: element_count(s)?,
                    })
                })
                .chain(g.states.iter().map(|(_, out, s)| {
                    Ok(FetchPlan {
                        name: out.clone(),
                        len: element_count(s)?,
                    })
                }))
                .collect::<Result<Vec<_>, String>>()
        })
        .collect::<Result<_, _>>()?;

    let find_output = |graph: &str, output: &str| -> Option<(usize, usize)> {
        let gi = order.iter().position(|g| *g == graph)?;
        let fi = raw[graph].outputs.iter().position(|(n, _)| n == output)?;
        Some((gi, fi))
    };

    let mut graphs = Vec::new();
    let mut states = Vec::new();
    for (gi, name) in order.iter().enumerate() {
        let g = &raw[*name];
        let routes = routing.get(*name);
        let mut inputs = Vec::new();
        for (input, shape) in &g.inputs {
            let len = element_count(shape)?;
            let source = routes
                .and_then(|r| r.get(input))
                .map_or("runtime", String::as_str);
            let feed = if source == "runtime" {
                let (feed, want) = match input.as_str() {
                    IN_FEAT_ERB => (Feed::FeatErb, DEFAULT_NB_ERB),
                    IN_FEAT_SPEC => (Feed::FeatSpec, 2 * DEFAULT_NB_DF),
                    IN_EMBEDDING => (Feed::Embedding, SPEAKER_EMBEDDING_DIM),
                    IN_PRESENT => (Feed::Present, 1),
                    other => {
                        return Err(format!(
                            "graph `{name}` input `{other}` is not routed and not a runtime input"
                        ));
                    }
                };
                if len != want {
                    return Err(format!("`{name}.{input}` holds {len} values, want {want}"));
                }
                feed
            } else {
                let (sg, so) = source_graph(source)
                    .ok_or_else(|| format!("routing source `{source}` is not <graph>.<output>"))?;
                let (graph, fetch) = find_output(sg, so)
                    .ok_or_else(|| format!("routing source `{source}` does not exist"))?;
                if graph >= gi {
                    return Err(format!("`{name}.{input}` is routed from a later graph"));
                }
                if fetches[graph][fetch].len != len {
                    return Err(format!("`{name}.{input}` and `{source}` differ in size"));
                }
                Feed::Route { graph, fetch }
            };
            inputs.push(InputPlan {
                name: input.clone(),
                shape: shape.clone(),
                len,
                feed,
            });
        }
        for (si, (input, output, shape)) in g.states.iter().enumerate() {
            let len = element_count(shape)?;
            let global = states.len();
            states.push(StatePlan {
                graph: gi,
                input: input.clone(),
                output: output.clone(),
                fetch: g.outputs.len() + si,
                len,
            });
            inputs.push(InputPlan {
                name: input.clone(),
                shape: shape.clone(),
                len,
                feed: Feed::State(global),
            });
        }
        graphs.push(GraphPlan {
            name: (*name).to_owned(),
            file: g.file.clone(),
            inputs,
            fetch: fetches[gi].clone(),
        });
    }

    let locate = |output: &str, want: usize| -> Result<(usize, usize), String> {
        let found: Vec<(usize, usize)> = graphs
            .iter()
            .enumerate()
            .filter_map(|(gi, g)| {
                g.fetch
                    .iter()
                    .position(|f| f.name == output)
                    .filter(|fi| *fi < raw[g.name.as_str()].outputs.len())
                    .map(|fi| (gi, fi))
            })
            .collect();
        match found.as_slice() {
            [(gi, fi)] if graphs[*gi].fetch[*fi].len == want => Ok((*gi, *fi)),
            [_] => Err(format!("output `{output}` does not hold {want} values")),
            [] => Err(format!("no graph outputs `{output}`")),
            _ => Err(format!("several graphs output `{output}`")),
        }
    };
    Ok(V3Plan {
        lsnr: locate(OUT_LSNR, 1)?,
        mask: locate(OUT_MASK, DEFAULT_NB_ERB)?,
        coefs: locate(OUT_COEFS, DF_COEFS_LEN)?,
        graphs,
        states,
    })
}

/// A verified v3 asset: manifest + the bytes of every graph it pins.
#[derive(Clone)]
pub struct V3Asset {
    pub manifest: V3Manifest,
    /// Graph bytes in plan order.
    pub graphs: Vec<Vec<u8>>,
    pub archive_sha256: String,
}

impl std::fmt::Debug for V3Asset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V3Asset")
            .field("model", &self.manifest.model)
            .field("graphs", &self.manifest.plan.graphs.len())
            .field("archive_sha256", &self.archive_sha256)
            .finish()
    }
}

impl V3Asset {
    /// Reads `path` (size-capped) and verifies it like [`Self::from_archive_bytes`].
    pub fn read(path: &Path, expected_sha256: &str) -> Result<Self, String> {
        let mut bytes = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| format!("{}: {e}", path.display()))?
            .take(MAX_ARCHIVE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if bytes.len() as u64 > MAX_ARCHIVE_BYTES {
            return Err("archive exceeds the size limit".to_owned());
        }
        Self::from_archive_bytes(&bytes, expected_sha256)
    }

    /// Checks the whole-archive SHA-256 **before** decompressing; then requires flat members only,
    /// each at most once, `manifest.json` plus exactly the files the manifest pins (size and
    /// SHA-256 checked); nothing else is accepted.
    pub fn from_archive_bytes(bytes: &[u8], expected_sha256: &str) -> Result<Self, String> {
        let actual = sha256_hex(bytes);
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(format!(
                "archive SHA-256 {actual} does not match the pinned {expected_sha256}"
            ));
        }
        let mut members: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
        for entry in archive.entries().map_err(|e| format!("archive: {e}"))? {
            let entry = entry.map_err(|e| format!("archive: {e}"))?;
            if !entry.header().entry_type().is_file() {
                return Err("archive holds a non-regular member".to_owned());
            }
            let path = entry.path().map_err(|e| format!("member path: {e}"))?;
            let mut parts = path
                .components()
                .filter(|c| !matches!(c, std::path::Component::CurDir));
            let name = match (parts.next(), parts.next()) {
                (Some(std::path::Component::Normal(n)), None) => {
                    n.to_str().ok_or("non-UTF-8 member name")?.to_owned()
                }
                _ => return Err(format!("member {} is not a flat file", path.display())),
            };
            let mut data = Vec::new();
            entry
                .take(MAX_MEMBER_BYTES + 1)
                .read_to_end(&mut data)
                .map_err(|e| format!("{name}: {e}"))?;
            if data.len() as u64 > MAX_MEMBER_BYTES {
                return Err(format!("{name} exceeds the size limit"));
            }
            if members.insert(name.clone(), data).is_some() {
                return Err(format!("duplicate member {name}"));
            }
        }
        let manifest_bytes = members
            .remove("manifest.json")
            .ok_or("archive has no manifest.json")?;
        if manifest_bytes.len() as u64 > MAX_MANIFEST_BYTES {
            return Err("manifest.json too large".to_owned());
        }
        let manifest = V3Manifest::parse(
            std::str::from_utf8(&manifest_bytes).map_err(|_| "manifest.json is not UTF-8")?,
        )?;
        for (name, data) in &members {
            let (sha, size) = manifest
                .files
                .get(name)
                .ok_or_else(|| format!("member {name} is not pinned by the manifest"))?;
            if data.len() as u64 != *size || sha256_hex(data) != *sha {
                return Err(format!(
                    "member {name} does not match its pinned size/SHA-256"
                ));
            }
        }
        if let Some(missing) = manifest.files.keys().find(|f| !members.contains_key(*f)) {
            return Err(format!("pinned member {missing} is missing"));
        }
        let graphs = manifest
            .plan
            .graphs
            .iter()
            .map(|g| members[&g.file].clone())
            .collect();
        Ok(Self {
            manifest,
            graphs,
            archive_sha256: actual,
        })
    }
}

/// Converts the outputs of one hop into the DSP prediction (fail closed on bad values).
fn prediction(lsnr: &[f32], mask: &[f32], coefs: &[f32]) -> Result<ModelFrameOutput, String> {
    let lsnr = *lsnr.first().ok_or("empty lsnr")?;
    if !lsnr.is_finite() || mask.iter().chain(coefs).any(|v| !v.is_finite()) {
        return Err("non-finite model output".to_owned());
    }
    let mut erb_gains = [0.0f32; DEFAULT_NB_ERB];
    erb_gains.copy_from_slice(mask);
    let df_coefs = coefs
        .chunks_exact(DEFAULT_DF_ORDER * 2)
        .map(|bin| std::array::from_fn(|tap| Complex32::new(bin[2 * tap], bin[2 * tap + 1])))
        .collect();
    Ok(ModelFrameOutput {
        lsnr,
        erb_gains: Some(erb_gains),
        df_coefs: Some(df_coefs),
    })
}

/// One compiled graph of the plan on some runtime.
pub trait GraphExec: Send {
    /// Writable buffer of input `index` (plan order).
    fn input_mut(&mut self, index: usize) -> Result<&mut [f32], String>;
    /// Runs the graph and copies every fetch (plan order) into `fetch`.
    fn run_into(&mut self, fetch: &mut [Vec<f32>]) -> Result<(), String>;
}

/// Runs the plan on one runtime: every graph, every hop; state committed after the whole hop.
pub struct PlanEngine<X: GraphExec> {
    label: String,
    plan: V3Plan,
    execs: Vec<X>,
    fetch: Vec<Vec<Vec<f32>>>,
    states: Vec<Vec<f32>>,
    embedding: [f32; SPEAKER_EMBEDDING_DIM],
    present: f32,
}

impl<X: GraphExec> PlanEngine<X> {
    fn new(label: String, plan: V3Plan, execs: Vec<X>) -> Self {
        let fetch = plan
            .graphs
            .iter()
            .map(|g| g.fetch.iter().map(|f| vec![0.0; f.len]).collect())
            .collect();
        let states = plan.states.iter().map(|s| vec![0.0; s.len]).collect();
        Self {
            label,
            plan,
            execs,
            fetch,
            states,
            embedding: [0.0; SPEAKER_EMBEDDING_DIM],
            present: 0.0,
        }
    }
}

fn copy_checked(dst: &mut [f32], src: &[f32], what: &str) -> Result<(), String> {
    if dst.len() != src.len() {
        return Err(format!("{what}: {} values, want {}", src.len(), dst.len()));
    }
    dst.copy_from_slice(src);
    Ok(())
}

impl<X: GraphExec> Engine for PlanEngine<X> {
    fn label(&self) -> String {
        self.label.clone()
    }

    fn run(&mut self, features: &ExtractedFeatures) -> Result<ModelFrameOutput, String> {
        if features
            .feat_erb
            .iter()
            .chain(features.feat_spec.iter().flatten())
            .any(|v| !v.is_finite())
        {
            return Err("feature frame contains a non-finite value".to_owned());
        }
        let mut spec = [0.0f32; 2 * DEFAULT_NB_DF];
        spec[..DEFAULT_NB_DF].copy_from_slice(&features.feat_spec[0]);
        spec[DEFAULT_NB_DF..].copy_from_slice(&features.feat_spec[1]);
        for (gi, graph) in self.plan.graphs.iter().enumerate() {
            for (ii, input) in graph.inputs.iter().enumerate() {
                let present = [self.present];
                let src: &[f32] = match &input.feed {
                    Feed::FeatErb => &features.feat_erb,
                    Feed::FeatSpec => &spec,
                    Feed::Embedding => &self.embedding,
                    Feed::Present => &present,
                    Feed::Route { graph, fetch } => &self.fetch[*graph][*fetch],
                    Feed::State(index) => &self.states[*index],
                };
                copy_checked(self.execs[gi].input_mut(ii)?, src, &input.name)?;
            }
            self.execs[gi].run_into(&mut self.fetch[gi])?;
        }
        let pick = |(g, f): (usize, usize)| self.fetch[g][f].as_slice();
        let result = prediction(
            pick(self.plan.lsnr),
            pick(self.plan.mask),
            pick(self.plan.coefs),
        )?;
        for state in &self.plan.states {
            if self.fetch[state.graph][state.fetch]
                .iter()
                .any(|v| !v.is_finite())
            {
                return Err(format!("state `{}` is not finite", state.output));
            }
        }
        for (index, state) in self.plan.states.iter().enumerate() {
            self.states[index].copy_from_slice(&self.fetch[state.graph][state.fetch]);
        }
        Ok(result)
    }

    fn reset(&mut self) -> Result<(), String> {
        for state in &mut self.states {
            state.fill(0.0);
        }
        Ok(())
    }

    fn set_conditioning(&mut self, conditioning: &Conditioning) -> Result<(), String> {
        match &conditioning.embedding {
            None => {
                self.embedding = [0.0; SPEAKER_EMBEDDING_DIM];
                self.present = 0.0;
            }
            Some(values) => {
                if values.len() != SPEAKER_EMBEDDING_DIM || values.iter().any(|v| !v.is_finite()) {
                    return Err(format!(
                        "speaker embedding must hold {SPEAKER_EMBEDDING_DIM} finite values"
                    ));
                }
                self.embedding.copy_from_slice(values);
                self.present = 1.0;
            }
        }
        Ok(())
    }

    fn supports_speaker(&self) -> bool {
        true
    }

    fn export_state(&self) -> Option<Vec<Vec<f32>>> {
        Some(self.states.clone())
    }

    fn import_state(&mut self, state: &[Vec<f32>]) -> Result<(), String> {
        if state.len() != self.states.len()
            || state
                .iter()
                .zip(&self.states)
                .any(|(a, b)| a.len() != b.len())
        {
            return Err("state layout differs".to_owned());
        }
        self.states.clone_from_slice(state);
        Ok(())
    }
}

// ---------------------------------------------------------------- ONNX Runtime ----------

fn ort_err<R>(error: ort::Error<R>) -> String {
    error.to_string()
}

pub(crate) fn ort_session(graph: &[u8]) -> Result<Session, String> {
    Session::builder()
        .map_err(ort_err)?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(ort_err)?
        .with_intra_threads(2)
        .map_err(ort_err)?
        .with_inter_threads(1)
        .map_err(ort_err)?
        .commit_from_memory(graph)
        .map_err(ort_err)
}

pub(crate) fn ort_zeros(shape: &[i64]) -> Result<Tensor<f32>, String> {
    let len = element_count(shape)?;
    Tensor::from_array((shape.to_vec(), vec![0.0f32; len])).map_err(ort_err)
}

pub(crate) fn ort_fill(dst: &mut Tensor<f32>, src: &[f32]) -> Result<(), String> {
    let (_, buf) = dst.try_extract_tensor_mut::<f32>().map_err(ort_err)?;
    copy_checked(buf, src, "tensor")
}

pub(crate) fn ort_read(tensor: &Tensor<f32>) -> Result<Vec<f32>, String> {
    let (_, data) = tensor.try_extract_tensor::<f32>().map_err(ort_err)?;
    Ok(data.to_vec())
}

fn check_graph_inputs(declared: &[String], plan: &GraphPlan) -> Result<(), String> {
    if declared.len() != plan.inputs.len()
        || plan.inputs.iter().any(|i| !declared.contains(&i.name))
    {
        return Err(format!(
            "graph `{}` declares inputs {declared:?}, the manifest {:?}",
            plan.name,
            plan.inputs.iter().map(|i| &i.name).collect::<Vec<_>>()
        ));
    }
    Ok(())
}

/// One graph on ONNX Runtime (CPU).
pub struct OrtGraph {
    session: Session,
    names: Vec<String>,
    tensors: Vec<Tensor<f32>>,
    fetch: Vec<String>,
}

impl OrtGraph {
    fn new(bytes: &[u8], plan: &GraphPlan) -> Result<Self, String> {
        let session = ort_session(bytes)?;
        let declared: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_owned())
            .collect();
        check_graph_inputs(&declared, plan)?;
        Ok(Self {
            names: plan.inputs.iter().map(|i| i.name.clone()).collect(),
            tensors: plan
                .inputs
                .iter()
                .map(|i| ort_zeros(&i.shape))
                .collect::<Result<_, _>>()?,
            fetch: plan.fetch.iter().map(|f| f.name.clone()).collect(),
            session,
        })
    }
}

impl GraphExec for OrtGraph {
    fn input_mut(&mut self, index: usize) -> Result<&mut [f32], String> {
        let tensor = self
            .tensors
            .get_mut(index)
            .ok_or("input index out of range")?;
        tensor
            .try_extract_tensor_mut::<f32>()
            .map(|(_, buf)| buf)
            .map_err(ort_err)
    }

    fn run_into(&mut self, fetch: &mut [Vec<f32>]) -> Result<(), String> {
        let inputs: Vec<(&str, &Tensor<f32>)> = self
            .names
            .iter()
            .map(String::as_str)
            .zip(self.tensors.iter())
            .collect();
        let outputs = self.session.run(inputs).map_err(ort_err)?;
        for (name, dst) in self.fetch.iter().zip(fetch.iter_mut()) {
            let value = outputs
                .get(name)
                .ok_or_else(|| format!("missing output `{name}`"))?;
            let (_, data) = value.try_extract_tensor::<f32>().map_err(ort_err)?;
            copy_checked(dst, data, name)?;
        }
        Ok(())
    }
}

pub type V3OrtEngine = PlanEngine<OrtGraph>;

/// Builds the v3 plan on ONNX Runtime (CPU).
pub fn v3_ort_engine(asset: &V3Asset) -> Result<V3OrtEngine, String> {
    let plan = asset.manifest.plan.clone();
    let execs = plan
        .graphs
        .iter()
        .zip(&asset.graphs)
        .map(|(g, bytes)| OrtGraph::new(bytes, g))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlanEngine::new("ort:CPU".to_owned(), plan, execs))
}

// ---------------------------------------------------------------- OpenVINO --------------

/// One graph compiled for an OpenVINO device, every dimension static (what the NPU needs),
/// precision from the Clearcore policy (FP16 on NPU/GPU, FP32 on the x86 CPU).
pub struct OpenVinoGraph {
    // Field order matters: the request drops before the compiled model that created it.
    request: OpenVinoInferRequest,
    _compiled: OpenVinoCompiledModel,
    tensors: Vec<OpenVinoTensor>,
    fetch: Vec<String>,
}

impl OpenVinoGraph {
    fn new(
        core: &OpenVinoCore,
        bytes: &[u8],
        plan: &GraphPlan,
        device: &str,
    ) -> Result<Self, String> {
        let ov = |e: realtime_noise_runtime_openvino::OpenVinoError| e.to_string();
        let mut model = core.read_model_from_memory(bytes, None).map_err(ov)?;
        check_graph_inputs(&model.input_names().map_err(ov)?, plan)?;
        for input in &plan.inputs {
            model.reshape_input(&input.name, &input.shape).map_err(ov)?;
        }
        let precision = precision_for_device(device);
        let compiled = core
            .compile_model_with_properties(
                &model,
                device,
                &[
                    ("PERFORMANCE_HINT", "LATENCY"),
                    ("INFERENCE_PRECISION_HINT", precision.hint()),
                ],
            )
            .map_err(ov)?;
        let mut request = compiled.create_infer_request().map_err(ov)?;
        let mut tensors = Vec::with_capacity(plan.inputs.len());
        for input in &plan.inputs {
            let tensor = OpenVinoTensor::from_slice_f32(
                &input.shape,
                &vec![0.0; input.len],
                core.library().clone(),
            )
            .map_err(ov)?;
            request.set_tensor(&input.name, &tensor).map_err(ov)?;
            tensors.push(tensor);
        }
        Ok(Self {
            request,
            _compiled: compiled,
            tensors,
            fetch: plan.fetch.iter().map(|f| f.name.clone()).collect(),
        })
    }
}

impl GraphExec for OpenVinoGraph {
    fn input_mut(&mut self, index: usize) -> Result<&mut [f32], String> {
        self.tensors
            .get_mut(index)
            .ok_or("input index out of range")?
            .as_mut_slice_f32()
            .map_err(|e| e.to_string())
    }

    fn run_into(&mut self, fetch: &mut [Vec<f32>]) -> Result<(), String> {
        self.request.infer().map_err(|e| e.to_string())?;
        for (name, dst) in self.fetch.iter().zip(fetch.iter_mut()) {
            let tensor = self.request.get_tensor(name).map_err(|e| e.to_string())?;
            copy_checked(dst, tensor.as_slice_f32().map_err(|e| e.to_string())?, name)?;
        }
        Ok(())
    }
}

pub type V3OpenVinoEngine = PlanEngine<OpenVinoGraph>;

/// Compiles the v3 plan for one OpenVINO device (`NPU`, `GPU`, `CPU`, ...).
pub fn v3_openvino_engine(asset: &V3Asset, device: &str) -> Result<V3OpenVinoEngine, String> {
    let core = OpenVinoCore::new().map_err(|e| e.to_string())?;
    let available = core.available_devices().map_err(|e| e.to_string())?;
    if !available.iter().any(|d| d.eq_ignore_ascii_case(device)) {
        return Err(format!(
            "OpenVINO device {device} not found (lists: {})",
            available.join(", ")
        ));
    }
    let plan = asset.manifest.plan.clone();
    let execs = plan
        .graphs
        .iter()
        .zip(&asset.graphs)
        .map(|(g, bytes)| OpenVinoGraph::new(&core, bytes, g, device))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PlanEngine::new(format!("openvino:{device}"), plan, execs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> serde_json::Value {
        serde_json::json!({
            "model": "pdfnet3-pro-v3-full", "hop": 480, "sample_rate": 48000,
            "files": {"enc.onnx": {"sha256": "AA", "size": 1}, "dec.onnx": {"sha256": "bb", "size": 1}},
            "graphs": {
                "enc": {"file": "enc.onnx",
                    "inputs": {"feat_erb": {"shape": [1,1,1,32]}, "feat_spec": {"shape": [1,2,1,96]},
                               "speaker_embedding": {"shape": [1,192]}, "speaker_present": {"shape": [1]}},
                    "outputs": {"emb": {"shape": [1,1,8]}, "lsnr": {"shape": [1,1,1]}},
                    "state_pairs": [{"in": "h_in", "out": "h_out", "shape": [1,"S",4], "init": "zeros"}]},
                "dec": {"file": "dec.onnx",
                    "inputs": {"emb": {"shape": [1,1,8]}},
                    "outputs": {"m": {"shape": [1,1,1,32]}, "coefs": {"shape": [1,1,96,10]}},
                    "state_pairs": []}
            },
            "routing": {"dec": {"emb": "enc.emb"}}
        })
    }

    #[test]
    fn the_plan_orders_graphs_by_routing_and_resolves_every_feed() {
        let m = V3Manifest::parse(&manifest().to_string()).expect("manifest");
        let plan = &m.plan;
        assert_eq!(plan.graphs[0].name, "enc", "the routed source runs first");
        assert_eq!(
            plan.graphs[1].inputs[0].feed,
            Feed::Route { graph: 0, fetch: 0 }
        );
        assert_eq!(plan.states[0].len, 4, "symbolic S is pinned to 1");
        assert_eq!(plan.lsnr, (0, 1));
        // Outputs are listed in name order (serde_json map): `coefs` before `m`.
        assert_eq!(plan.mask, (1, 1));
        assert_eq!(plan.coefs, (1, 0));
        assert_eq!(m.files["enc.onnx"].0, "aa");
    }

    #[test]
    fn an_input_the_filter_cannot_feed_fails_closed() {
        let mut m = manifest();
        m["graphs"]["dec"]["inputs"]["mystery"] = serde_json::json!({"shape": [1]});
        let err = V3Manifest::parse(&m.to_string()).unwrap_err();
        assert!(err.contains("mystery"), "{err}");
    }

    #[test]
    fn a_routing_cycle_or_unknown_source_fails_closed() {
        let mut m = manifest();
        m["routing"]["enc"] = serde_json::json!({"feat_erb": "dec.m"});
        assert!(V3Manifest::parse(&m.to_string()).is_err());
        let mut m = manifest();
        m["routing"]["dec"]["emb"] = serde_json::json!("enc.nothing");
        assert!(V3Manifest::parse(&m.to_string()).is_err());
    }

    #[test]
    fn wrong_model_or_hop_fails_closed() {
        let mut m = manifest();
        m["model"] = serde_json::json!("pdfnet3-pro-v2");
        assert!(V3Manifest::parse(&m.to_string()).is_err());
        let mut m = manifest();
        m["hop"] = serde_json::json!(256);
        assert!(V3Manifest::parse(&m.to_string()).is_err());
    }

    #[test]
    fn archive_hash_is_checked_before_anything_else() {
        let err = V3Asset::from_archive_bytes(b"not a tarball", &"0".repeat(64)).unwrap_err();
        assert!(err.contains("does not match"), "{err}");
    }
}
