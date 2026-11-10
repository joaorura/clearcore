//! Manifest-described stateful model archives (pDFNet3 Pro v3 full and later).
//!
//! Layout: a flat `.tar.gz` with `manifest.json` and the ONNX graph(s) — one graph, or a split
//! export (`enc.onnx`/`erb_dec.onnx`/`df_dec.onnx`) — optionally `config.ini` and text notes
//! (`*.txt`, `*.md`). The manifest:
//!
//! ```json
//! { "model": "pdfnet3-pro-v3-full", "hop": 480, "sample_rate": 48000,
//!   "inputs":  [{"name": "feat_erb", "shape": [1, 1, 1, 32]}, ...],
//!   "outputs": [{"name": "m", "shape": [1, 1, 1, 32]}, ...],
//!   "state_pairs": [["h_in", "h_out"], ...],
//!   "sha256": {"model.onnx": "<hex>"} }
//! ```
//!
//! `inputs`/`outputs` entries may also be bare names; `state_pairs` entries may be
//! `{"in": .., "out": ..}` (or `input`/`output`) objects or a `{in: out}` map; `sha256` may be a
//! single string when there is one graph. Every graph's SHA-256 listed in the manifest is checked
//! (the whole-archive SHA-256 pinned in the code stays the trust anchor).

use std::io::Read;

use flate2::read::GzDecoder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tar::Archive;

use crate::InferenceError;

/// Name of the manifest member.
pub const MANIFEST_FILE: &str = "manifest.json";
const MAX_GRAPH_BYTES: u64 = 48 * 1024 * 1024;
const MAX_TEXT_BYTES: u64 = 256 * 1024;
const MAX_DECOMPRESSED_BYTES: u64 = 160 * 1024 * 1024;
const MAX_ENTRIES: usize = 16;

/// A port listed by the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestPort {
    pub name: String,
    /// Declared shape, if any (`None` entries are symbolic dimensions).
    pub shape: Option<Vec<Option<i64>>>,
}

/// Parsed `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatefulManifest {
    pub model: String,
    pub hop: usize,
    pub sample_rate: u32,
    pub inputs: Vec<ManifestPort>,
    pub outputs: Vec<ManifestPort>,
    pub state_pairs: Vec<(String, String)>,
    /// `(file, lowercase hex)` for every member the manifest pins (`sha256` or `files`).
    pub sha256: Vec<(String, String)>,
    /// `(graph name, file)` declared under `graphs` (empty: the graphs are the `.onnx` members).
    pub graph_files: Vec<(String, String)>,
    /// `(graph, input, producer graph, producer output)` from `routing` (`"runtime"` entries,
    /// fed by the executor itself, are not listed).
    pub routing: Vec<(String, String, String, String)>,
}

fn invalid(message: impl Into<String>) -> InferenceError {
    InferenceError::ModelCorruption(message.into())
}

fn parse_ports(value: Option<&Value>, field: &str) -> Result<Vec<ManifestPort>, InferenceError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if let Some(map) = value.as_object() {
        return Ok(map
            .iter()
            .map(|(name, spec)| ManifestPort {
                name: name.clone(),
                shape: spec
                    .get("shape")
                    .and_then(Value::as_array)
                    .map(|dims| dims.iter().map(Value::as_i64).collect()),
            })
            .collect());
    }
    let items = value
        .as_array()
        .ok_or_else(|| invalid(format!("manifest `{field}` is not a list")))?;
    items
        .iter()
        .map(|item| match item {
            Value::String(name) => Ok(ManifestPort {
                name: name.clone(),
                shape: None,
            }),
            Value::Object(map) => {
                let name = map
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid(format!("manifest `{field}` entry without a name")))?;
                let shape = map
                    .get("shape")
                    .and_then(Value::as_array)
                    .map(|dims| dims.iter().map(Value::as_i64).collect::<Vec<Option<i64>>>());
                Ok(ManifestPort {
                    name: name.to_owned(),
                    shape,
                })
            }
            _ => Err(invalid(format!("manifest `{field}` entry is not a port"))),
        })
        .collect()
}

