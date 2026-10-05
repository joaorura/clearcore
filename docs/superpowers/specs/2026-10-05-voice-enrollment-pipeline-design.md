# ClearCore Voice Enrollment Pipeline Design

**Date:** 2026-10-05  
**Status:** Draft for owner review; implementation pending  
**Builds on:** `2026-10-05-voice-profile-activation-and-runtime-guidance-design.md` (Stage 1, merged in `c893ea7`)  
**Repositories:** `clearcore` (this spec) and `clearcore-train` (dev model asset, read-only here)

## 1. Goal

Make the voice profile real: turn the user's recorded samples into a `VoiceProfile` (FiLM vectors + microphone EQ) inside the service, then apply it through the Stage 1 transaction. Today nothing produces a real profile: the app stores a webm blob URL, sends a constant fake embedding (`0.01 × 192`) and sends its local status JSON as `profile_json`, which the service rejects with `INVALID_COMMAND`.

This spec is **development-integrated**, not product end-to-end and not production-approved (see §10). The enrollment model is the M3 development asset; the quality gate M2 is `NO-GO`.

## 2. Owner decisions (2026-10-05)

| # | Decision | Consequence |
|---|---|---|
| D1 | Capture the **raw physical microphone**; never fall back to the virtual microphone. | Remove the unconstrained `getUserMedia` fallback that may land on the system default (possibly the ClearCore virtual mic). |
| D2 | The **service denoises** the capture with the approved **base DFNet3**, **neutral profile** (no embedding), before anything else. | Reverses the default of the Stage 1 spec §3.6 (raw by default). Explicit owner decision, not an accident. The M1 experiment pointed the other way and remains a known risk. |
| D3 | The audio **saved on the machine is the denoised audio**; the raw audio is never persisted. | Replaces "source PCM is erased" by "raw PCM is erased, denoised copy is the sample file". |
| D4 | Samples are **concatenated with silence removed**; no quality-driven upper duration limit. | The validator's 12 s maximum is relaxed (§6.2). |
| D5 | The **microphone EQ is calibrated on the denoised audio** (48 kHz). | Matches the runtime chain, where the EQ sits after the denoiser. |
| D6 | Scope: development end-to-end with the M3 `enrollment.onnx` behind explicit development configuration, plus the same path in CI with the synthetic test ONNX. | Honest labelling in the UI (§9). |
| D7 | Only samples from the **same capture source** (same microphone) are concatenated. | Each sample records its capture device; the builder uses one device group (§4.2); the microphone EQ of D5 is a property of that microphone. |

## 3. Evidence on duration (experiment of 2026-10-05)

Inference only, `runs/m3/enrollment.onnx`, onnxruntime CPU, 10 speakers of CML-TTS pt, 100 s each. Reference = first 12 s; vector = the four 256-value FiLM outputs. Relative L2 error vs the 12 s reference:

| Window | 2 s | 6 s | 12 s | 20 s | 30 s | 60 s | 90 s |
|---|---|---|---|---|---|---|---|
| mean rel. L2 | 0.088 | 0.037 | 0 | 0.028 | 0.039 | 0.049 | 0.057 |

Same speaker, another 12 s window ("floor"): 0.077. Different speakers: 0.143. Median CPU latency: 190 ms (12 s) to 1.6 s (90 s), roughly linear. All outputs finite, no collapse, no norm growth with duration. With pink noise at 10 dB SNR the vector moved by about 0.06 and showed no visible trend from 12 s to 60 s.

**What this supports:** up to 90 s the vector stays inside the same-speaker window-to-window variation. **What it does not show:** (a) only 10 speakers of read speech from one source; (b) prefixes overlap, so the dip at 12 s is partly a bias, and the Common Voice run concatenated clips of one `client_id` that may come from different sessions and microphones, which is exactly what D7 forbids in the product; (c) it measures vector stability, **not** denoiser quality (SI-SDR/TSOS with the conditioned pDFNet3); (d) the same-speaker floor is already 54% of the between-speaker distance, so the vector separates speakers weakly. Material: `docs/superpowers/research/2026-10-05-enrollment-duration/`.

