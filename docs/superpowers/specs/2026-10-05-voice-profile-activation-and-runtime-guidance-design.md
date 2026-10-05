# ClearCore Voice Profile Activation and Runtime Guidance Design

**Date:** 2026-10-05  
**Status:** Approved direction; implementation pending  
**Repositories:** `clearcore` and `clearcore-train`

## 1. Goal

Finish the existing personalized-voice work without conflating four different states:

1. a voice profile is stored;
2. a profile is applied to the active inference backend;
3. an exported model is technically compatible with ClearCore;
4. a model is approved for production quality and distribution.

The immediate product goal is development-grade end-to-end profile activation with honest status reporting. Production activation remains blocked by the failed M2 quality gates and missing M4/M5 governance evidence.

The independent runtime-guidance work remains in scope because the current TypeScript build is broken and the accelerator UI must not imply that detected hardware or installed vendor libraries are executing ClearCore inference.

## 2. Verified Baseline

### Voice profile runtime

- `IpcCommand::SetVoiceProfile` and `ClearVoiceProfile` exist.
- `ServiceDaemon` validates and persists profiles but does not apply them to audio.
- `GetStatus` intentionally reports `is_voice_profile_active: false`.
- `TractBackend::set_voice_profile` already validates integrity, prepares FiLM/EQ conditioning, sends it to its inference worker, preserves GRU state, and updates `active_profile` only after success.
- `InferenceBackend`, `StudioBackend`, `EngineSupervisor`, and `DenoiseEngine` do not expose a complete profile-control path.
- The packaged Linux audio path also passes through `filter-capi` and the native helper, so the Rust service path alone is not sufficient to claim product-wide end-to-end activation.

### Model assets

- M2 is formally `NO-GO`. Blocking failures include SI-SDR improvement, WER, no-profile TSOS, and wrong-speaker rejection.
- M3 exported deterministic pDFNet3 and enrollment packages from the M2-selected checkpoint. It proves archive/ONNX/Tract compatibility, not quality approval.
- Development artifacts:
  - `pdfnet3-release-asset-v1.tar.gz`: 7,979,862 bytes, SHA-256 `42dfc577fdf8a881ecbafce7777bf6f0a4cf914ffc1aaff2580aec0cbac79505`.
  - `voice-enrollment-asset-v1.tar.gz`: 79,406,455 bytes, SHA-256 `bd4d6dd941f8527b5011a2bae78169148f33155e25d30c5707e88963e7ea824d`.
- ClearCore descriptors for these roles are intentionally unpinned and therefore fail closed.
- M4 controlled evaluation and M5 governance/signing evidence do not exist.

### Frontend and runtime guidance

- The current TypeScript build has three confirmed defects: orphaned copy state, voice-profile IPC envelope normalization, and an incomplete locale type contract.
- CPU Tract is the guaranteed working backend.
- Runtime or device detection does not prove ClearCore inference.
- AMD remains preview/detection-only until a real MIGraphX backend is wired and qualified.

## 3. Design Decisions

### 3.1 Profile control belongs on the backend contract

Add profile control to `InferenceBackend`:

```rust
fn set_voice_profile(
    &mut self,
    profile: Option<&VoiceProfile>,
) -> Result<(), InferenceError>;
```

Every implementation must choose an explicit policy:

- `TractBackend` applies or clears the profile using its existing implementation.
- `StudioBackend<B>` and `Box<T>` delegate to their inner backend.
- Backends without speaker conditioning reject `Some(profile)` with a typed unsupported-feature error and accept `None` as a no-op.
- Test backends implement an explicit deterministic policy; the trait must not silently claim support.

This keeps capability truth at the same boundary that owns inference rather than inferring support from hardware detection or backend names.

### 3.2 Apply profiles outside native audio callbacks

Profile validation and FiLM/EQ preparation allocate, clone data, use channels, and may block. They must never run in a native audio callback.

- `EngineSupervisor::set_voice_profile` applies updates at its serialized processing boundary.
- `DenoiseEngine` receives an owned `VoiceProfileUpdate` command with variants `Set(VoiceProfile)` and `Clear`.
- `DenoiseWorker` applies the pending update between frames before processing the next hop.
- The existing model worker remains responsible for mutating Tract conditioning without resetting GRU state.

No new lock-free snapshot framework or dependency is introduced. The existing worker/channel boundaries already provide the necessary isolation.

### 3.3 Service state is transactional

`ServiceDaemon` keeps the complete active profile in memory, not only its ID. A profile is reported active only after backend application succeeds.

For `SetVoiceProfile`:

1. parse and verify the profile;
2. remember the previously active profile;
3. apply the new profile to the live backend at a frame boundary;
4. persist it atomically with mode `0600`;
5. if persistence fails, restore the previous backend profile and return an error;
6. update the reported active ID only after both operations succeed.

For `ClearVoiceProfile`, apply neutral conditioning first, clear persisted state second, and restore the previous profile if clearing storage fails.

At startup, load and verify the stored profile, then attempt application. Invalid or unsupported profiles remain stored for diagnosis but are not reported active. Audio continues with neutral conditioning and the existing fail-closed processing policy.

### 3.4 Shipping audio paths are separate completion gates

Implementation proceeds in two runtime stages:

1. **Rust control path:** IPC, service, supervisor, asynchronous engine, wrappers, and Tract backend.
2. **Native packaged path:** `filter-capi` and the Linux helper control mechanism, followed by equivalent Windows/macOS verification where those paths differ.

The feature may be called development-integrated after stage 1. It may be called product end-to-end only after the packaged virtual microphone path applies the same profile and passes real audio QA.

### 3.5 M3 assets are development-only

M3 assets may be used from their existing ignored training output only in explicit development/test configuration.

