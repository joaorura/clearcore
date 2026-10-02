use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};
use realtime_noise_model::InferenceError;
use realtime_noise_runtime_coreml::{ComputeUnit, CoreMlError, CoreMlModelRunner, CoreMlTensor};
use std::path::Path;

#[test]
fn compute_unit_mappings_and_properties() {
    assert_eq!(ComputeUnit::from_str_name("all"), ComputeUnit::All);
    assert_eq!(ComputeUnit::from_str_name("auto"), ComputeUnit::All);
    assert_eq!(
        ComputeUnit::from_str_name("ane"),
        ComputeUnit::CpuAndNeuralEngine
    );
    assert_eq!(
        ComputeUnit::from_str_name("npu"),
        ComputeUnit::CpuAndNeuralEngine
    );
    assert_eq!(ComputeUnit::from_str_name("gpu"), ComputeUnit::CpuAndGpu);
    assert_eq!(ComputeUnit::from_str_name("metal"), ComputeUnit::CpuAndGpu);
    assert_eq!(ComputeUnit::from_str_name("cpu"), ComputeUnit::CpuOnly);

    assert_eq!(ComputeUnit::All.to_ml_compute_units(), 2);
    assert_eq!(ComputeUnit::CpuAndNeuralEngine.to_ml_compute_units(), 3);
    assert_eq!(ComputeUnit::CpuAndGpu.to_ml_compute_units(), 1);
    assert_eq!(ComputeUnit::CpuOnly.to_ml_compute_units(), 0);

    assert!(ComputeUnit::All.is_ane_enabled());
    assert!(ComputeUnit::All.is_gpu_enabled());
    assert!(ComputeUnit::CpuAndNeuralEngine.is_ane_enabled());
    assert!(!ComputeUnit::CpuAndNeuralEngine.is_gpu_enabled());
    assert!(!ComputeUnit::CpuAndGpu.is_ane_enabled());
    assert!(ComputeUnit::CpuAndGpu.is_gpu_enabled());
    assert!(!ComputeUnit::CpuOnly.is_ane_enabled());
    assert!(!ComputeUnit::CpuOnly.is_gpu_enabled());
}

#[test]
fn coreml_tensor_lifecycle_and_contracts() {
    let mut tensor = CoreMlTensor::for_audio_frame();
    assert_eq!(tensor.shape(), &[1, 1, HOP_SAMPLES]);
    assert_eq!(tensor.len(), HOP_SAMPLES);
    assert!(!tensor.is_empty());
    assert!(tensor.verify_finite().is_ok());

    let frame: AudioFrame = [0.125; HOP_SAMPLES];
    assert!(tensor.copy_from_audio_frame(&frame).is_ok());
    let out_frame = tensor.copy_to_audio_frame().expect("valid copy");
    assert_eq!(out_frame, frame);

    // Reject non-finite samples
    tensor.as_mut_slice()[10] = f32::NAN;
    assert!(matches!(
        tensor.verify_finite(),
        Err(CoreMlError::ContractViolation(_))
    ));

    tensor.as_mut_slice()[10] = f32::INFINITY;
    assert!(matches!(
        tensor.verify_finite(),
        Err(CoreMlError::ContractViolation(_))
    ));
}

#[test]
fn runner_simulated_execution_and_contracts() {
    let mut runner = CoreMlModelRunner::new_simulated("df3-ane-v1", ComputeUnit::All);
    assert_eq!(runner.model_name(), "df3-ane-v1");
    assert_eq!(runner.compute_unit(), ComputeUnit::All);
    // A simulated runner copies frames through: it neither runs a model nor is accelerated.
    assert!(!runner.executes_inference());
    assert!(!runner.is_hardware_accelerated());

    // Process valid audio frame (480 samples @ 48kHz mono)
    let input: AudioFrame = [0.05; HOP_SAMPLES];
    let output = runner.run_frame(&input).expect("successful inference");
    assert_eq!(output.len(), HOP_SAMPLES);
    assert!(output.iter().all(|s| s.is_finite()));

    // Process raw slice
    let slice_in = [0.02f32; HOP_SAMPLES];
    let mut slice_out = [0.0f32; HOP_SAMPLES];
    assert!(runner.run_slice(&slice_in, &mut slice_out).is_ok());
    assert_eq!(slice_in, slice_out);

    // Reject non-finite input
    let mut nan_input = [0.0; HOP_SAMPLES];
    nan_input[5] = f32::NAN;
    assert!(matches!(
        runner.run_frame(&nan_input),
        Err(CoreMlError::ContractViolation(_))
    ));

    // Simulated failure
    runner.set_simulated_failure(true);
    assert!(matches!(
        runner.run_frame(&input),
        Err(CoreMlError::SimulatedFailure)
    ));
    runner.set_simulated_failure(false);
    assert!(runner.run_frame(&input).is_ok());
}

#[test]
fn runner_load_missing_file_fails() {
    let result = CoreMlModelRunner::load(
        Path::new("/nonexistent/clearcore_missing_model.mlmodelc"),
        ComputeUnit::CpuAndNeuralEngine,
    );
    assert!(matches!(result, Err(CoreMlError::ModelNotFound(_))));
}

#[test]
fn error_conversions_to_model_inference_error() {
    let err1 = CoreMlError::ModelNotFound(std::path::PathBuf::from("model.mlmodelc"));
    let inf1: InferenceError = err1.into();
    assert!(matches!(inf1, InferenceError::ModelCorruption(_)));

    let err2 = CoreMlError::ContractViolation("nan sample".to_owned());
    let inf2: InferenceError = err2.into();
    assert!(matches!(inf2, InferenceError::InputContract(_)));

    let err3 = CoreMlError::SimulatedFailure;
    let inf3: InferenceError = err3.into();
    assert!(matches!(inf3, InferenceError::InferenceExecution(_)));
}
