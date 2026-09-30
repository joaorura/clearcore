# Implementation Plan: Task 9 macOS HAL Audio Server Plug-in Spike

## Phase 1: HAL Plug-in Architecture & Swift Sources
- [x] Task: Create Xcode project structure, Swift sources, and bridge package
    - [x] Create `platform/macos/HAL/RealtimeNoiseHAL.xcodeproj/project.pbxproj`
    - [x] Create `platform/macos/HAL/Sources/RingBuffer.swift`
    - [x] Create `platform/macos/HAL/Sources/HiddenOutput.swift`
    - [x] Create `platform/macos/HAL/Sources/VisibleInput.swift`
    - [x] Create `platform/macos/HAL/Sources/Plugin.swift`
    - [x] Create `platform/macos/Bridge/Package.swift`
    - [x] Create `platform/macos/Bridge/Sources/RealtimeNoiseBridge/EngineXpc.swift`

## Phase 2: Test Scripts & Verification Harness
- [x] Task: Create endpoint test runner script
    - [x] Create `platform/macos/tests/endpoint-spike.sh`

## Phase 3: Evidence Documentation & Gating
- [x] Task: Document spike evidence and physical host gating
    - [x] Create `docs/evidence/macos-spike.md` with CoreAudio contract, ring buffer architecture, and `BLOCKED_PENDING_DEVELOPER_ID` for physical rack
- [x] Task: Verify workspace integrity and complete track

