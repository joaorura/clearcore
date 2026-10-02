use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};

use crate::error::CoreMlError;

/// Continuous tensor buffer used for CoreML I/O operations.
///
/// Designed to be allocated once during backend initialization to guarantee
/// zero allocations in the real-time audio thread cycle.
#[derive(Debug, Clone, PartialEq)]
pub struct CoreMlTensor {
    shape: Vec<usize>,
    data: Vec<f32>,
}

impl CoreMlTensor {
    /// Allocates a new zero-initialized tensor with the given multidimensional shape.
    pub fn new(shape: &[usize]) -> Result<Self, CoreMlError> {
        if shape.is_empty() {
            return Err(CoreMlError::InvalidShape {
                expected: vec![1, 1, HOP_SAMPLES],
                actual: Vec::new(),
            });
        }
        let total_elements = shape.iter().product();
        Ok(Self {
            shape: shape.to_vec(),
            data: vec![0.0; total_elements],
        })
    }

    /// Allocates a pre-configured tensor matching the Clearcore audio contract:
    /// shape `[1, 1, 480]` (batch=1, channel=1, hop_samples=480).
    #[must_use]
    pub fn for_audio_frame() -> Self {
        Self {
            shape: vec![1, 1, HOP_SAMPLES],
            data: vec![0.0; HOP_SAMPLES],
        }
    }

    /// Multidimensional shape descriptor.
    #[must_use]
    pub fn shape(&self) -> &[usize] {
        &self.shape
    }

    /// Total number of float32 elements in the tensor.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Checks if the tensor is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Immutable slice view over the underlying buffer.
    #[must_use]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Mutable slice view over the underlying buffer.
    pub fn as_mut_slice(&mut self) -> &mut [f32] {
        &mut self.data
    }

    /// Copies samples into this tensor from an external slice.
    pub fn copy_from_slice(&mut self, src: &[f32]) -> Result<(), CoreMlError> {
        if src.len() != self.data.len() {
            return Err(CoreMlError::InvalidShape {
                expected: self.shape.clone(),
                actual: vec![src.len()],
            });
        }
        self.data.copy_from_slice(src);
        Ok(())
    }

    /// Copies samples from this tensor into an external slice.
    pub fn copy_to_slice(&self, dst: &mut [f32]) -> Result<(), CoreMlError> {
        if dst.len() != self.data.len() {
            return Err(CoreMlError::InvalidShape {
                expected: self.shape.clone(),
                actual: vec![dst.len()],
            });
        }
        dst.copy_from_slice(&self.data);
        Ok(())
    }

    /// Copies samples from a contract-standard [`AudioFrame`] (480 samples).
    pub fn copy_from_audio_frame(&mut self, frame: &AudioFrame) -> Result<(), CoreMlError> {
        if self.data.len() < HOP_SAMPLES {
            return Err(CoreMlError::ContractViolation(
                "tensor capacity smaller than AudioFrame hop size".to_owned(),
            ));
        }
        self.data[..HOP_SAMPLES].copy_from_slice(frame);
        Ok(())
    }

    /// Copies samples into a new contract-standard [`AudioFrame`] (480 samples).
    pub fn copy_to_audio_frame(&self) -> Result<AudioFrame, CoreMlError> {
        if self.data.len() < HOP_SAMPLES {
            return Err(CoreMlError::ContractViolation(
                "tensor contains fewer samples than AudioFrame hop size".to_owned(),
            ));
        }
        let mut frame = [0.0; HOP_SAMPLES];
        frame.copy_from_slice(&self.data[..HOP_SAMPLES]);
        Ok(frame)
    }

    /// Ensures all samples in the tensor are finite numbers (no NaN or Infinities).
    pub fn verify_finite(&self) -> Result<(), CoreMlError> {
        if self.data.iter().any(|&sample| !sample.is_finite()) {
            Err(CoreMlError::ContractViolation(
                "tensor contains non-finite samples (NaN or Inf)".to_owned(),
            ))
        } else {
            Ok(())
        }
    }
}
