#![forbid(unsafe_code)]
#![allow(clippy::missing_errors_doc, clippy::missing_const_for_fn)]

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES};

use crate::FormatError;

pub const DEFRAMER_CAPACITY_SAMPLES: usize = 1920;

/// Buffers processed 480-sample [`AudioFrame`] buffers and yields arbitrary requested chunk sizes
/// to native audio output callbacks.
///
/// On underrun, drains remaining available samples (if any) and zero-fills the remainder with digital silence.
#[derive(Debug)]
pub struct OutputDeframer {
    buffer: [f32; DEFRAMER_CAPACITY_SAMPLES],
    head: usize,
    len: usize,
    underrun_count: u64,
}

impl Default for OutputDeframer {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputDeframer {
    /// Creates a new `OutputDeframer` with a capacity of 1920 samples (4 frames @ 48 kHz).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: [0.0; DEFRAMER_CAPACITY_SAMPLES],
            head: 0,
            len: 0,
            underrun_count: 0,
        }
    }

    /// Pushes a 480-sample frame into the deframer buffer.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError::CapacityExceeded`] if buffering another frame would exceed capacity.
    pub fn push_frame(&mut self, frame: &AudioFrame) -> Result<(), FormatError> {
        if self.len + HOP_SAMPLES > DEFRAMER_CAPACITY_SAMPLES {
            return Err(FormatError::CapacityExceeded);
        }

        for &sample in frame {
            let write_idx = (self.head + self.len) % DEFRAMER_CAPACITY_SAMPLES;
            self.buffer[write_idx] = sample;
            self.len += 1;
        }

        Ok(())
    }

    /// Fills the destination output slice with samples.
    ///
    /// If there are not enough samples in the buffer (underrun), all available samples
    /// are copied and the remainder is filled with `0.0` (digital silence).
    pub fn fill_slice(&mut self, output: &mut [f32]) {
        let to_drain = output.len().min(self.len);

        for item in &mut output[..to_drain] {
            *item = self.buffer[self.head];
            self.head = (self.head + 1) % DEFRAMER_CAPACITY_SAMPLES;
        }
        self.len -= to_drain;

        if to_drain < output.len() {
            output[to_drain..].fill(0.0);
            self.underrun_count = self.underrun_count.saturating_add(1);
        }
    }

    /// Returns the number of samples available in the deframer.
    #[must_use]
    pub const fn available_samples(&self) -> usize {
        self.len
    }

    /// Returns `true` if the deframer buffer contains no samples.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Returns the total count of underrun occurrences.
    #[must_use]
    pub const fn underrun_count(&self) -> u64 {
        self.underrun_count
    }

    /// Flushes all samples from the deframer.
    pub fn clear(&mut self) {
        self.head = 0;
        self.len = 0;
    }
}