fn parse_state_pairs(value: Option<&Value>) -> Result<Vec<(String, String)>, InferenceError> {
    let bad = || invalid("manifest `state_pairs` entry is not an (input, output) pair");
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Object(map)) => map
            .iter()
            .map(|(input, output)| {
                output
                    .as_str()
                    .map(|output| (input.clone(), output.to_owned()))
                    .ok_or_else(bad)
            })
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|item| match item {
                Value::Array(pair) => match pair.as_slice() {
                    [Value::String(a), Value::String(b)] => Ok((a.clone(), b.clone())),
                    _ => Err(bad()),
                },
                Value::Object(map) => {
                    let get = |keys: &[&str]| {
                        keys.iter()
                            .find_map(|key| map.get(*key).and_then(Value::as_str))
                            .map(str::to_owned)
                    };
                    match (get(&["in", "input"]), get(&["out", "output"])) {
                        (Some(a), Some(b)) => Ok((a, b)),
                        _ => Err(bad()),
                    }
                }
                _ => Err(bad()),
            })
            .collect(),
        Some(_) => Err(bad()),
    }
}

fn is_hex64(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())
}

impl StatefulManifest {
    /// Parses and sanity-checks a manifest (`model`, `hop`, `sample_rate` required).
    pub fn parse(bytes: &[u8]) -> Result<Self, InferenceError> {
        let root: Value = serde_json::from_slice(bytes)
            .map_err(|error| invalid(format!("manifest.json is not JSON: {error}")))?;
        let model = root
            .get("model")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("manifest has no `model`"))?
            .to_owned();
        let hop = root
            .get("hop")
            .and_then(Value::as_u64)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| invalid("manifest has no integer `hop`"))?;
        let sample_rate = root
            .get("sample_rate")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .ok_or_else(|| invalid("manifest has no integer `sample_rate`"))?;
        let mut sha256: Vec<(String, String)> = match root.get("sha256") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::String(hex)) => vec![(String::new(), hex.to_ascii_lowercase())],
            Some(Value::Object(map)) => map
                .iter()
                .map(|(file, hex)| {
                    hex.as_str()
                        .map(|hex| (file.clone(), hex.to_ascii_lowercase()))
                        .ok_or_else(|| invalid("manifest `sha256` value is not a string"))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(invalid("manifest `sha256` is neither a string nor a map")),
        };
        if let Some(files) = root.get("files").and_then(Value::as_object) {
            for (file, entry) in files {
                let hex = entry
                    .get("sha256")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid("manifest `files` entry has no sha256"))?;
                sha256.push((file.clone(), hex.to_ascii_lowercase()));
            }
        }
        if sha256.iter().any(|(_, hex)| !is_hex64(hex)) {
            return Err(invalid("manifest `sha256` is not a SHA-256 hex digest"));
        }
        let mut inputs = parse_ports(root.get("inputs"), "inputs")?;
        let mut outputs = parse_ports(root.get("outputs"), "outputs")?;
        let mut state_pairs = parse_state_pairs(root.get("state_pairs"))?;
        let mut graph_files = Vec::new();
        if let Some(graphs) = root.get("graphs").and_then(Value::as_object) {
            for (graph, spec) in graphs {
                let file = spec
                    .get("file")
                    .and_then(Value::as_str)
                    .map_or_else(|| format!("{graph}.onnx"), str::to_owned);
                graph_files.push((graph.clone(), file));
                inputs.extend(parse_ports(spec.get("inputs"), "inputs")?);
                outputs.extend(parse_ports(spec.get("outputs"), "outputs")?);
                state_pairs.extend(parse_state_pairs(spec.get("state_pairs"))?);
            }
        }
        let mut routing = Vec::new();
        if let Some(graphs) = root.get("routing").and_then(Value::as_object) {
            for (graph, inputs) in graphs {
                let inputs = inputs
                    .as_object()
                    .ok_or_else(|| invalid("manifest `routing` entry is not a map"))?;
                for (input, source) in inputs {
                    let source = source
                        .as_str()
                        .ok_or_else(|| invalid("manifest `routing` source is not a string"))?;
                    if source == "runtime" {
                        continue;
                    }
                    let (producer, output) = source
                        .split_once('.')
                        .ok_or_else(|| invalid("manifest `routing` source is not graph.output"))?;
                    routing.push((
                        graph.clone(),
                        input.clone(),
                        producer.to_owned(),
                        output.to_owned(),
                    ));
                }
            }
        }
        Ok(Self {
            model,
            hop,
            sample_rate,
            inputs,
            outputs,
            state_pairs,
            sha256,
            graph_files,
            routing,
        })
    }
}

