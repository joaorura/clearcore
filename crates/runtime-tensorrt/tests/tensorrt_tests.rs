#![allow(clippy::unwrap_used, clippy::expect_used)]

use realtime_noise_contracts::HOP_SAMPLES;
use realtime_noise_runtime_tensorrt::cuda::CudaDriver;
use realtime_noise_runtime_tensorrt::{
    CudaContext, CudaDevice, CudaStream, GpuBuffer, PrecisionTarget, TensorRtContext,
    TensorRtError, TensorRtInferenceContext, is_cuda_driver_available, is_tensorrt_available,
};

#[test]
fn test_precision_target_attributes() {
    assert_eq!(PrecisionTarget::Fp16.as_str(), "fp16");
    assert_eq!(PrecisionTarget::Fp8.as_str(), "fp8");
    assert_eq!(PrecisionTarget::Fp32.as_str(), "fp32");

    assert!(PrecisionTarget::Fp16.is_sm120_accelerated());
    assert!(PrecisionTarget::Fp8.is_sm120_accelerated());
    assert!(!PrecisionTarget::Fp32.is_sm120_accelerated());

    assert_eq!(PrecisionTarget::Fp16.bytes_per_sample(), 2);
    assert_eq!(PrecisionTarget::Fp8.bytes_per_sample(), 1);
    assert_eq!(PrecisionTarget::Fp32.bytes_per_sample(), 4);

    assert_eq!(
        PrecisionTarget::from_name("fp16"),
        Some(PrecisionTarget::Fp16)
    );
    assert_eq!(
        PrecisionTarget::from_name("fp8"),
        Some(PrecisionTarget::Fp8)
    );
    assert_eq!(
        PrecisionTarget::from_name("fp32"),
        Some(PrecisionTarget::Fp32)
    );
    assert_eq!(PrecisionTarget::from_name("invalid"), None);
}

#[test]
fn test_blackwell_sm120_mock_device() {
    let dev = CudaDevice::mock_blackwell_sm120();
    assert_eq!(dev.ordinal(), 0);
    assert!(dev.name().contains("Blackwell"));
    assert_eq!(dev.compute_capability(), (12, 0));
    assert_eq!(dev.sm_string(), "sm_120");
    assert!(dev.is_blackwell());
    assert!(dev.is_sm120());

    let supported = dev.supported_precisions();
    assert!(supported.contains(&PrecisionTarget::Fp8));
    assert!(supported.contains(&PrecisionTarget::Fp16));
    assert!(supported.contains(&PrecisionTarget::Fp32));
    assert!(dev.supports_precision(PrecisionTarget::Fp8));
    assert!(dev.supports_precision(PrecisionTarget::Fp16));
}

#[test]
fn test_mock_tensorrt_inference_fp16() {
    let mut ctx = TensorRtContext::new_mock_sm120_fp16();
    assert!(ctx.is_mock());
    assert_eq!(ctx.precision(), PrecisionTarget::Fp16);
    assert_eq!(ctx.device().sm_string(), "sm_120");

    let input = [0.125f32; HOP_SAMPLES];
    let mut output = [0.0f32; HOP_SAMPLES];

    ctx.process_frame(&input, &mut output).unwrap();
    assert_eq!(output.len(), HOP_SAMPLES);
    for sample in &output {
        assert!(sample.is_finite());
        assert!((*sample - 0.125).abs() < 1e-3);
    }
}

#[test]
fn test_mock_tensorrt_inference_fp8() {
    let mut ctx = TensorRtContext::new_mock_sm120_fp8();
    assert!(ctx.is_mock());
    assert_eq!(ctx.precision(), PrecisionTarget::Fp8);

    let input = [0.25f32; HOP_SAMPLES];
    let mut output = [0.0f32; HOP_SAMPLES];

    ctx.process_frame(&input, &mut output).unwrap();
    assert_eq!(output.len(), HOP_SAMPLES);
    for sample in &output {
        assert!(sample.is_finite());
        assert!((*sample - 0.25).abs() < 0.05);
    }
}

