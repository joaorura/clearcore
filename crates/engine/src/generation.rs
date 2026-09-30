#![forbid(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]

use crate::ResetReason;

/// Type-safe generation identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct GenerationId(pub u64);

impl GenerationId {
    #[must_use]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl From<u64> for GenerationId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl From<GenerationId> for u64 {
    fn from(generation_id: GenerationId) -> Self {
        generation_id.0
    }
}

/// Lifecycle state of a generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GenerationState {
    #[default]
    Warming,
    Active,
    Closed,
}

/// A generation tracking warm-up, active processing, and closure reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Generation {
    id: GenerationId,
    state: GenerationState,
    warm_up_count: u32,
    closure_reason: Option<ResetReason>,
}

impl Generation {
    #[must_use]
    pub const fn new(id: GenerationId) -> Self {
        Self {
            id,
            state: GenerationState::Warming,
            warm_up_count: 0,
            closure_reason: None,
        }
    }

    #[must_use]
    pub const fn active(id: GenerationId) -> Self {
        Self {
            id,
            state: GenerationState::Active,
            warm_up_count: 1,
            closure_reason: None,
        }
    }

    pub fn mark_warmed(&mut self) {
        if self.state == GenerationState::Warming {
            self.warm_up_count = self.warm_up_count.saturating_add(1);
            self.state = GenerationState::Active;
        }
    }

    pub fn increment_warm_up(&mut self) {
        self.warm_up_count = self.warm_up_count.saturating_add(1);
    }

    pub fn close(&mut self, reason: ResetReason) {
        self.state = GenerationState::Closed;
        self.closure_reason = Some(reason);
    }

    #[must_use]
    pub const fn id(&self) -> GenerationId {
        self.id
    }

    #[must_use]
    pub const fn state(&self) -> GenerationState {
        self.state
    }

    #[must_use]
    pub const fn warm_up_count(&self) -> u32 {
        self.warm_up_count
    }

    #[must_use]
    pub const fn closure_reason(&self) -> Option<ResetReason> {
        self.closure_reason
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        matches!(self.state, GenerationState::Active)
    }

    #[must_use]
    pub const fn is_closed(&self) -> bool {
        matches!(self.state, GenerationState::Closed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_initializes_in_warming_state() {
        let generation = Generation::new(GenerationId::new(1));
        assert_eq!(generation.id().get(), 1);
        assert_eq!(generation.state(), GenerationState::Warming);
        assert_eq!(generation.warm_up_count(), 0);
        assert!(!generation.is_active());
        assert!(!generation.is_closed());
        assert_eq!(generation.closure_reason(), None);
    }

    #[test]
    fn mark_warmed_transitions_to_active() {
        let mut generation = Generation::new(GenerationId::new(1));
        generation.mark_warmed();
        assert_eq!(generation.state(), GenerationState::Active);
        assert_eq!(generation.warm_up_count(), 1);
        assert!(generation.is_active());
        assert!(!generation.is_closed());
    }

    #[test]
    fn close_transitions_to_closed_and_records_reason() {
        let mut generation = Generation::active(GenerationId::new(5));
        assert!(generation.is_active());
        generation.close(ResetReason::InferenceDeadlineMiss);
        assert_eq!(generation.state(), GenerationState::Closed);
        assert!(generation.is_closed());
        assert!(!generation.is_active());
        assert_eq!(
            generation.closure_reason(),
            Some(ResetReason::InferenceDeadlineMiss)
        );
    }

    #[test]
    fn generation_id_increments_cleanly() {
        let id = GenerationId::new(42);
        assert_eq!(id.next().get(), 43);
    }
}
