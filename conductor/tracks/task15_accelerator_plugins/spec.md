# Specification: Task 15 Accelerator Plugins & AUTO Policy

## Overview
Implement hardware accelerator backends (`OpenVINOBackend`, `CudaBackend`, `CoreMlBackend`) and the conservative `AUTO` selection policy.

## Key Requirements
1. **Trait Implementation:** All backends implement `InferenceBackend` from `realtime-noise-model`.
2. **Conservative AUTO Policy:**
   - If an accelerator fails calibration, misses deadlines, or fails quality gates, the policy falls back to the warmed `tract` CPU backend.
   - It never switches to uninitialized or failing backends mid-stream.
3. **Off-Thread Calibration:**
   - Calibration runs out-of-band (5-minute soak or test run).
   - If hardware fails or is absent, reports `NOT_PROMOTED`.
4. **Zero Unsafe:** `#![forbid(unsafe_code)]` across `crates/accelerators`.
5. **Tooling:** Implement `crates/tools/src/bin/calibrate-backend.rs`.
6. **Documentation:** Produce `docs/accelerator-qualification.md`.
