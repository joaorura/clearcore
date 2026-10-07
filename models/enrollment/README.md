# Speaker Enrollment Model (Vocal Biometrics)

## Overview
This directory contains the embedded ONNX model used for speaker enrollment and voice profile extraction in Clearcore.

- **File**: `enrollment.onnx`
- **Architecture**: Deep speaker embedding network (16 kHz mono PCM audio input -> 256-dimensional FiLM conditioning vectors).
- **Format**: ONNX model loaded via `tract-onnx` runtime in `crates/model/src/enrollment.rs`.
- **Target sample rate**: 16,000 Hz.
- **Output embedding**: Normalized 256-element float array embedded directly into `.clearcore-profile` user profiles.