Hence the engineering cap in §6.2 is **90 s of speech, the measured range**, not a quality limit.

### 3.1 Quality pilot with the conditioned pDFNet3 (2026-10-05)

Inference on CPU with the M2 checkpoint `t3/ckpt-00022500` (formal `NO-GO`), 24 speakers (8 each from LibriSpeech dev-clean, VCTK and CML-TTS pt), 24 synthetic mixtures per speaker: target plus a speech interferer (SIR 0/5 dB), interferer plus noise (SNR 10 dB), or noise only (SNR 0/5/10 dB). The **enrollment audio was clean speech of the target only**, from a pool disjoint from the mixtures, silence removed, prefixes of d = 6, 12, 30, 60, 90 s (3 draws each). Paired differences with bootstrap 95% CIs over speakers.

| Condition | SI-SDRi (dB) | Δ vs 12 s [95% CI] |
|---|---|---|
| 6 s | 1.99 | −0.13 [−0.54, +0.17] |
| 12 s | 2.12 | reference |
| 30 s | 2.18 | +0.07 [−0.08, +0.24] |
| 60 s | 2.14 | +0.02 [−0.24, +0.29] |
| 90 s | 2.07 | −0.05 [−0.32, +0.23] |
| no profile (neutral) | 2.48 | — |
| other speaker's profile | −2.57 | — |

Findings: (a) durations from 6 to 90 s are **indistinguishable** (every CI contains 0; the pilot resolves effects of about 0.3 dB and above); (b) the correct 12 s profile is **not better than no profile** with this checkpoint (−0.36 dB [−1.66, +0.69] SI-SDRi) and is worse on noise-only mixtures (−2.04 dB [−3.72, −0.68]); (c) a wrong-speaker profile is clearly harmful (about 4.7 dB below the correct one), so the vector does steer the model; (d) the checkpoint does not separate an interfering speaker (SI-SDRi about −0.1 dB there) and its STOI is below the input's in about 45% of mixtures. Limits: 24 speakers, synthetic mixtures, one weak checkpoint, CPU, per-corpus effects diverge, and raw versus denoised enrollment audio (D2) was **not** compared. Material: `docs/superpowers/research/2026-10-05-enrollment-quality/`.

**Consequence:** this spec delivers the profile *pipeline*; it makes **no claim** that the profile improves isolation quality. That question belongs to M4 with a model that passes M2.

## 4. Architecture

Two service-side units, separated so each can be tested alone.

### 4.1 Sample ingestion
`PCM 48 kHz mono f32 (memory only)` → base DFNet3 denoise (neutral conditioning, fresh state per sample, latency compensated: the first `algorithmic_latency_samples` output samples are dropped and the tail is flushed with zeros) → quality measures (peak, RMS dBFS, active-speech fraction) → reject clipping (`peak ≥ 0.99`) or too quiet (`RMS < −40 dBFS`) → write the denoised **WAV, 16-bit, 48 kHz, mono** (§8) → record the `VoiceSample` (no embedding) together with its **capture device** (display label and a stable hash of the device identifier, both sent by the app, plus the capture sample rate) → zero the raw PCM → reply with the quality numbers. The device identity lives in the sample records (`voice_samples.json`), not in the `VoiceProfile`, so the profile format, its version and its integrity hash do not change.

### 4.2 Profile builder
Triggered by `BuildVoiceProfile`. Reads the active samples' WAVs in chronological order, then:

