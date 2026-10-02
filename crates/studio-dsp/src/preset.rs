//! Preset identifiers and the lock-free control handle shared between threads.

use std::sync::atomic::{AtomicU8, Ordering};

/// Studio finishing preset. `Off` is a bit-exact pass-through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Preset {
    /// Bit-exact pass-through; the chain does not touch the samples.
    Off = 0,
    /// Light compression, no presence boost.
    Natural = 1,
    /// 3:1 compression and +2 dB presence.
    Podcast = 2,
    /// 4:1 compression and a stronger de-esser.
    Broadcast = 3,
}

impl Preset {
    /// Decodes a wire/atomic value; `None` for unknown values.
    #[must_use]
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Off),
            1 => Some(Self::Natural),
            2 => Some(Self::Podcast),
            3 => Some(Self::Broadcast),
            _ => None,
        }
    }

    /// Encodes the preset as its `repr(u8)` discriminant.
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

/// Thread-safe preset selector: the control thread writes, the audio thread reads once per hop.
#[derive(Debug)]
pub struct StudioControl {
    preset: AtomicU8,
}

impl StudioControl {
    /// Creates a control initialised to `preset`.
    #[must_use]
    pub const fn new(preset: Preset) -> Self {
        Self {
            preset: AtomicU8::new(preset.as_u8()),
        }
    }

    /// Selects a preset; visible to the audio thread on its next [`StudioControl::preset`] call.
    pub fn set_preset(&self, preset: Preset) {
        self.preset.store(preset.as_u8(), Ordering::Release);
    }

    /// Returns the selected preset. An unrepresentable stored value falls back to `Off`.
    #[must_use]
    pub fn preset(&self) -> Preset {
        Preset::from_u8(self.preset.load(Ordering::Acquire)).unwrap_or(Preset::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::{Preset, StudioControl};

    #[test]
    fn preset_round_trips_through_u8() {
        for preset in [
            Preset::Off,
            Preset::Natural,
            Preset::Podcast,
            Preset::Broadcast,
        ] {
            assert_eq!(Preset::from_u8(preset.as_u8()), Some(preset));
        }
    }

    #[test]
    fn preset_discriminants_are_stable_wire_values() {
        assert_eq!(Preset::Off.as_u8(), 0);
        assert_eq!(Preset::Natural.as_u8(), 1);
        assert_eq!(Preset::Podcast.as_u8(), 2);
        assert_eq!(Preset::Broadcast.as_u8(), 3);
    }

    #[test]
    fn preset_rejects_unknown_values() {
        assert_eq!(Preset::from_u8(4), None);
        assert_eq!(Preset::from_u8(255), None);
    }

    #[test]
    fn control_stores_and_returns_the_selected_preset() {
        let control = StudioControl::new(Preset::Off);
        assert_eq!(control.preset(), Preset::Off);
        control.set_preset(Preset::Broadcast);
        assert_eq!(control.preset(), Preset::Broadcast);
    }
}
