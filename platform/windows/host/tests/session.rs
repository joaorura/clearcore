#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use realtime_noise_contracts::{
    Discontinuity, EndpointError, EndpointStatus, FrameEnvelope, HOP_SAMPLES, VirtualMicrophone,
    WireFrameEnvelopeV1,
};
use realtime_noise_windows_host::endpoint::{SupervisorState, WindowsVirtualMicrophone};

#[test]
fn session_contention_returns_device_busy_or_unavailable_busy() {
    let device_path = r"\\.\RealtimeNoise_TestContention";

    let mut mic1 = WindowsVirtualMicrophone::with_device_path(device_path);
    let mut mic2 = WindowsVirtualMicrophone::with_device_path(device_path);

    // Initial status should be Silence
    assert_eq!(mic1.status(), EndpointStatus::Silence);
    assert_eq!(mic2.status(), EndpointStatus::Silence);

    // mic1 acquires session
    mic1.start().expect("mic1 must acquire session ownership");
    assert_eq!(mic1.status(), EndpointStatus::Active);

    // mic2 attempts to acquire session on the same device path while mic1 is active
    let err = mic2
        .start()
        .expect_err("mic2 must fail due to session contention");
    assert_eq!(
        err,
        EndpointError::DeviceBusy,
        "Session contention must return EndpointError::DeviceBusy"
    );
    assert_eq!(
        mic2.status(),
        EndpointStatus::UnavailableBusy,
        "Conflicted endpoint must reflect EndpointStatus::UnavailableBusy"
    );

    // mic1 releases session
    mic1.stop().expect("mic1 stop must release session");
    assert_eq!(mic1.status(), EndpointStatus::Closed);

    // Now mic2 should be able to acquire session
    mic2.start()
        .expect("mic2 must succeed after mic1 releases session");
    assert_eq!(mic2.status(), EndpointStatus::Active);

    // Cleanup
    mic2.stop().expect("mic2 stop must succeed");
    assert_eq!(mic2.status(), EndpointStatus::Closed);
}

#[test]
fn digital_silence_policy_for_unavailable_restarting_and_terminal() {
    let device_path = r"\\.\RealtimeNoise_TestSilence";
    let mut mic = WindowsVirtualMicrophone::with_device_path(device_path);

    mic.start().expect("mic must start");

    // Helper frame with non-zero audio content
    let non_zero_samples = [0.42_f32; HOP_SAMPLES];
    let frame = FrameEnvelope {
        samples: non_zero_samples,
        sequence: 1,
        capture_monotonic_ns: 10_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };

    // 1. Running state: non-zero samples are preserved in encoded wire envelope
    mic.set_supervisor_state(SupervisorState::Running);
    let encoded = mic
        .encode_and_submit(&frame)
        .expect("submission in Running state must succeed");
    let decoded = WireFrameEnvelopeV1::decode(&encoded).expect("must decode valid envelope");
    assert_eq!(decoded.samples[0], 0.42_f32);
    assert_eq!(decoded.samples[HOP_SAMPLES - 1], 0.42_f32);

    // 2. EngineUnavailable state: pure digital silence (all zeros) must be enforced
    mic.set_supervisor_state(SupervisorState::EngineUnavailable);
    let frame_seq2 = FrameEnvelope {
        samples: non_zero_samples,
        sequence: 2,
        capture_monotonic_ns: 20_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    let encoded_silence_unavailable = mic
        .encode_and_submit(&frame_seq2)
        .expect("submission in EngineUnavailable must succeed with silence");
    let decoded_unavailable = WireFrameEnvelopeV1::decode(&encoded_silence_unavailable)
        .expect("must decode valid envelope");
    for (i, &s) in decoded_unavailable.samples.iter().enumerate() {
        assert_eq!(
            s, 0.0_f32,
            "EngineUnavailable must produce digital silence at index {i}"
        );
    }

    // 3. Restarting state: pure digital silence (all zeros) must be enforced
    mic.set_supervisor_state(SupervisorState::Restarting {
        attempt: 1,
        next_retry_ms: 500,
    });
    let frame_seq3 = FrameEnvelope {
        samples: non_zero_samples,
        sequence: 3,
        capture_monotonic_ns: 30_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    let encoded_silence_restarting = mic
        .encode_and_submit(&frame_seq3)
        .expect("submission in Restarting must succeed with silence");
    let decoded_restarting = WireFrameEnvelopeV1::decode(&encoded_silence_restarting)
        .expect("must decode valid envelope");
    for (i, &s) in decoded_restarting.samples.iter().enumerate() {
        assert_eq!(
            s, 0.0_f32,
            "Restarting must produce digital silence at index {i}"
        );
    }

    // 4. TerminalSafeState: pure digital silence (all zeros) must be enforced
    mic.set_supervisor_state(SupervisorState::TerminalSafeState {
        reason: "Crash loop limit reached".to_string(),
        diagnostic: Some("WDF_VIOLATION".to_string()),
    });
    let frame_seq4 = FrameEnvelope {
        samples: non_zero_samples,
        sequence: 4,
        capture_monotonic_ns: 40_000_000,
        generation: 1,
        discontinuity: Discontinuity::NONE,
    };
    let encoded_silence_terminal = mic
        .encode_and_submit(&frame_seq4)
        .expect("submission in TerminalSafeState must succeed with silence");
    let decoded_terminal =
        WireFrameEnvelopeV1::decode(&encoded_silence_terminal).expect("must decode valid envelope");
    for (i, &s) in decoded_terminal.samples.iter().enumerate() {
        assert_eq!(
            s, 0.0_f32,
            "TerminalSafeState must produce digital silence at index {i}"
        );
    }

    mic.stop().expect("stop must succeed");
}
