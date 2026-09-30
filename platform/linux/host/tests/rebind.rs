#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::redundant_clone
)]

use realtime_noise_contracts::{AudioFrame, HOP_SAMPLES, VirtualMicrophone};
use realtime_noise_engine::{GenerationId, GenerationState};
use realtime_noise_linux_host::LinuxVirtualMicrophone;

#[test]
fn recreated_source_requires_rebind_before_audio_is_observed() {
    let mut mic = LinuxVirtualMicrophone::new();
    mic.start().expect("virtual mic should start");

    // Connect a consumer stream (e.g. Teams, OBS)
    let consumer_id = "teams-client";
    mic.register_consumer(consumer_id);
    mic.rebind_consumer(consumer_id)
        .expect("initial bind should succeed");

    // Warm initial generation and push active audio frame
    mic.set_generation(GenerationId::new(1), GenerationState::Active);
    let test_frame: AudioFrame = [0.42; HOP_SAMPLES];
    mic.push_frame(test_frame);

    // Initial state: consumer bound and generation active -> audio is observed
    let audio = mic.read_frame_for_consumer(consumer_id);
    assert_eq!(
        audio, test_frame,
        "consumer should observe active audio before recreation"
    );

    // Recreate PipeWire virtual source node (simulating helper restart / node recreation)
    let new_node_id = mic.recreate_source();
    assert!(new_node_id > 0, "recreated node id must be valid");

    // Consumer stream observes node recreation and requires rebind
    assert!(
        mic.consumer_needs_rebind(consumer_id),
        "consumer stream must observe node recreation and require rebind"
    );

    // BEFORE rebind: consumer MUST receive fail-closed digital silence
    let silence_before_rebind = mic.read_frame_for_consumer(consumer_id);
    assert_eq!(
        silence_before_rebind, [0.0; HOP_SAMPLES],
        "consumer stream must receive digital silence before rebind"
    );

    // Rebind consumer to the new node ID
    mic.rebind_consumer(consumer_id)
        .expect("rebind should succeed");
    assert!(
        !mic.consumer_needs_rebind(consumer_id),
        "consumer should no longer need rebind after rebind operation"
    );

    // AFTER rebind but BEFORE valid warmed generation: MUST STILL receive digital silence
    assert!(
        mic.generation_state() == GenerationState::Warming,
        "new generation after node recreation must start in Warming state"
    );
    let silence_during_warming = mic.read_frame_for_consumer(consumer_id);
    assert_eq!(
        silence_during_warming, [0.0; HOP_SAMPLES],
        "consumer stream must receive digital silence during generation warming"
    );

    // Warm generation to Active
    mic.warm_generation();
    assert_eq!(
        mic.generation_state(),
        GenerationState::Active,
        "generation must transition to Active after warming"
    );

    // AFTER valid warmed generation: consumer now receives processed audio!
    let active_audio = mic.read_frame_for_consumer(consumer_id);
    assert_eq!(
        active_audio, test_frame,
        "consumer stream must observe valid audio only after rebind and valid warmed generation"
    );
}