- Verify the full-file SHA-256 and exact archive-member allowlist before loading.
- Record the source checkpoint and M2 `NO-GO` status in test evidence.
- Do not copy the files to `vendor/approved/`.
- Do not add production registry pins, sign manifests, distribute assets, or describe them as production-ready.
- The upstream approved DFNet3 remains the production fallback.

Production promotion requires all of the following:

1. controlled M4 quality evaluation resolving every blocking M2 gate;
2. controlled raw-versus-denoised enrollment ablation;
3. M5 provenance, model card, license/conversion terms, legal review, hashes, sizes, goldens, and resampler information;
4. owner-issued Ed25519 approval signatures;
5. pinned `ModelAssetRegistry` descriptors and complete governance directories.

### 3.6 Enrollment denoise is conditional

The older approved studio design made enrollment denoising conditional on ablation evidence. The later continuous-enrollment spec called it mandatory, but M1's indicative experiment moved embeddings farther from clean speech at every tested SNR and used a confounded reference path.

Until M4 resolves the question:

- raw enrollment is the default input to the enrollment model;
- denoised enrollment is an explicit development experiment, never silently enabled;
- the controlled ablation must process reference and test sides consistently;
- UI copy must not claim denoising improves identity extraction before evidence exists.

VAD trimming and signal-quality validation remain mandatory regardless of denoise selection.

### 3.7 Runtime guidance remains manual and evidence-based

The accelerator UI uses distinct states:

- hardware detected;
- runtime detected;
- vendor device visible;
- ClearCore inference verified;
- preview/detection-only.

Installation remains user-controlled. ClearCore may show copyable diagnostic commands and official vendor links, but it does not execute privileged installers. Python imports, package discovery, and vendor CLIs are diagnostics, not proof of ClearCore inference. CPU Tract remains visible as the working fallback.

## 4. Data Flow

```text
Enrollment audio
  -> VAD/quality validation
  -> raw input by default (optional controlled denoise experiment)
  -> enrollment ONNX
  -> VoiceProfile integrity validation
  -> SetVoiceProfile IPC
  -> ServiceDaemon transaction
  -> EngineSupervisor or queued DenoiseEngine command
  -> StudioBackend delegation
  -> TractBackend conditioning worker
  -> next audio-frame boundary uses new FiLM/EQ values
```

The source PCM is erased or released after profile generation. Profiles remain excluded from diagnostics and stored with strict permissions.

## 5. Error and Status Semantics

- Malformed or integrity-invalid profile: reject before touching disk or audio state.
- Unsupported backend: preserve the previous profile and return a typed unsupported-feature error.
- Backend application failure: preserve disk and reported state.
- Persistence failure after application: restore the previous backend profile and report inactive transition failure.
- Startup load failure: continue with neutral conditioning and expose a diagnostic error without deleting user data automatically.
- `is_voice_profile_active`: true only when the current backend confirms application.
- `active_voice_profile_id`: ID of the applied profile, not merely the stored file.
- Runtime detection fields never imply profile support or accelerator execution.

## 6. Testing Strategy

Implementation follows red-green-refactor in this order:

1. backend contract and explicit unsupported policies;
2. `StudioBackend` and boxed-backend delegation;
3. supervisor application and invalid-update rollback;
4. queued engine update at a frame boundary;
5. service set/clear/startup transaction semantics;
6. native `filter-capi`/helper propagation;
7. development asset hash/member verification;
8. frontend IPC normalization and truthful status presentation;
9. runtime-guidance capability classification and responsive UI.

Required automated evidence includes:

- identity conditioning remains bit-exact;
- changing profiles preserves GRU state and frame continuity;
- invalid replacement leaves the previous profile active;
- set/clear becomes visible only after a frame boundary;
- persistence rollback restores prior conditioning;
- startup application distinguishes stored from active;
- unsupported backends never report active profiles;
- development assets fail on hash or archive-member mismatch;
- TypeScript focused tests, full tests, Electron state tests, and production build pass;
- Rust formatting, Clippy, focused tests, workspace tests, and offline gate pass where the local toolchain supports them.

Manual QA must exercise enrollment, profile replacement, clearing, restart restoration, and audio continuity through the actual packaged virtual microphone. Frontend visual QA covers desktop and mobile in English and Portuguese.

## 7. Delivery Boundaries

The following claims require distinct evidence:

- **Build repaired:** TypeScript and Rust builds/tests pass.
- **Development integrated:** M3 assets can drive the Rust path under explicit development configuration.
- **Product end-to-end:** packaged native audio paths apply profiles and real audio QA passes.
- **Production approved:** M2 blockers are resolved through M4 and M5 governance/signing is complete.

No implementation task may collapse these labels or advance a later label using evidence from an earlier one.

## 8. Out of Scope

- Changing M2 quality thresholds or approving exceptions.
- Running new training as part of the application integration work.
- Owner signatures or legal approvals generated by an agent.
- Automatic privileged runtime installation.
- Claiming GPU/NPU execution from detection-only backends.
- Broad UI redesign unrelated to truthful status and existing build repairs.

## 9. Acceptance Criteria

1. Stored and applied profile state cannot diverge silently.
2. Profile updates occur outside native callbacks and take effect at frame boundaries without resetting recurrent state.
3. Unsupported backends fail explicitly and preserve the previous working state.
4. The packaged audio path is not described as complete until profile-conditioned audio is verified through the virtual microphone.
5. M3 assets remain development-only until M4/M5 requirements are satisfied.
6. Raw enrollment remains the default until a controlled ablation justifies denoising.
7. Runtime guidance distinguishes detection from verified ClearCore inference and never executes privileged installation commands.
8. All automated and manual verification evidence is recorded without weakening existing fail-closed, privacy, offline, or `unsafe_code` policies.
