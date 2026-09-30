#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};

use crate::FormatError;

pub const ACCUMULATOR_CAPACITY_SAMPLES: usize = 1920;

/// Bounded circular buffer that accumulates variable-sized native audio chunks
/// into fixed 480-sample [`AudioFrame`] buffers.
#[derive(Debug)]
pub struct InputAccumulator {
    buffer: [f32; ACCUMULATOR_CAPACITY_SAMPLES],
    head: usize,
    len: usize,
}

impl Default for InputAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

impl InputAccumulator {
    /// Creates a new `InputAccumulator` with a capacity of 1920 samples (4 frames @ 48 kHz).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: [0.0; ACCUMULATOR_CAPACITY_SAMPLES],
            head: 0,
            len: 0,
        }
    }

    /// Pushes a slice of samples into the accumulator.
    ///
    /// If pushing exceeds the capacity, samples that fit are written and excess are discarded
    /// to avoid unbounded memory allocation. Use [`Self::try_push_samples`] for fail-closed rejection.
    pub fn push_samples(&mut self, samples: &[f32]) {
        let available_space = ACCUMULATOR_CAPACITY_SAMPLES.saturating_sub(self.len);
        let to_push = samples.len().min(available_space);

        for &sample in &samples[..to_push] {
            let write_idx = (self.head + self.len) % ACCUMULATOR_CAPACITY_SAMPLES;
            self.buffer[write_idx] = sample;
            self.len += 1;
        }
    }

    /// Attempts to push all samples in the slice, failing closed if accumulator capacity is exceeded.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::CapacityExceeded`] if `self.len + samples.len() > ACCUMULATOR_CAPACITY_SAMPLES`.
    pub fn try_push_samples(&mut self, samples: &[f32]) -> Result<(), FormatError> {
        if self.len + samples.len() > ACCUMULATOR_CAPACITY_SAMPLES {
            return Err(FormatError::CapacityExceeded);
        }
        self.push_samples(samples);
        Ok(())
    }

    /// Pops a fixed 480-sample frame if at least 480 samples are available.
    pub fn pop_frame(&mut self) -> Option<AudioFrame> {
        if self.len < HOP_SAMPLES {
            return None;
        }

        let mut frame = [0.0f32; HOP_SAMPLES];
        for item in &mut frame {
            *item = self.buffer[self.head];
            self.head = (self.head + 1) % ACCUMULATOR_CAPACITY_SAMPLES;
        }
        self.len -= HOP_SAMPLES;

        Some(frame)
    }

    /// Returns the number of unpopped samples currently in the accumulator.
    #[must_use]
    pub const fn available_samples(&self) -> usize {
        self.len
    }

    /// Returns `true` if the accumulator contains no samples.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Flushes all samples from the accumulator.
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}
