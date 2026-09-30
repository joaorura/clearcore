use realtime_noise_qualification::{GaDecision, PlatformStatus, aggregate};

#[test]
fn one_platform_failure_blocks_simultaneous_ga() {
    let results = [
        PlatformStatus::Passed,
        PlatformStatus::Passed,
        PlatformStatus::Failed,
        PlatformStatus::Passed,
    ];
    assert_eq!(aggregate(&results), GaDecision::Blocked);
}

#[test]
fn all_platforms_passing_grants_simultaneous_ga() {
    let results = [
        PlatformStatus::Passed,
        PlatformStatus::Passed,
        PlatformStatus::Passed,
        PlatformStatus::Passed,
    ];
    assert_eq!(aggregate(&results), GaDecision::Approved);
}

#[test]
fn incomplete_platforms_count_blocks_simultaneous_ga() {
    let results = [PlatformStatus::Passed, PlatformStatus::Passed];
    assert_eq!(aggregate(&results), GaDecision::Blocked);
}