0. **Select one device group (D7):** among the active samples, take the capture device of the **most recent** sample and use only the samples of that device. Samples of other devices stay in the gallery, are flagged `other_microphone` (not used) and are listed to the user; the build fails with `ENROLL_TOO_LITTLE_SPEECH` and a "record more with this microphone" message if the group has under 6 s of speech.
1. **Trim and join:** frame energy VAD (20 ms frames, threshold `max(−50 dBFS, loudest − 30 dB)`, the same rule as `validate_enrollment_audio`); keep each active segment with a 40 ms margin; **match the active-speech RMS of every sample to the group median (gain limited to ±12 dB)** so there is no level step at the joins; join segments with a 20 ms linear crossfade.
2. **Cap:** if the speech exceeds 90 s, keep the samples with the highest active-speech fraction (then lowest peak) until 90 s; otherwise keep all, chronologically.
3. **EQ:** `estimate_microphone_eq` on the joined 48 kHz audio (D5).
4. **Resample 48 → 16 kHz** (integer factor 3): reuse an existing resampler if the codebase has one (first task of the plan checks); otherwise a polyphase low-pass FIR in `crates/model`, no new dependency, with a frequency-response test.
5. **Validate:** at least 6 s of speech after trimming, finite samples, no clipping (§6.2).
6. **Enroll:** `SpeakerEnrollmentEngine::enroll` with the development `enrollment.onnx` (§7) → `VoiceProfile` with the EQ embedded; the recording is wiped by `enroll`.
7. **Apply:** hand the profile to the Stage 1 transaction (backend apply → atomic 0600 persist → rollback on failure).

