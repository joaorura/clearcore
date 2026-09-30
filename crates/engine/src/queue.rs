#![forbid(unsafe_code)]
#![allow(
    clippy::missing_errors_doc,
    clippy::missing_const_for_fn,
    clippy::significant_drop_tightening
)]

use std::collections::VecDeque;
use std::sync::{Mutex, PoisonError};

use realtime_noise_contracts::{
    DEFAULT_CAPACITY_HOPS, FrameEnvelope, RealtimeTransport, TransportFull,
};

/// High-watermark backlog threshold (2 hops = 20 ms @ 48 kHz).
pub const DEFAULT_WATERMARK_HOPS: usize = 2;

/// Thread-safe, bounded real-time audio transport queue.
#[derive(Debug)]
pub struct BoundedQueueTransport {
    capacity: usize,
    watermark_hops: usize,
    queue: Mutex<VecDeque<FrameEnvelope>>,
    closed_generations: Mutex<Vec<u64>>,
}

impl Default for BoundedQueueTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl BoundedQueueTransport {
    /// Creates a new `BoundedQueueTransport` with default 24-hop capacity and 2-hop watermark.
    #[must_use]
    pub fn new() -> Self {
        Self::with_watermark(DEFAULT_CAPACITY_HOPS, DEFAULT_WATERMARK_HOPS)
    }

    /// Creates a new `BoundedQueueTransport` with custom capacity and default watermark.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_watermark(capacity, DEFAULT_WATERMARK_HOPS)
    }

    /// Creates a new `BoundedQueueTransport` with custom capacity and watermark.
    #[must_use]
    pub fn with_watermark(capacity: usize, watermark_hops: usize) -> Self {
        Self {
            capacity,
            watermark_hops,
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
            closed_generations: Mutex::new(Vec::new()),
        }
    }

    /// Checks if the current queue backlog exceeds the watermark threshold (> 2 hops / 20 ms).
    #[must_use]
    pub fn is_watermark_exceeded(&self) -> bool {
        self.backlog_hops() > self.watermark_hops
    }

    /// Returns the watermark threshold in hops.
    #[must_use]
    pub const fn watermark_hops(&self) -> usize {
        self.watermark_hops
    }

    /// Checks whether the queue is currently empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.backlog_hops() == 0
    }

    /// Flushes all queued frames.
    pub fn clear(&self) {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        queue.clear();
    }

    /// Checks whether a specific generation ID has been marked closed.
    #[must_use]
    pub fn is_generation_closed(&self, generation: u64) -> bool {
        self.closed_generations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .contains(&generation)
    }
}

impl RealtimeTransport for BoundedQueueTransport {
    fn try_push(&self, frame: FrameEnvelope) -> Result<(), TransportFull> {
        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        if queue.len() >= self.capacity {
            return Err(TransportFull);
        }
        queue.push_back(frame);
        drop(queue);
        Ok(())
    }

    fn try_pop(&self) -> Option<FrameEnvelope> {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop_front()
    }

    fn capacity_hops(&self) -> usize {
        self.capacity
    }

    fn backlog_hops(&self) -> usize {
        self.queue
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    fn close_generation(&self, generation: u64) {
        let mut closed = self
            .closed_generations
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !closed.contains(&generation) {
            closed.push(generation);
        }
        drop(closed);

        let mut queue = self.queue.lock().unwrap_or_else(PoisonError::into_inner);
        queue.retain(|f| f.generation != generation);
        drop(queue);
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use realtime_noise_contracts::{Discontinuity, HOP_SAMPLES};

    const fn test_frame(seq: u64, generation: u64) -> FrameEnvelope {
        FrameEnvelope {
            samples: [0.0; HOP_SAMPLES],
            sequence: seq,
            capture_monotonic_ns: seq * 10_000_000,
            generation,
            discontinuity: Discontinuity::NONE,
        }
    }

    #[test]
    fn bounded_capacity_is_twenty_four() {
        let transport = BoundedQueueTransport::new();
        assert_eq!(transport.capacity_hops(), DEFAULT_CAPACITY_HOPS);
        assert_eq!(transport.capacity_hops(), 24);
        assert_eq!(transport.backlog_hops(), 0);
        assert!(transport.is_empty());
    }

    #[test]
    fn try_push_full_returns_transport_full() {
        let transport = BoundedQueueTransport::with_capacity(3);
        assert!(transport.try_push(test_frame(1, 1)).is_ok());
        assert!(transport.try_push(test_frame(2, 1)).is_ok());
        assert!(transport.try_push(test_frame(3, 1)).is_ok());
        assert_eq!(transport.backlog_hops(), 3);

        let res = transport.try_push(test_frame(4, 1));
        assert_eq!(res, Err(TransportFull));
        assert_eq!(transport.backlog_hops(), 3);
    }

    #[test]
    fn watermark_detected_when_backlog_exceeds_two() {
        let transport = BoundedQueueTransport::new();
        assert!(!transport.is_watermark_exceeded());

        assert!(transport.try_push(test_frame(1, 1)).is_ok());
        assert!(transport.try_push(test_frame(2, 1)).is_ok());
        assert!(!transport.is_watermark_exceeded());

        assert!(transport.try_push(test_frame(3, 1)).is_ok());
        assert!(transport.is_watermark_exceeded());
    }

    #[test]
    fn close_generation_flushes_stale_frames() {
        let transport = BoundedQueueTransport::new();
        assert!(transport.try_push(test_frame(1, 1)).is_ok());
        assert!(transport.try_push(test_frame(2, 1)).is_ok());
        assert!(transport.try_push(test_frame(3, 2)).is_ok());
        assert_eq!(transport.backlog_hops(), 3);

        transport.close_generation(1);
        assert!(transport.is_generation_closed(1));
        assert_eq!(transport.backlog_hops(), 1);

        let remaining = transport.try_pop().expect("frame from gen 2 should remain");
        assert_eq!(remaining.sequence, 3);
        assert_eq!(remaining.generation, 2);
    }
}
