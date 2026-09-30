# Specification: Task 17 GA Qualification Matrix

## Overview
Implement the unified GA decision engine and simultaneous qualification matrix across all supported target platforms:
- Windows 11
- macOS 13+ (Ventura, Sonoma, Sequoia)
- Ubuntu 24.04 LTS
- Fedora 42+

## Key Deliverables
1. **Decision Aggregator in `realtime-noise-qualification`:**
   - Function: `aggregate([PlatformResult; 4]) -> GaDecision`
   - Strict rule: any single failure or unverified platform produces `GaDecision::Blocked`. GA requires all 4 platforms to pass simultaneously.
2. **Qualification Test Suite:**
   - `crates/qualification/tests/ga_decision.rs`:
     * `one_platform_failure_blocks_simultaneous_ga`
     * `all_platforms_passing_grants_simultaneous_ga`
3. **Execution Tooling:**
   - `tools/qualification/collect_metrics.rs`: evaluates latency percentiles (p95 <= 80 ms, p99 <= 7 ms), zero soak dropouts, contract compliance.
   - `tools/qualification/run_matrix.py`: executes qualification across the 4 platform profiles and emits evidence reports.
4. **Platform Evidence Reports:**
   - `docs/evidence/ga-matrix.md`: aggregated 4-platform matrix
   - `docs/evidence/windows.md`: Windows 11 WaveRT + Driver Verifier + HLK
   - `docs/evidence/macos.md`: macOS CoreAudio HAL + Developer ID notarization
   - `docs/evidence/ubuntu.md`: Ubuntu 24.04 PipeWire/WirePlumber
   - `docs/evidence/fedora.md`: Fedora 44 (live host) PipeWire 1.6.9
   - `docs/ga-decision.md`: authoritative GA decision document
