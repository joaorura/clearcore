#![forbid(unsafe_code)]

use std::time::{Duration, Instant};
use realtime_noise_supervisor::{EngineSupervisor, SupervisorState};

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
        other => panic!("Expected TerminalSafeState, got {:?}", other),
    }

    assert!(!supervisor.can_restart());
    assert!(supervisor.try_restart().is_err());

    // Explicit user reset recovers to running state
    supervisor.reset().expect("reset should succeed");
    assert_eq!(supervisor.state(), &SupervisorState::Running);
    assert!(supervisor.can_restart());
}
