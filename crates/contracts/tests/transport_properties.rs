use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard, PoisonError};

use realtime_noise_contracts::{
    DEFAULT_CAPACITY_HOPS, Discontinuity, FrameEnvelope, RealtimeTransport, TransportFull,
};

struct TestTransport {
    capacity: usize,
    queue: Mutex<VecDeque<FrameEnvelope>>,
}

impl TestTransport {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            queue: Mutex::new(VecDeque::with_capacity(capacity)),
        }
    }

    fn queue(&self) -> MutexGuard<'_, VecDeque<FrameEnvelope>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl RealtimeTransport for TestTransport {
    fn try_push(&self, frame: FrameEnvelope) -> Result<(), TransportFull> {
        let mut queue = self.queue();
        if queue.len() == self.capacity {
            return Err(TransportFull);
        }
        queue.push_back(frame);
        drop(queue);
        Ok(())
    }

    fn try_pop(&self) -> Option<FrameEnvelope> {
        self.queue().pop_front()
    }

    fn capacity_hops(&self) -> usize {
        self.capacity
    }

    fn backlog_hops(&self) -> usize {
        self.queue().len()
    }

    fn close_generation(&self, _generation: u64) {}
}

const fn frame(sequence: u64) -> FrameEnvelope {
    FrameEnvelope {
        samples: [0.0; 480],
        sequence,
        capture_monotonic_ns: sequence * 10_000_000,
        generation: 7,
        discontinuity: Discontinuity::NONE,
    }
}

fn assert_object_safe(transport: &dyn RealtimeTransport) {
    transport.close_generation(7);
}

#[test]
fn realtime_transport_is_object_safe_and_thread_shareable() {
    let transport = TestTransport::new(1);

    assert_object_safe(&transport);
}

#[test]
fn default_capacity_is_twenty_four_hops() {
    assert_eq!(DEFAULT_CAPACITY_HOPS, 24);
}

#[test]
fn full_transport_rejects_new_frame_without_removing_consumer_items() {
    let transport = TestTransport::new(2);
    assert_eq!(transport.try_push(frame(1)), Ok(()));
    assert_eq!(transport.try_push(frame(2)), Ok(()));

    let rejected = transport.try_push(frame(3));

    assert_eq!(rejected, Err(TransportFull));
    assert_eq!(transport.backlog_hops(), 2);
    assert_eq!(transport.try_pop().map(|item| item.sequence), Some(1));
    assert_eq!(transport.try_pop().map(|item| item.sequence), Some(2));
}

#[test]
fn transport_preserves_fifo_order() {
    let transport = TestTransport::new(3);
    assert_eq!(transport.try_push(frame(10)), Ok(()));
    assert_eq!(transport.try_push(frame(11)), Ok(()));
    assert_eq!(transport.try_push(frame(12)), Ok(()));

    let sequences = [
        transport.try_pop().map(|item| item.sequence),
        transport.try_pop().map(|item| item.sequence),
        transport.try_pop().map(|item| item.sequence),
    ];

    assert_eq!(sequences, [Some(10), Some(11), Some(12)]);
}

#[test]
fn transport_reports_capacity_and_backlog() {
    let transport = TestTransport::new(3);
    assert_eq!(transport.try_push(frame(1)), Ok(()));

    assert_eq!(transport.capacity_hops(), 3);
    assert_eq!(transport.backlog_hops(), 1);
}

#[test]
fn empty_transport_pop_is_deterministic() {
    let transport = TestTransport::new(1);

    assert!(transport.try_pop().is_none());
    assert!(transport.try_pop().is_none());
}
