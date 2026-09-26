use core::ops::{BitOr, BitOrAssign};

pub const SAMPLE_RATE_HZ: u32 = 48_000;
pub const CHANNELS: usize = 1;
pub const HOP_SAMPLES: usize = 480;

pub type AudioFrame = [f32; HOP_SAMPLES];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Discontinuity(u32);

impl Discontinuity {
    pub const NONE: Self = Self(0);
    pub const CAPTURE_DROP: Self = Self(1);
    pub const DEVICE_CHANGE: Self = Self(2);
    pub const GENERATION_CHANGE: Self = Self(4);
    pub const INFERENCE_DEADLINE_MISS: Self = Self(8);

    const KNOWN_BITS: u32 = Self::CAPTURE_DROP.0
        | Self::DEVICE_CHANGE.0
        | Self::GENERATION_CHANGE.0
        | Self::INFERENCE_DEADLINE_MISS.0;

    pub const fn from_bits(bits: u32) -> Option<Self> {
        if bits & !Self::KNOWN_BITS == 0 {
            Some(Self(bits))
        } else {
            None
        }
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl BitOr for Discontinuity {
    type Output = Self;

    fn bitor(self, right: Self) -> Self::Output {
        Self(self.0 | right.0)
    }
}

impl BitOrAssign for Discontinuity {
    fn bitor_assign(&mut self, right: Self) {
        self.0 |= right.0;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameEnvelope {
    pub samples: AudioFrame,
    pub sequence: u64,
    pub capture_monotonic_ns: u64,
    pub generation: u64,
    pub discontinuity: Discontinuity,
}
