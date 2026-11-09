//! `OpenVINO` as a [`GraphSession`] for the generic stateful executor of `realtime-noise-model`.
//!
//! Every input is pinned to a fully static shape (dynamic dimensions → 1, the `S = 1` hop) with
//! `ov_model_reshape_input_by_name` before compiling, as the NPU requires; the input tensors are
//! allocated once and bound to the infer request, the outputs are read by name after each run.
//! Ports are discovered from the graph (`ov_model_const_input_by_index`/`..._output_by_index` +
//! `ov_port_get_partial_shape`), so no shape is hard-coded here.

use realtime_noise_model::stateful_graph::{GraphSession, TensorSpec, pin_dynamic_dims};
use realtime_noise_model::{InferenceError, StatefulGraphExecutor, StatefulModelFiles};

use crate::core::OpenVinoCore;
use crate::error::OpenVinoError;
use crate::infer::OpenVinoInferRequest;
use crate::model::{OpenVinoCompiledModel, OpenVinoModel};
use crate::precision::{InferencePrecision, precision_for_device};
use crate::tensor::OpenVinoTensor;

fn load_error(name: &str, device: &str, error: &OpenVinoError) -> InferenceError {
    InferenceError::ModelCorruption(format!(
        "OpenVINO cannot compile graph `{name}` for {device}: {error}"
    ))
}

fn run_error(error: &OpenVinoError) -> InferenceError {
    InferenceError::InferenceExecution(error.to_string())
}

/// One stateful graph compiled for one `OpenVINO` device.
pub struct OpenVinoGraphSession {
    request: OpenVinoInferRequest,
    _compiled: OpenVinoCompiledModel,
    inputs: Vec<TensorSpec>,
    outputs: Vec<TensorSpec>,
    input_tensors: Vec<OpenVinoTensor>,
}

impl std::fmt::Debug for OpenVinoGraphSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenVinoGraphSession")
            .field("inputs", &self.inputs)
            .field("outputs", &self.outputs)
            .finish_non_exhaustive()
    }
}

impl OpenVinoGraphSession {
    /// Reads `onnx`, pins every input to a static shape and compiles it for `device` with the
    /// latency hint and `precision`.
    pub fn load(
        core: &OpenVinoCore,
        name: &str,
        onnx: &[u8],
        device: &str,
        precision: InferencePrecision,
    ) -> Result<Self, InferenceError> {
        let fail = |error: OpenVinoError| load_error(name, device, &error);
        let mut model: OpenVinoModel = core.read_model_from_memory(onnx, None).map_err(fail)?;
        let mut inputs = Vec::new();
        for (input, dims) in model.ports(false).map_err(fail)? {
            let shape = pin_dynamic_dims(&dims);
            model.reshape_input(&input, &shape).map_err(fail)?;
            inputs.push(TensorSpec::new(input, shape));
        }
        let outputs = model
            .ports(true)
            .map_err(fail)?
            .into_iter()
            .map(|(output, dims)| {
                TensorSpec::new(output, dims.iter().map(|d| d.unwrap_or(-1)).collect())
            })
            .collect();
        let compiled = core
            .compile_model_with_properties(
                &model,
                device,
                &[
                    ("PERFORMANCE_HINT", "LATENCY"),
                    ("INFERENCE_PRECISION_HINT", precision.hint()),
                ],
            )
            .map_err(fail)?;
        let mut request = compiled.create_infer_request().map_err(fail)?;
        let mut input_tensors = Vec::with_capacity(inputs.len());
        for spec in &inputs {
            let len = spec.static_len().ok_or_else(|| {
                InferenceError::ModelCorruption(format!("input `{}` is not static", spec.name))
            })?;
            let tensor = OpenVinoTensor::from_slice_f32(
                &spec.shape,
                &vec![0.0; len],
                core.library().clone(),
            )
            .map_err(fail)?;
            request.set_tensor(&spec.name, &tensor).map_err(fail)?;
            input_tensors.push(tensor);
        }
        Ok(Self {
            request,
            _compiled: compiled,
            inputs,
            outputs,
            input_tensors,
        })
    }
}

impl GraphSession for OpenVinoGraphSession {
    fn inputs(&self) -> &[TensorSpec] {
        &self.inputs
    }

    fn outputs(&self) -> &[TensorSpec] {
        &self.outputs
    }

    fn run(&mut self, inputs: &[&[f32]], outputs: &mut [Vec<f32>]) -> Result<(), InferenceError> {
        for (tensor, data) in self.input_tensors.iter_mut().zip(inputs) {
            let buffer = tensor.as_mut_slice_f32().map_err(|e| run_error(&e))?;
            if buffer.len() != data.len() {
                return Err(InferenceError::InferenceExecution(format!(
                    "input holds {} values, expected {}",
                    data.len(),
                    buffer.len()
                )));
            }
            buffer.copy_from_slice(data);
        }
        self.request.infer().map_err(|e| run_error(&e))?;
        for (slot, spec) in outputs.iter_mut().zip(&self.outputs) {
            let tensor = self
                .request
                .get_tensor(&spec.name)
                .map_err(|e| run_error(&e))?;
            let data = tensor.as_slice_f32().map_err(|e| run_error(&e))?;
            slot.clear();
            slot.extend_from_slice(data);
        }
        Ok(())
    }
}

/// Compiles every graph of a verified manifest export for `device` (precision from
/// [`precision_for_device`]) and resolves the generic executor.
pub fn build_openvino_executor(
    files: &StatefulModelFiles,
    device: &str,
) -> Result<StatefulGraphExecutor<OpenVinoGraphSession>, InferenceError> {
    let core = OpenVinoCore::new().map_err(|e| load_error("*", device, &e))?;
    let available = core
        .available_devices()
        .map_err(|e| load_error("*", device, &e))?;
    if !available.iter().any(|d| d.eq_ignore_ascii_case(device)) {
        return Err(InferenceError::UnsupportedFeature(format!(
            "OpenVINO device {device} not found (OpenVINO lists: {})",
            available.join(", ")
        )));
    }
    let precision = precision_for_device(device);
    realtime_noise_model::stateful_graph::build_executor(files, |name, onnx| {
        OpenVinoGraphSession::load(&core, name, onnx, device, precision)
    })
}
