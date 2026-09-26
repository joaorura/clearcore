use core::fmt;

use crate::FrameEnvelope;

pub const DEFAULT_CAPACITY_HOPS: usize = 24;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct TransportFull;

impl fmt::Display for TransportFull {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("realtime transport is full")
    }
}

impl std::error::Error for TransportFull {}

pub trait RealtimeTransport: Send + Sync {
    /// Attempts to enqueue a frame without blocking.
    ///
    /// # Errors
    ///
    /// Returns [`TransportFull`] when the bounded transport cannot accept the new frame.
    fn try_push(&self, frame: FrameEnvelope) -> Result<(), TransportFull>;
    fn try_pop(&self) -> Option<FrameEnvelope>;
    fn capacity_hops(&self) -> usize;
    fn backlog_hops(&self) -> usize;
    fn close_generation(&self, generation: u64);
}
