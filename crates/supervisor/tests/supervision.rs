#![forbid(unsafe_code)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::float_cmp
)]

use std::time::{Duration, Instant};
use realtime_noise_engine::DenoiseMode;
use realtime_noise_supervisor::{
    BackoffTracker, EngineSupervisor, SupervisorState, BACKOFF_SECONDS,
};

#[test]
fn sixth_crash_in_fifteen_minutes_enters_terminal_safe_state() {
    let mut supervisor = EngineSupervisor::default();
    let now = Instant::now();

    for i in 1..=5 {
        supervisor.record_crash("engine panic", now + Duration::from_secs(i));
        assert!(!supervisor.is_terminal());
    }

    // 6th crash within 15 minutes window
    supervisor.record_crash("engine panic", now + Duration::from_secs(6));
    assert!(supervisor.is_terminal());

    match supervisor.state() {
        SupervisorState::TerminalSafeState { reason, .. } => {
            assert!(reason.contains("crash limit"));
        }
        other => panic!("Expected TerminalSafeState, got {other:?}"),
    }

    assert!(!supervisor.can_restart());
    assert!(supervisor.try_restart().is_err());

    // Explicit user reset recovers to running state
    supervisor.reset().expect("reset should succeed");
    assert_eq!(supervisor.state(), &SupervisorState::Running);
    assert!(supervisor.can_restart());
}

#[test]
fn backoff_delay_progresses_exponentially_up_to_sixteen_seconds() {
    let mut tracker = BackoffTracker::new();
    let now = Instant::now();

    let expected_delays = [1, 2, 4, 8, 16];
    assert_eq!(expected_delays, BACKOFF_SECONDS);

    for (idx, &expected_sec) in expected_delays.iter().enumerate() {
        tracker.record_crash(now + Duration::from_secs(idx as u64));
        assert_eq!(
            tracker.next_backoff_delay(),
            Duration::from_secs(expected_sec)
        );
    }
}

#[test]
fn crashes_older_than_fifteen_minutes_do_not_trigger_terminal_safe_state() {
    let mut supervisor = EngineSupervisor::default();
    let start = Instant::now();

    // 5 crashes spaced 4 minutes apart (total span: 16 minutes)
    // Crash 0 at 0m
    // Crash 1 at 4m
    // Crash 2 at 8m
    // Crash 3 at 12m
    // Crash 4 at 16m -> at this point, crash 0 (at 0m) is older than 15m and pruned!
    // Crash 5 at 20m -> still only 4-5 crashes in sliding window.
    for i in 0..10 {
        let ts = start + Duration::from_secs(i * 240); // 4 minutes each
        supervisor.record_crash("transient crash", ts);
        assert!(
            !supervisor.is_terminal(),
            "Crash #{i} spaced 4 mins apart should not trigger terminal safe state"
        );
    }
}

#[test]
fn terminal_safe_state_enforces_digital_silence() {
    let mut supervisor = EngineSupervisor::default();
    let mut frame = [0.85f32; 480];

    // While running and Active, audio is unchanged
    supervisor.process_frame_or_silence(&mut frame);
    assert_eq!(frame[0], 0.85);

    // Trigger 6 crashes
    let now = Instant::now();
    for i in 1..=6 {
        supervisor.record_crash("fatal model fault", now + Duration::from_secs(i));
    }
    assert!(supervisor.is_terminal());

    // In terminal safe state, audio endpoint MUST be zeroed out
    supervisor.process_frame_or_silence(&mut frame);
    assert!(frame.iter().all(|&s| s == 0.0));
}

#[test]
fn supervisor_mode_switches_and_diagnostics_retrieval() {
    let mut supervisor = EngineSupervisor::default();
    assert_eq!(supervisor.mode(), DenoiseMode::Active);

    supervisor.set_mode(DenoiseMode::Bypass);
    assert_eq!(supervisor.mode(), DenoiseMode::Bypass);

    supervisor.record_crash("test crash reason", Instant::now());
    let diagnostics = supervisor.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("test crash reason"));
}
