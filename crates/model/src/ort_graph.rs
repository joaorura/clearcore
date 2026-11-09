//! ONNX Runtime as a [`GraphSession`] for the generic stateful executor (the runtime of the
//! virtual microphone's filter), so the filter and the daemon run the same executor.

use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;

use crate::InferenceError;
use crate::stateful_graph::{GraphSession, TensorSpec, pin_dynamic_dims};

fn ort_error<R>(error: ort::Error<R>) -> InferenceError {
    InferenceError::InferenceExecution(error.to_string())
}

/// One stateful graph on ONNX Runtime (CPU, `Level3`, 2 intra-op threads).
pub struct OrtGraphSession {
    session: Session,
    inputs: Vec<TensorSpec>,
    outputs: Vec<TensorSpec>,
    tensors: Vec<Tensor<f32>>,
}

impl std::fmt::Debug for OrtGraphSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrtGraphSession")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .finish_non_exhaustive()
    }
}

fn dims(outlet: &ort::value::Outlet) -> Vec<Option<i64>> {
    outlet
        .dtype()
        .tensor_shape()
        .map(|shape| shape.iter().map(|&d| (d >= 0).then_some(d)).collect())
        .unwrap_or_default()
}

impl OrtGraphSession {
    /// Compiles `onnx` (already verified by the caller).
    pub fn load(name: &str, onnx: &[u8]) -> Result<Self, InferenceError> {
        let fail = |error: InferenceError| {
            InferenceError::ModelCorruption(format!(
                "ONNX Runtime cannot load graph `{name}`: {error}"
            ))
        };
        let session = Session::builder()
            .map_err(ort_error)
            .and_then(|b| {
                b.with_optimization_level(GraphOptimizationLevel::Level3)
                    .map_err(ort_error)
            })
            .and_then(|b| b.with_intra_threads(2).map_err(ort_error))
            .and_then(|b| b.with_inter_threads(1).map_err(ort_error))
            .and_then(|mut b| b.commit_from_memory(onnx).map_err(ort_error))
            .map_err(fail)?;
        let mut inputs = Vec::new();
        let mut tensors = Vec::new();
        for outlet in session.inputs() {
            let shape = pin_dynamic_dims(&dims(outlet));
            let len = shape.iter().product::<i64>();
            let len = usize::try_from(len)
                .map_err(|_| InferenceError::ModelCorruption("invalid input shape".to_owned()))?;
            tensors
                .push(Tensor::from_array((shape.clone(), vec![0.0f32; len])).map_err(ort_error)?);
            inputs.push(TensorSpec::new(outlet.name(), shape));
        }
        let outputs = session
            .outputs()
            .iter()
            .map(|outlet| {
                TensorSpec::new(
                    outlet.name(),
                    dims(outlet).iter().map(|d| d.unwrap_or(-1)).collect(),
                )
            })
            .collect();
        Ok(Self {
            session,
            inputs,
            outputs,
            tensors,
        })
    }
}

impl GraphSession for OrtGraphSession {
    fn inputs(&self) -> &[TensorSpec] {
        &self.inputs
    }

    fn outputs(&self) -> &[TensorSpec] {
        &self.outputs
    }

    fn run(&mut self, inputs: &[&[f32]], outputs: &mut [Vec<f32>]) -> Result<(), InferenceError> {
        for (tensor, data) in self.tensors.iter_mut().zip(inputs) {
            let (_, buffer) = tensor.try_extract_tensor_mut::<f32>().map_err(ort_error)?;
            if buffer.len() != data.len() {
                return Err(InferenceError::InferenceExecution(format!(
                    "input holds {} values, expected {}",
                    data.len(),
                    buffer.len()
                )));
            }
            buffer.copy_from_slice(data);
        }
        let feed: Vec<(&str, &Tensor<f32>)> = self
            .inputs
            .iter()
            .map(|spec| spec.name.as_str())
            .zip(self.tensors.iter())
            .collect();
        let results = self.session.run(feed).map_err(ort_error)?;
        for (slot, spec) in outputs.iter_mut().zip(&self.outputs) {
            let value = results.get(spec.name.as_str()).ok_or_else(|| {
                InferenceError::InferenceExecution(format!("missing output `{}`", spec.name))
            })?;
            let (_, data) = value.try_extract_tensor::<f32>().map_err(ort_error)?;
            slot.clear();
            slot.extend_from_slice(data);
        }
        Ok(())
    }
}
