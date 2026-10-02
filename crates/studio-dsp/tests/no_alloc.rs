//! Proves that `StudioChain::process` and `StudioChain::set_preset` never allocate.
//!
//! The allocator is process-wide, so this file must keep exactly one `#[test]` that measures: any
//! other test running in parallel would pollute the counters.

#![forbid(unsafe_code)]

use std::hint::black_box;

use stats_alloc::{INSTRUMENTED_SYSTEM, Region, StatsAlloc};
use studio_dsp::{HOP_SAMPLES, Preset, StudioChain};

#[global_allocator]
static GLOBAL: &StatsAlloc<std::alloc::System> = &INSTRUMENTED_SYSTEM;

const HOPS: usize = 600;
const PRESETS: [Preset; 4] = [
    Preset::Off,
    Preset::Natural,
    Preset::Podcast,
    Preset::Broadcast,
];

/// Deterministic pseudo-random hops in `[-1.0, 1.0]` (xorshift64), built before measuring.
fn input_hops() -> Vec<[f32; HOP_SAMPLES]> {
    let mut state: u64 = 0x1234_5678_9ABC_DEF1;
    (0..HOPS)
        .map(|_| {
            let mut hop = [0.0_f32; HOP_SAMPLES];
            for sample in &mut hop {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                *sample = f32::from((state >> 48) as u16) / 32_768.0 - 1.0;
            }
            hop
        })
        .collect()
}

fn allocations_in(work: impl FnOnce()) -> usize {
    let region = Region::new(GLOBAL);
    work();
    let change = region.change();
    change.allocations + change.reallocations
}

#[test]
fn processing_and_switching_presets_never_allocate() {
    // Control: the instrumented allocator really counts.
    let control = allocations_in(|| {
        black_box(Vec::<u8>::with_capacity(64));
    });
    assert!(control >= 1, "the counting allocator is not installed");

    let input = input_hops();
    let mut chain = StudioChain::new(Preset::Natural);
    let mut allocating_hops = 0_usize;
    let mut checksum = 0_u32;
    for (index, source) in input.iter().enumerate() {
        let mut hop = *source;
        let count = allocations_in(|| {
            if index % 5 == 0 {
                chain.set_preset(PRESETS[(index / 5) % PRESETS.len()]);
            }
            chain.process(&mut hop);
        });
        allocating_hops += usize::from(count > 0);
        checksum ^= hop[0].to_bits();
    }
    black_box(checksum);
    assert_eq!(allocating_hops, 0, "hops that allocated");
}