#[test]
fn test_fail_closed_zero_bypass_on_nan() {
    let mut ctx = TensorRtInferenceContext::new_mock_sm120_fp16();

    let mut nan_input = [0.1f32; HOP_SAMPLES];
    nan_input[10] = f32::NAN;
    let mut output = [1.0f32; HOP_SAMPLES];

    let res = ctx.process_frame(&nan_input, &mut output);
    assert!(res.is_err());
    // Zero raw bypass invariant: output is strictly zeroed on error
    assert!(output.iter().all(|&s| s == 0.0));
}

#[test]
fn test_simulated_failure_enforces_silence() {
    let mut ctx = TensorRtInferenceContext::new_mock_sm120_fp16();
    ctx.set_simulated_failure(true);

    let input = [0.5f32; HOP_SAMPLES];
    let mut output = [1.0f32; HOP_SAMPLES];

    let res = ctx.process_frame(&input, &mut output);
    assert!(matches!(res, Err(TensorRtError::ExecutionFailed(_))));
    assert!(output.iter().all(|&s| s == 0.0));
}

#[test]
fn test_hardware_availability_probes() {
    let cuda_ok = is_cuda_driver_available();
    let trt_ok = is_tensorrt_available();
    println!("Host probe - CUDA: {cuda_ok}, TensorRT: {trt_ok}");
}

#[test]
fn test_precision_policy_prefers_fp8_then_fp16_then_int8_never_fp32() {
    // Blackwell consumer / workstation, Hopper, Ada: FP8 tensor cores.
    for cc in [(12, 0), (10, 0), (9, 0), (8, 9)] {
        let dev = CudaDevice::mock("fp8-capable", cc);
        assert_eq!(
            dev.preferred_precision(),
            Some(PrecisionTarget::Fp8),
            "{cc:?}"
        );
    }
    // Ampere, Turing, Volta: no FP8, fast FP16.
    for cc in [(8, 6), (8, 0), (7, 5), (7, 0)] {
        let dev = CudaDevice::mock("fp16-capable", cc);
        assert_eq!(
            dev.preferred_precision(),
            Some(PrecisionTarget::Fp16),
            "{cc:?}"
        );
        assert!(!dev.supports_precision(PrecisionTarget::Fp8));
    }
    // Pascal consumer: DP4A INT8 only.
    let pascal = CudaDevice::mock("pascal", (6, 1));
    assert_eq!(pascal.preferred_precision(), Some(PrecisionTarget::Int8));
    assert!(!pascal.supports_precision(PrecisionTarget::Fp16));
    // Maxwell: nothing allowed on GPU, FP32 is not an automatic choice.
    let maxwell = CudaDevice::mock("maxwell", (5, 2));
    assert_eq!(maxwell.preferred_precision(), None);
    assert!(maxwell.supports_precision(PrecisionTarget::Fp32));
}

#[test]
fn test_int8_precision_attributes_and_mock_inference() {
    assert_eq!(PrecisionTarget::Int8.as_str(), "int8");
    assert_eq!(PrecisionTarget::Int8.bytes_per_sample(), 1);
    assert_eq!(
        PrecisionTarget::from_name("INT8"),
        Some(PrecisionTarget::Int8)
    );

    let pascal = CudaDevice::mock("pascal", (6, 1));
    let mut ctx = TensorRtContext::new_mock_preferred(pascal).unwrap();
    assert_eq!(ctx.precision(), PrecisionTarget::Int8);

    let input = [0.5f32; HOP_SAMPLES];
    let mut output = [0.0f32; HOP_SAMPLES];
    ctx.process_frame(&input, &mut output).unwrap();
    assert!(output.iter().all(|s| (*s - 0.5).abs() < 0.01));
}

#[test]
fn test_mock_preferred_is_none_without_gpu_precision() {
    let maxwell = CudaDevice::mock("maxwell", (5, 2));
    assert!(TensorRtContext::new_mock_preferred(maxwell).is_none());
    let blackwell = TensorRtContext::new_mock_preferred(CudaDevice::mock_blackwell_sm120());
    assert_eq!(blackwell.unwrap().precision(), PrecisionTarget::Fp8);
}