/// The graphs and manifest of a verified manifest archive.
#[derive(Debug, Clone)]
pub struct StatefulModelFiles {
    pub manifest: StatefulManifest,
    /// `(graph name = file stem, ONNX bytes)` in archive order.
    pub graphs: Vec<(String, Vec<u8>)>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Whether the (gzip'd tar) `bytes` contain a top-level `manifest.json`.
#[must_use]
pub fn has_manifest(bytes: &[u8]) -> bool {
    let mut archive = Archive::new(GzDecoder::new(bytes).take(MAX_DECOMPRESSED_BYTES));
    let Ok(entries) = archive.entries() else {
        return false;
    };
    for entry in entries.take(MAX_ENTRIES) {
        let Ok(entry) = entry else {
            return false;
        };
        if entry
            .path()
            .ok()
            .is_some_and(|path| path.to_str() == Some(MANIFEST_FILE))
        {
            return true;
        }
    }
    false
}

/// Walks a manifest archive (already verified by its whole-archive SHA-256).
///
/// Flat regular members only, bounded sizes and count, `manifest.json` required, at least one `.onnx`, no
/// duplicates, no other member kinds; every graph pinned by the manifest must match.
/// `name.ends_with(".{extension}")`, spelled so the case-sensitivity is explicit.
fn has_extension(name: &str, extension: &str) -> bool {
    name.rsplit_once('.')
        .is_some_and(|(_, ext)| ext == extension)
}

#[allow(clippy::too_many_lines)] // linear pipeline; splitting it before the release risks behaviour changes
pub fn read_manifest_archive(bytes: &[u8]) -> Result<StatefulModelFiles, InferenceError> {
    let mut archive = Archive::new(GzDecoder::new(bytes).take(MAX_DECOMPRESSED_BYTES));
    let entries = archive
        .entries()
        .map_err(|_| invalid("unreadable archive"))?;
    let mut manifest_bytes: Option<Vec<u8>> = None;
    let mut graphs: Vec<(String, Vec<u8>)> = Vec::new();
    let mut texts: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (index, entry) in entries.enumerate() {
        if index >= MAX_ENTRIES {
            return Err(invalid("too many archive members"));
        }
        let entry = entry.map_err(|_| invalid("unreadable archive"))?;
        if !entry.header().entry_type().is_file() {
            return Err(invalid("archive member is not a regular file"));
        }
        let name = entry
            .path()
            .ok()
            .and_then(|path| path.to_str().map(str::to_owned))
            .ok_or_else(|| invalid("archive member path is not valid"))?;
        if name.contains('/') || name.contains('\\') || name.starts_with('.') || name.is_empty() {
            return Err(invalid("archive member is not a flat file name"));
        }
        if seen.contains(&name) {
            return Err(invalid("duplicate archive member"));
        }
        seen.push(name.clone());
        let limit = if has_extension(&name, "onnx") {
            MAX_GRAPH_BYTES
        } else if name == MANIFEST_FILE
            || name == "config.ini"
            || has_extension(&name, "txt")
            || has_extension(&name, "md")
        {
            MAX_TEXT_BYTES
        } else {
            return Err(invalid("unexpected archive member"));
        };
        if entry.size() > limit {
            return Err(invalid("archive member is too large"));
        }
        let expected = entry.size();
        let mut content = Vec::new();
        entry
            .take(limit + 1)
            .read_to_end(&mut content)
            .map_err(|_| invalid("archive member is unreadable"))?;
        if content.len() as u64 != expected {
            return Err(invalid("archive member is truncated"));
        }
        if name == MANIFEST_FILE {
            manifest_bytes = Some(content);
        } else if let Some(stem) = name.strip_suffix(".onnx") {
            graphs.push((stem.to_owned(), content));
        } else {
            texts.push((name, content));
        }
    }
    let manifest = StatefulManifest::parse(
        &manifest_bytes.ok_or_else(|| invalid("archive has no manifest.json"))?,
    )?;
    if graphs.is_empty() {
        return Err(invalid("archive has no ONNX graph"));
    }
    for (file, hex) in &manifest.sha256 {
        let graph = if file.is_empty() {
            match graphs.as_slice() {
                [(_, only)] => only,
                _ => return Err(invalid("manifest pins one SHA-256 for several graphs")),
            }
        } else if let Some(stem) = file.strip_suffix(".onnx") {
            match graphs.iter().find(|(name, _)| name == stem) {
                Some((_, graph)) => graph,
                None => return Err(invalid("manifest pins a graph the archive lacks")),
            }
        } else {
            match texts.iter().find(|(name, _)| name == file) {
                Some((_, text)) => text,
                None => return Err(invalid("manifest pins a member the archive lacks")),
            }
        };
        if &sha256_hex(graph) != hex {
            return Err(invalid("graph SHA-256 does not match the manifest"));
        }
    }
    // Graph names: those of `graphs` in the manifest when present (each file must exist).
    if !manifest.graph_files.is_empty() {
        let mut named = Vec::with_capacity(manifest.graph_files.len());
        for (graph, file) in &manifest.graph_files {
            let stem = file.strip_suffix(".onnx").unwrap_or(file);
            let position = graphs
                .iter()
                .position(|(name, _)| name == stem)
                .ok_or_else(|| invalid("manifest declares a graph the archive lacks"))?;
            named.push((graph.clone(), graphs.swap_remove(position).1));
        }
        if !graphs.is_empty() {
            return Err(invalid("archive has a graph the manifest does not declare"));
        }
        graphs = named;
    }
    Ok(StatefulModelFiles { manifest, graphs })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use std::io::Write;

    use flate2::{Compression, write::GzEncoder};
    use tar::{Builder, Header};

    use super::*;

    fn archive(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
        for (name, data) in members {
            let mut header = Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, name, *data).unwrap();
        }
        let mut gz = builder.into_inner().unwrap();
        gz.flush().unwrap();
        gz.finish().unwrap()
    }

    fn manifest(sha: &str) -> String {
        format!(
            r#"{{"model":"pdfnet3-pro-v3-full","hop":480,"sample_rate":48000,
               "inputs":["feat_erb",{{"name":"speaker_embedding","shape":[1,192]}}],
               "outputs":[{{"name":"m","shape":[1,1,"S",32]}}],
               "state_pairs":[["h_in","h_out"],{{"in":"a_in","out":"a_out"}}],
               "sha256":{{"model.onnx":"{sha}"}}}}"#
        )
    }

    #[test]
    fn manifest_parses_flexible_forms() {
        let parsed = StatefulManifest::parse(manifest(&"a".repeat(64)).as_bytes()).unwrap();
        assert_eq!(parsed.model, "pdfnet3-pro-v3-full");
        assert_eq!(parsed.hop, 480);
        assert_eq!(parsed.inputs[1].shape, Some(vec![Some(1), Some(192)]));
        assert_eq!(
            parsed.outputs[0].shape,
            Some(vec![Some(1), Some(1), None, Some(32)])
        );
        assert_eq!(
            parsed.state_pairs,
            vec![
                ("h_in".to_owned(), "h_out".to_owned()),
                ("a_in".to_owned(), "a_out".to_owned())
            ]
        );
        let map = StatefulManifest::parse(
            br#"{"model":"x","hop":480,"sample_rate":48000,"state_pairs":{"s_in":"s_out"},"sha256":null}"#,
        )
        .unwrap();
        assert_eq!(
            map.state_pairs,
            vec![("s_in".to_owned(), "s_out".to_owned())]
        );
        assert!(StatefulManifest::parse(br#"{"model":"x","hop":480}"#).is_err());
        assert!(
            StatefulManifest::parse(
                br#"{"model":"x","hop":480,"sample_rate":48000,"sha256":"zz"}"#
            )
            .is_err()
        );
    }

    #[test]
    fn archive_walk_checks_graph_hashes_and_members() {
        let graph = b"onnx-bytes".as_slice();
        let good = manifest(&sha256_hex(graph));
        let bytes = archive(&[("manifest.json", good.as_bytes()), ("model.onnx", graph)]);
        assert!(has_manifest(&bytes));
        let files = read_manifest_archive(&bytes).unwrap();
        assert_eq!(files.graphs.len(), 1);
        assert_eq!(files.graphs[0].0, "model");

        let wrong = manifest(&"0".repeat(64));
        let bytes = archive(&[("manifest.json", wrong.as_bytes()), ("model.onnx", graph)]);
        assert!(read_manifest_archive(&bytes).is_err());

        let bytes = archive(&[
            ("manifest.json", good.as_bytes()),
            ("model.onnx", graph),
            ("x.so", b""),
        ]);
        assert!(read_manifest_archive(&bytes).is_err());

        let bytes = archive(&[("model.onnx", graph)]);
        assert!(!has_manifest(&bytes));
        assert!(read_manifest_archive(&bytes).is_err());

        let bytes = archive(&[
            ("manifest.json", good.as_bytes()),
            ("sub/model.onnx", graph),
        ]);
        assert!(read_manifest_archive(&bytes).is_err());
    }
}
