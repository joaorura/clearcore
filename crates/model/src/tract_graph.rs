//! tract (`tract-onnx` 0.19) as a [`GraphSession`].
//!
//! One stateful ONNX graph compiled with every
//! dynamic dimension pinned to 1 (`S = 1`), run hop by hop. The same stateful export runs here,
//! on `OpenVINO` and on ONNX Runtime, so the daemon and the virtual microphone share one model.
//!
//! Operator coverage (tract-onnx 0.19.16): `GRU`, `Einsum`, `MatMul`/`Gemm`, `Softmax`,
//! `Erf`, `Exp`, `Conv`/`ConvTranspose` and the shape ops are registered; `LayerNormalization`
//! (ONNX opset 17) is **not** — a graph that uses it must be exported at opset <= 16, where the
//! exporter decomposes it (`ReduceMean`/`Sub`/`Pow`/`Sqrt`/`Div`), as the Pro v3 full (opset 12) is.

use tract_onnx::prelude::*;
use tract_onnx::tract_hir::infer::Factoid as _;

use crate::InferenceError;
use crate::stateful_graph::{GraphSession, TensorSpec, pin_dynamic_dims};

/// One compiled graph on tract.
pub struct TractGraphSession {
    plan: TypedSimplePlan<TypedModel>,
    inputs: Vec<TensorSpec>,
    input_shapes: Vec<Vec<usize>>,
    outputs: Vec<TensorSpec>,
}

impl std::fmt::Debug for TractGraphSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TractGraphSession")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .finish_non_exhaustive()
    }
}

fn to_usize_shape(shape: &[i64]) -> Vec<usize> {
    shape
        .iter()
        .map(|&dim| usize::try_from(dim).unwrap_or(1))
        .collect()
}

impl TractGraphSession {
    /// Parses `onnx`, pins every input to a static shape and optimises the graph.
    pub fn load(name: &str, onnx: &[u8]) -> Result<Self, InferenceError> {
        let fail = |error: TractError| {
            InferenceError::ModelCorruption(format!("tract cannot load graph `{name}`: {error:#}"))
        };
        let mut model = tract_onnx::onnx()
            .model_for_read(&mut &onnx[..])
            .map_err(fail)?;
        let outlets = model.input_outlets().map_err(fail)?.to_vec();
        let mut inputs = Vec::with_capacity(outlets.len());
        let mut input_shapes = Vec::with_capacity(outlets.len());
        for (index, outlet) in outlets.iter().enumerate() {
            let input_name = model.node(outlet.node).name.clone();
            let fact = model.input_fact(index).map_err(fail)?;
            if fact.shape.is_open() {
                return Err(InferenceError::ModelCorruption(format!(
                    "graph `{name}` input `{input_name}` has no declared rank"
                )));
            }
            let dims: Vec<Option<i64>> = fact
                .shape
                .dims()
                .map(|dim| dim.concretize().and_then(|dim| dim.to_i64().ok()))
                .collect();
            let shape = pin_dynamic_dims(&dims);
            let static_shape = to_usize_shape(&shape);
            model
                .set_input_fact(
                    index,
                    InferenceFact::dt_shape(f32::datum_type(), static_shape.clone()),
                )
                .map_err(fail)?;
            inputs.push(TensorSpec::new(input_name, shape));
            input_shapes.push(static_shape);
        }
        let output_names: Vec<String> = model
            .output_outlets()
            .map_err(fail)?
            .iter()
            .map(|outlet| {
                model
                    .outlet_label(*outlet)
                    .map_or_else(|| model.node(outlet.node).name.clone(), str::to_owned)
            })
            .collect();
        let typed = model.into_optimized().map_err(fail)?;
        let outputs = typed
            .output_outlets()
            .map_err(fail)?
            .iter()
            .zip(output_names)
            .map(|(outlet, output_name)| {
                let shape = typed
                    .outlet_fact(*outlet)
                    .ok()
                    .and_then(|fact| fact.shape.as_concrete().map(<[usize]>::to_vec))
                    .map_or_else(Vec::new, |dims| {
                        dims.iter()
                            .map(|&dim| i64::try_from(dim).unwrap_or(-1))
                            .collect()
                    });
                TensorSpec::new(output_name, shape)
            })
            .collect();
        let plan = typed.into_runnable().map_err(fail)?;
        Ok(Self {
            plan,
            inputs,
            input_shapes,
            outputs,
        })
    }
}

impl GraphSession for TractGraphSession {
    fn inputs(&self) -> &[TensorSpec] {
        &self.inputs
    }

    fn outputs(&self) -> &[TensorSpec] {
        &self.outputs
    }

    fn run(&mut self, inputs: &[&[f32]], outputs: &mut [Vec<f32>]) -> Result<(), InferenceError> {
        let fail = |error: TractError| InferenceError::InferenceExecution(format!("{error:#}"));
        let values: TVec<TValue> = inputs
            .iter()
            .zip(&self.input_shapes)
            .map(|(data, shape)| Tensor::from_shape(shape, data).map(TValue::from))
            .collect::<TractResult<_>>()
            .map_err(fail)?;
        let results = self.plan.run(values).map_err(fail)?;
        if results.len() != outputs.len() {
            return Err(InferenceError::InferenceExecution(format!(
                "graph returned {} outputs, expected {}",
                results.len(),
                outputs.len()
            )));
        }
        for (slot, value) in outputs.iter_mut().zip(results.iter()) {
            let data = value.as_slice::<f32>().map_err(fail)?;
            slot.clear();
            slot.extend_from_slice(data);
        }
        Ok(())
    }
}