#[test]
fn test_set_precision_rejects_unsupported_target() {
    let mut ctx =
        TensorRtContext::new_mock(CudaDevice::mock("turing", (7, 5)), PrecisionTarget::Fp16);
    let err = ctx.set_precision(PrecisionTarget::Fp8).unwrap_err();
    assert!(matches!(err, TensorRtError::UnsupportedPrecision { .. }));
    assert_eq!(ctx.precision(), PrecisionTarget::Fp16);
}

#[test]
fn test_frame_length_mismatch_zeroes_output() {
    let mut ctx = TensorRtContext::new_mock_sm120_fp16();
    let input = [0.1f32; HOP_SAMPLES - 1];
    let mut output = [1.0f32; HOP_SAMPLES];
    assert!(ctx.process_frame(&input, &mut output).is_err());
    assert!(output.iter().all(|&s| s == 0.0));
}

#[test]
fn test_mock_context_does_not_claim_hardware_or_engine_execution() {
    let ctx = TensorRtContext::new_mock_sm120_fp8();
    assert!(!ctx.is_hardware_backed());
    // Known gap: no TensorRT engine is executed by any path yet.
    assert!(!ctx.executes_engine());
}

#[test]
fn test_gpu_buffer_allocation_skips_without_cuda() {
    if !is_cuda_driver_available() {
        eprintln!("CUDA driver not found; skipping GpuBuffer test");
        return;
    }
    let Ok(driver) = CudaDriver::load() else {
        eprintln!("CUDA driver failed to initialise; skipping GpuBuffer test");
        return;
    };
    // Zero-length allocations never touch VRAM and must be empty.
    let empty = GpuBuffer::<f32>::allocate(std::sync::Arc::clone(&driver), 0).unwrap();
    assert!(empty.is_empty());
    assert_eq!(empty.size_bytes(), 0);
}

#[test]
fn test_gpu_buffer_safe_read_back_round_trips_and_checks_length() {
    let Ok(driver) = CudaDriver::load() else {
        eprintln!("CUDA driver unavailable; skipping GpuBuffer round trip");
        return;
    };
    let Ok(context) = CudaContext::new(std::sync::Arc::clone(&driver), 0) else {
        eprintln!("CUDA context unavailable; skipping GpuBuffer round trip");
        return;
    };
    context.make_current().unwrap();
    let stream = CudaStream::new(std::sync::Arc::clone(&driver)).unwrap();
    let mut buffer = GpuBuffer::<f32>::allocate(driver, HOP_SAMPLES).unwrap();

    let source: Vec<f32> = (0..HOP_SAMPLES).map(|i| i as f32 * 0.5).collect();
    buffer.async_copy_from_host(&source, &stream).unwrap();
    let mut readback = vec![-1.0f32; HOP_SAMPLES];
    buffer.copy_to_host(&mut readback, &stream).unwrap();
    assert_eq!(readback, source);

    let mut short = vec![0.0f32; HOP_SAMPLES - 1];
    assert!(buffer.copy_to_host(&mut short, &stream).is_err());
}

#[test]
fn test_new_auto_follows_policy_or_fails_gracefully() {
    match TensorRtInferenceContext::new_auto(0) {
        Ok(ctx) => {
            assert!(ctx.is_hardware_backed());
            assert_eq!(ctx.device().preferred_precision(), Some(ctx.precision()));
            let input = [0.25f32; HOP_SAMPLES];
            let mut output = [9.0f32; HOP_SAMPLES];
            let mut ctx = ctx;
            ctx.process_frame(&input, &mut output).unwrap();
            assert!(output.iter().all(|s| s.is_finite()));
        }
        Err(err) => {
            // No NVIDIA stack on this host: the error must be a typed library/driver error.
            eprintln!("TensorRT hardware path unavailable (graceful): {err}");
            assert!(matches!(
                err,
                TensorRtError::LibraryNotFound(_)
                    | TensorRtError::Cuda(_)
                    | TensorRtError::NoSupportedPrecision { .. }
                    | TensorRtError::InitializationFailed(_)
                    | TensorRtError::SymbolNotFound(_)
            ));
        }
    }
}