### 4.3 Jobs, not blocking calls
`serve_client` is a single-threaded accept loop. Denoise plus enrollment takes seconds, so ingestion of one sample and the profile build run on a worker thread; the IPC reply is immediate with a job id. Results return through a channel that the daemon thread drains at the start of **every** request (so the UI's periodic `GetStatus` completes the job): the profile is applied on the daemon thread, using the existing transaction. If no client talks to the service, a finished job waits until the next request.

## 5. IPC contract

New or changed commands (`crates/ipc/src/protocol.rs`), all payload sizes capped:

- `AddVoiceSample { name, pcm_f32_le_b64, sample_rate, device_label, device_id_hash }` — **changed**: carries audio (48 kHz mono) and the capture device instead of an embedding. Reply: `{ job_id }`. Final status carries the sample id and quality numbers.
- `BuildVoiceProfile { name }` — **new**. Reply: `{ job_id }`.
- `GetEnrollmentJob { job_id }` — **new**: `state` (`running` | `done` | `failed`), `stage` (`denoise` | `trim` | `eq` | `enroll` | `apply`), fixed error code on failure.
- `ListVoiceSamples` — `audio_path` now points to the saved WAV; each item adds `device_label` and `used_in_profile`; samples without a WAV are flagged `needs_reenroll`, samples of a non-selected device `other_microphone`.
- `DeleteVoiceSample` — unchanged; also removes the WAV. A rebuild is triggered by the client.
- `GetVoiceProfileEmbedding`, `profile.bin` — **deprecated** (they exposed the fake embedding). Kept returning an error until removed.

Error codes are fixed strings (no free text, no payload echo), e.g. `ENROLL_CLIPPING`, `ENROLL_TOO_QUIET`, `ENROLL_TOO_LITTLE_SPEECH`, `ENROLL_MODEL_NOT_CONFIGURED`, `ENROLL_FAILED`, `ENROLL_PAYLOAD_TOO_LARGE`.

## 6. Changes to existing code

### 6.1 App (`crates/app-tauri`)
- Capture: raw physical device only, 48 kHz mono; drop the fallback to an unconstrained `getUserMedia`; if the physical device cannot be opened, show an error. Send PCM, not a `blob:` URL, together with the device label and a stable hash of the device id. Show which microphone each sample used and which samples are not used in the profile.
- `handleActivateProfile`: stop writing `neural_eq_calibrated: true` and `gain_boost_db: 1.8`; those fields come from the service result. Stop sending the local status as `profile_json`.
- Show per-sample quality numbers and the build/apply state (§9).

### 6.2 Validator (`crates/model/src/enrollment.rs`)
`validate_enrollment_audio` keeps the 16 kHz, finiteness, clipping, level and speech-fraction rules. The duration rule becomes: **≥ 6 s of speech after trimming**; the 12 s maximum (`ENROLLMENT_MAX_*`) is removed from validation and replaced by the 90 s engineering cap applied by the builder. The doc comment states that the model was trained on 6–12 s and the stability evidence of §3.

### 6.3 Storage
One directory for everything: the `ProfileStore` dir (`XDG_DATA_HOME/clearcore/profiles`). `VoiceSampleManager` moves from `$HOME/.clearcore/profiles` to it (one-time migration of `voice_samples.json`; samples with no WAV are marked `needs_reenroll`). WAVs live in `samples/<sample-id>.wav`.

## 7. Development enrollment asset

Loaded only with explicit configuration: the path to `voice-enrollment-asset-v1.tar.gz` **and** the expected SHA-256 (`bd4d6dd941f8527b5011a2bae78169148f33155e25d30c5707e88963e7ea824d`), full-file hash checked, archive members restricted to an allowlist (`enrollment.onnx`). Without configuration the service answers `ENROLL_MODEL_NOT_CONFIGURED`. The asset is not copied to `vendor/approved/`, not pinned in the registry, not signed and not distributed. The denoise step uses the approved, pinned base DFNet3.

## 8. Privacy

Raw PCM exists only in memory and is zeroed after denoising. Denoised WAVs and profiles are created with mode `0600` in directories `0700`, are excluded from diagnostics and logs, and are deleted with their sample or on profile clear. IPC audio payloads are size-capped. The existing 0600/0700 checks and symlink refusal of `ProfileStore` apply to the WAV directory.

## 9. Status and UI honesty

The Stage 1 semantics stand: "applied" means applied in the service backend; the packaged virtual microphone does not use the profile yet. While the enrollment model is the development asset, the card says so ("development model, not approved"). Build progress shows the job stage. Quality errors show the measured values.

## 10. Out of scope

- Stage 2: the native packaged path (`filter-capi`, Linux helper).
- Production approval: M4 controlled evaluation and M5 governance/signing; pinning or signing the M3 assets.
- A controlled raw-vs-denoised ablation and denoiser-quality measurement (SI-SDR/TSOS) with conditioned vectors.
- Measuring durations above 90 s.
- UX redesign beyond what §6.1 and §9 need; call-take capture (`AddIntakeSuggestion`) stays as is.

## 11. Testing

- **Unit:** trim-and-join (durations, margins, crossfade continuity, no NaN, gap removal), resampler frequency response, cap selection order, WAV round trip, size caps, error-code mapping.
- **Integration (CI):** full path with the synthetic `enrollment-contract-test.onnx` and a fake denoiser: ingest → build → profile applied → status shows `applied`.
- **Integration (local, `#[ignore]`):** same path with the M3 asset and the real base DFNet3.
- **Privacy:** no raw audio on disk after ingestion; WAV permissions; deletion removes WAV; payload cap.
- **IPC and job tests:** job states, drain-on-request applies the profile, failure leaves the previous profile.
- **Front end:** capture refuses the virtual device and the fallback; selftests for the PCM message and quality display.

## 12. Acceptance criteria

1. A user can record samples, build a profile and see it **applied in the service** with quality numbers, using only the development asset behind explicit configuration.
2. Raw audio is never written to disk; the saved sample is the denoised WAV.
3. Without the development configuration the feature fails closed with `ENROLL_MODEL_NOT_CONFIGURED`.
4. The profile is built from concatenated, silence-trimmed samples; ≥ 6 s of speech is required and 90 s is the cap.
5. The EQ is calibrated on the denoised audio.
6. The IPC loop is never blocked by denoise or enrollment.
7. A failed build leaves the previous profile and the stored state unchanged.
8. Nothing in the code or UI calls the result product end-to-end or production-approved, and nothing claims that the profile improves isolation quality (§3.1).
9. A profile is built only from samples of one capture device; samples of other devices are visibly excluded, and the level step between joined samples is removed.
