# Implementation Plan: Task 12 Production macOS CoreAudio & HAL Host Adapter

## Phase 1: Test Suites & RED Contracts
- [x] Task: Create Swift integration test suites
    - [x] Create `platform/macos/tests/hotplug.swift`
    - [x] Create `platform/macos/tests/session.swift`
    - [x] Create `platform/macos/tests/endpoint_formats.swift`

## Phase 2: HAL Driver & Bridge Refinement (GREEN)
- [x] Task: Refine HAL components and atomic ring operations
    - [x] Refine `platform/macos/HAL/Sources/RingBuffer.swift` for atomic generation clearing
    - [x] Refine `platform/macos/HAL/Sources/HiddenOutput.swift` and `VisibleInput.swift`
    - [x] Refine `platform/macos/Bridge/Sources/RealtimeNoiseBridge/EngineXpc.swift`

## Phase 3: Integration Evidence & Gating
- [x] Task: Document macOS integration evidence
    - [x] Create `docs/evidence/macos-integration.md`
- [x] Task: Verify workspace integrity and complete track
