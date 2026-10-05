# Voice Enrollment Pipeline — Service (Rust) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. **Waves 1–3 run their tasks in parallel, one git worktree per task; the file-ownership table below is what makes that safe — never edit a file another task owns.**

**Goal:** Turn recorded samples into a real `VoiceProfile` inside the service (denoise → store denoised WAV → trim/join → EQ → enroll) with a 90 s speech budget, applied through the Stage 1 transaction.

**Architecture:** Pure DSP and enrollment orchestration live in new `crates/model` modules; I/O, jobs, budget and IPC handlers live in new `crates/service` modules. One serial contract task (Wave 0) fixes every shared type/constant/module declaration so later tasks only fill files they own. Heavy work runs on worker threads; results are drained and applied on the daemon thread at the start of every request.

**Tech Stack:** Rust 1.90.0 (`realtime-noise-model`, `-service`, `-ipc`), `base64 =0.22.1` (already in the lock), `tract` feature for the real denoiser and ONNX enrollment.

**Spec:** `docs/superpowers/specs/2026-10-05-voice-enrollment-pipeline-design.md` (read it first; this plan implements §4–§8, §11, §12 for the service). The app side is `2026-10-05-voice-enrollment-pipeline-app.md`.

## Global Constraints

- Shell: `export PATH=$HOME/.cargo/bin:$PATH`; `export CARGO_TARGET_DIR=/home/joaorura/orca/workspaces/clearcore/hippocamp/target`; always `--offline --locked`. Rust 1.90.0. Tract tests need `--features tract`.
- Production code: no `unwrap`, `expect`, `panic!`, `todo!`, `unimplemented!` (workspace lint deny). Test modules start with `#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]`. `unsafe_code` is forbidden. Clippy `-D warnings` must stay clean for the files you touch (known pre-existing failures: `crates/service/tests/preset_ipc.rs:202`, `crates/supervisor/tests/backend_selection.rs:71` — leave them).
- No new dependency except `base64 =0.22.1` with `default-features = false, features = ["alloc"]` added to `crates/service/Cargo.toml` **in Task 0 only**. No `hound`, no `rubato`.
- Raw PCM never touches disk and is zeroed after use. Saved sample = denoised WAV, 16-bit, 48 kHz, mono. Files `0600`, directories `0700`, symlinks refused. Nothing logs or echoes audio or profile payloads. Error responses carry fixed codes only (spec §5).
- Budget: 90 s of **speech after trimming** per device group; manual sample over budget → `ENROLL_BUDGET_EXCEEDED`; dynamic take recorded only if `remaining ≥ 5 s` **and** its speech ≤ remaining; no automatic deletion (spec §4.4).
- Enrollment model is **development-only**: loaded only from `CLEARCORE_DEV_ENROLLMENT_ASSET` + `CLEARCORE_DEV_ENROLLMENT_SHA256` (full-file SHA-256, member allowlist); otherwise `ENROLL_MODEL_NOT_CONFIGURED`. Never copy it to `vendor/approved/`, never pin or sign it. Nothing may claim "end-to-end", "production" or that the profile improves isolation (spec §12.8).
- Agent preamble (every task): your worktree may start on an old commit. Run `git log --oneline -3`; unless `d43c345` is in your history run `git merge --ff-only docs-using-superpowers-skill`. Commit only your own files (`git add <paths>`, never `-A`), no `git stash`. Commit trailers:
  `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` and `Claude-Session: https://claude.ai/code/session_013NV6Le7wWJGJVHVStPZrBy`.
- Commit messages are English, conventional (`feat(model): …`).

## Waves and file ownership

| Wave | Task | Owns (only this task edits these files) |
|---|---|---|
| 0 (serial) | **T0 Contract** | `crates/ipc/src/{lib.rs,protocol.rs,enrollment_codes.rs,line_limit.rs}` (stub), `crates/ipc/tests/protocol.rs`, `crates/service/Cargo.toml`, `Cargo.lock`, `crates/service/src/{enrollment_error.rs,enrollment_ingest.rs (trait only)}`, `crates/service/src/voice_samples.rs` (struct fields), `mod` lines in `crates/service/src/lib.rs` and `crates/model/src/lib.rs`, empty module files |
| 1 (parallel) | **M1** trim/gain/join | `crates/model/src/speech_trim.rs` |
| 1 | **M2** decimator | `crates/model/src/resample.rs` |
| 1 | **M3** WAV | `crates/model/src/wav.rs` |
| 1 | **M4** validator cap + dev archive loader | `crates/model/src/enrollment.rs` |
| 1 | **S1** budget | `crates/service/src/voice_budget.rs` |
| 1 | **S2** config | `crates/service/src/enrollment_config.rs` |
| 1 | **S3** storage safety + migration | `crates/service/src/voice_storage_migration.rs`, `crates/service/src/voice_samples.rs` (methods only, after T0) |
| 1 | **S5** job table | `crates/service/src/enrollment_jobs.rs` |
| 1 | **I1** line cap + parse-error masking | `crates/ipc/src/line_limit.rs`, `crates/ipc/src/server.rs` |
| 1 | **D1** diagnostics exclusion | `crates/diagnostics/**` |
| 2 (parallel) | **E1** builder | `crates/model/src/enrollment_builder.rs` |
| 2 | **S4** ingestion + real denoiser | `crates/service/src/enrollment_ingest.rs` |
| 3 (serial) | **S6** handlers + daemon wiring | `crates/service/src/ipc_voice.rs`, `crates/service/src/lib.rs`, `crates/service/src/voice_intake.rs`, `crates/service/tests/enrollment_*.rs` |
| 3 (parallel with S6) | **D2** docs | `docs/ipc-v1.md`, `CHANGELOG.md` |
| 4 | **V1** workspace verification and final review | read-only |

Merge order inside a wave is irrelevant (disjoint files). Between waves: cherry-pick the finished commits onto `docs-using-superpowers-skill`, rebuild once, then start the next wave from that HEAD.

---

## Task 0: Contract (serial, everything else depends on it)

**Files:** see the Wave 0 row above.

**Interfaces produced (copy these verbatim; later tasks rely on them):**

```rust
// crates/ipc/src/enrollment_codes.rs
pub const ENROLL_CLIPPING: &str = "ENROLL_CLIPPING";
pub const ENROLL_TOO_QUIET: &str = "ENROLL_TOO_QUIET";
pub const ENROLL_TOO_LITTLE_SPEECH: &str = "ENROLL_TOO_LITTLE_SPEECH";
pub const ENROLL_MODEL_NOT_CONFIGURED: &str = "ENROLL_MODEL_NOT_CONFIGURED";
pub const ENROLL_BUDGET_EXCEEDED: &str = "ENROLL_BUDGET_EXCEEDED";
pub const ENROLL_INVALID_AUDIO: &str = "ENROLL_INVALID_AUDIO";
pub const ENROLL_PAYLOAD_TOO_LARGE: &str = "ENROLL_PAYLOAD_TOO_LARGE";
pub const ENROLL_JOB_NOT_FOUND: &str = "ENROLL_JOB_NOT_FOUND";
pub const ENROLL_FAILED: &str = "ENROLL_FAILED";
/// 90 s * 48 kHz * 4 B = 17.28 MB; base64 inflates by 4/3 (23.04 MB) plus envelope.
pub const MAX_REQUEST_LINE_BYTES: usize = 32 * 1024 * 1024;
```

```rust
// crates/ipc/src/protocol.rs — IpcCommand changes (serde PascalCase enum, externally tagged)
//   REMOVE:  AddVoiceSample { sample_json: String }
//   ADD:
AddVoiceSample { name: String, pcm_f32_le_b64: String, sample_rate: u32,
                 device_label: String, device_id_hash: String },
BuildVoiceProfile { name: String },
GetEnrollmentJob { job_id: String },
// Debug stays manually redacted: AddVoiceSample { .. } and BuildVoiceProfile { .. } print "<redacted>";
// GetEnrollmentJob prints its job_id. Every other variant unchanged.
```

```rust
// crates/service/src/enrollment_error.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnrollError {
    Clipping, TooQuiet, TooLittleSpeech, ModelNotConfigured, BudgetExceeded,
    InvalidAudio, PayloadTooLarge, JobNotFound, Failed,
}
impl EnrollError {
    #[must_use] pub fn code(&self) -> &'static str { /* map to realtime_noise_ipc::enrollment_codes::* */ }
}
```

```rust
// crates/service/src/enrollment_ingest.rs (trait only in T0; S4 fills the rest)
pub trait Denoiser: Send {
    /// 48 kHz mono in -> 48 kHz mono out, same length, latency already compensated.
    fn denoise(&mut self, pcm48: &[f32]) -> Result<Vec<f32>, EnrollError>;
}
```

```rust
// crates/service/src/voice_samples.rs — VoiceSample gains (all #[serde(default)] so old files load):
pub device_label: String,
pub device_id_hash: String,
pub capture_sample_rate: u32,
pub speech_seconds: f32,
// `embedding: Vec<f32>` gets #[serde(default)]; validate() accepts an EMPTY embedding or exactly 192 finite values.
```

Module declarations added with empty files (each file is only `//! Owned by task <id>; see docs/superpowers/plans/2026-10-05-voice-enrollment-pipeline-service.md`):
- `crates/model/src/lib.rs`: `pub mod speech_trim; pub mod resample; pub mod wav; pub mod enrollment_builder;`
- `crates/service/src/lib.rs`: `mod enrollment_error; mod enrollment_ingest; mod enrollment_config; mod enrollment_jobs; mod voice_budget; mod voice_storage_migration; mod ipc_voice;` (and `pub use enrollment_error::EnrollError;`)
- `crates/ipc/src/lib.rs`: `pub mod enrollment_codes; pub mod line_limit;`

- [ ] **Step 1: Write failing tests**

In `crates/ipc/tests/protocol.rs` (replace the old `AddVoiceSample{sample_json}` cases):

```rust
#[test]
fn enrollment_commands_roundtrip_through_json() {
    let cmds = vec![
        IpcCommand::AddVoiceSample { name: "n".into(), pcm_f32_le_b64: "AAAA".into(), sample_rate: 48_000,
            device_label: "Mic".into(), device_id_hash: "ab12".into() },
        IpcCommand::BuildVoiceProfile { name: "João".into() },
        IpcCommand::GetEnrollmentJob { job_id: "job-1".into() },
    ];
    for c in cmds {
        let json = serde_json::to_string(&c).unwrap();
        let back: IpcCommand = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{c:?}"), format!("{back:?}"));
    }
}

#[test]
fn debug_of_enrollment_commands_never_prints_audio_or_names() {
    let c = IpcCommand::AddVoiceSample { name: "Secret Name".into(), pcm_f32_le_b64: "SECRETAUDIO".into(),
        sample_rate: 48_000, device_label: "Secret Mic".into(), device_id_hash: "h".into() };
    let d = format!("{c:?}");
    assert!(!d.contains("SECRETAUDIO") && !d.contains("Secret Name") && !d.contains("Secret Mic"));
    let b = format!("{:?}", IpcCommand::BuildVoiceProfile { name: "Secret Name".into() });
    assert!(!b.contains("Secret Name"));
}
```

In `crates/service/src/enrollment_error.rs` tests:

```rust
#[test]
fn every_error_maps_to_a_distinct_fixed_code() {
    use EnrollError::*;
    let all = [Clipping, TooQuiet, TooLittleSpeech, ModelNotConfigured, BudgetExceeded,
               InvalidAudio, PayloadTooLarge, JobNotFound, Failed];
    let mut codes: Vec<&str> = all.iter().map(EnrollError::code).collect();
    assert!(codes.iter().all(|c| c.starts_with("ENROLL_")));
    codes.sort_unstable(); codes.dedup();
    assert_eq!(codes.len(), all.len());
}
```

In `crates/service/src/voice_samples.rs` tests:

```rust
#[test]
fn old_sample_json_without_new_fields_still_loads() {
    let json = r#"{"id":"s1","timestamp":"1","name":"n","audio_path":null,"embedding":[],"is_active":true}"#;
    let s: VoiceSample = serde_json::from_str(json).unwrap();
    assert_eq!(s.speech_seconds, 0.0);
    assert!(s.validate().is_ok());
}
```

- [ ] **Step 2: Run, expect failure**

Run: `cargo test --offline --locked -p realtime-noise-ipc -p realtime-noise-service --lib --test protocol 2>&1 | tail -20`
Expected: FAIL (variants and types missing).

- [ ] **Step 3: Implement the contract exactly as listed above** (including the `Debug` redaction arms; the `match` is exhaustive, the compiler will tell you every place that must change). Update the existing tests that used `sample_json` (`voice_samples_and_intake.rs`, `protocol.rs`) to the new shape or to old-JSON-loading behavior. `crates/service/src/lib.rs`: the old `AddVoiceSample{sample_json}` arm becomes `IpcResponse::internal_error(..)` with code `ENROLL_FAILED` and message `"not implemented"` **only until Task S6** (clippy-clean, no `todo!`). Add `base64` to `crates/service/Cargo.toml` and let `Cargo.lock` update (`cargo build --offline`).

- [ ] **Step 4: Verify**

Run: `cargo fmt --all -- --check && cargo test --offline --locked --workspace --features tract --no-run && cargo test --offline --locked -p realtime-noise-ipc -p realtime-noise-service`
Expected: compiles; tests PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ipc crates/service Cargo.lock crates/model/src
git commit -m "feat(ipc,service): fix the voice enrollment contract and declare pipeline modules"
```

---

## Wave 1 (all parallel; start only after Task 0 is on the branch)

### Task M1: speech trim, gain match, crossfade join (`crates/model/src/speech_trim.rs`)

**Interfaces:**
```rust
pub struct TrimmedSpeech { pub samples: Vec<f32>, pub speech_seconds: f32, pub active_fraction: f32 }
/// 20 ms frames (sample_rate/50), threshold max(-50 dBFS, loudest - 30 dB) on frame mean-square,
/// keep each active run with a 40 ms margin on both sides, join runs with a 20 ms linear crossfade.
pub fn trim_speech(samples: &[f32], sample_rate: u32) -> TrimmedSpeech;
/// RMS in dBFS over active frames only (same VAD rule); -120.0 when nothing is active.
pub fn active_rms_dbfs(samples: &[f32], sample_rate: u32) -> f32;
/// Multiply by 10^(gain_db/20) with gain_db clamped to ±max_abs_db.
pub fn apply_gain_limited(samples: &[f32], gain_db: f32, max_abs_db: f32) -> Vec<f32>;
/// Join parts with a linear crossfade of `crossfade_ms`; parts shorter than the fade are copied whole.
pub fn join_crossfade(parts: &[&[f32]], sample_rate: u32, crossfade_ms: u32) -> Vec<f32>;
```

- [ ] **Step 1: Failing tests** (`#[cfg(test)] mod tests` in the file)

```rust
fn tone(len: usize, amp: f32) -> Vec<f32> {
    (0..len).map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 48_000.0).sin()).collect()
}
#[test]
fn silence_around_a_burst_is_removed_but_margins_stay() {
    let mut x = vec![0.0; 48_000];               // 1 s silence
    x.extend(tone(24_000, 0.3));                  // 0.5 s speech-like burst
    x.extend(vec![0.0; 48_000]);
    let t = trim_speech(&x, 48_000);
    let secs = t.samples.len() as f32 / 48_000.0;
    assert!((0.5 + 0.08 - 0.03..=0.5 + 0.08 + 0.03).contains(&secs), "got {secs}");   // burst + 2×40 ms margin
    assert!((t.speech_seconds - secs).abs() < 1e-6);
}
#[test]
fn pure_silence_gives_zero_speech() {
    let t = trim_speech(&vec![0.0; 48_000], 48_000);
    assert!(t.samples.is_empty() && t.speech_seconds == 0.0 && t.active_fraction == 0.0);
}
#[test]
fn two_bursts_are_joined_without_a_jump() {
    let mut x = tone(9_600, 0.3);
    x.extend(vec![0.0; 48_000]);
    x.extend(tone(9_600, 0.3));
    let t = trim_speech(&x, 48_000);
    let max_step = t.samples.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
    assert!(max_step < 0.1, "step {max_step}");
}
#[test]
fn gain_is_limited() {
    let y = apply_gain_limited(&[0.1], 30.0, 12.0);
    assert!((y[0] - 0.1 * 10f32.powf(12.0 / 20.0)).abs() < 1e-5);
}
#[test]
fn crossfade_join_has_expected_length_and_no_clicks() {
    let a = vec![0.5; 4_800]; let b = vec![0.5; 4_800];
    let j = join_crossfade(&[&a, &b], 48_000, 20);       // 20 ms = 960 samples overlap
    assert_eq!(j.len(), 4_800 + 4_800 - 960);
    assert!(j.iter().all(|v| (*v - 0.5).abs() < 1e-5));  // equal signals crossfade to themselves
}
```

- [ ] **Step 2:** `cargo test --offline --locked -p realtime-noise-model --lib speech_trim` → FAIL (not defined).
- [ ] **Step 3: Implement** the four functions. Algorithm for `trim_speech`: `frame = (sample_rate / 50) as usize`; per frame `dbfs = 10*log10(mean(x²)+1e-12)`; `thr = max(-50.0, loudest - 30.0)`; `active[i] = dbfs >= thr`; `active_fraction = active_count / n_frames`; dilate `active` by 2 frames each side (2 × 20 ms = 40 ms); collect maximal runs of dilated frames as `&samples[start..end]`; return `join_crossfade(&runs, sample_rate, 20)`; `speech_seconds = out.len() / sample_rate`. Frames with a remainder shorter than `frame` are ignored.
- [ ] **Step 4:** run the tests again → PASS; `cargo clippy --offline --locked -p realtime-noise-model --features tract --all-targets -- -D warnings`.
- [ ] **Step 5: Commit** `git add crates/model/src/speech_trim.rs && git commit -m "feat(model): add speech trimming, gain matching and crossfade join"`.

### Task M2: 48 → 16 kHz decimator (`crates/model/src/resample.rs`)

**Interface:** `pub fn decimate_48k_to_16k(input: &[f32]) -> Vec<f32>;` — output length `input.len() / 3`, zero phase (group delay compensated), no new dependency.

- [ ] **Step 1: Failing tests**

```rust
fn sine(freq: f32, secs: f32) -> Vec<f32> {
    (0..(48_000.0 * secs) as usize).map(|i| (2.0*std::f32::consts::PI*freq*i as f32/48_000.0).sin()).collect()
}
fn rms(x: &[f32]) -> f32 { (x.iter().map(|v| v*v).sum::<f32>() / x.len() as f32).sqrt() }
#[test] fn length_is_one_third() { assert_eq!(decimate_48k_to_16k(&vec![0.0; 48_000]).len(), 16_000); }
#[test] fn passband_tone_keeps_its_level() {
    let y = decimate_48k_to_16k(&sine(1_000.0, 1.0));
    let db = 20.0 * (rms(&y[400..y.len()-400]) / (1.0/2f32.sqrt())).log10();
    assert!(db.abs() < 0.5, "{db} dB");
}
#[test] fn alias_band_tone_is_rejected() {
    // 10 kHz would alias to 6 kHz at 16 kHz if not filtered.
    let y = decimate_48k_to_16k(&sine(10_000.0, 1.0));
    let db = 20.0 * (rms(&y[400..y.len()-400]) / (1.0/2f32.sqrt())).log10();
    assert!(db < -60.0, "{db} dB");
}
#[test] fn dc_gain_is_unity() {
    let y = decimate_48k_to_16k(&vec![0.25; 48_000]);
    assert!((y[8_000] - 0.25).abs() < 1e-3);
}
```

- [ ] **Step 2:** run → FAIL.
- [ ] **Step 3: Implement.** Windowed-sinc low-pass FIR: `fc = 7_000.0 / 48_000.0` (cycles/sample), `TAPS = 127` (odd), `h[n] = 2*fc*sinc(2*fc*(n-M))*w[n]` with `M = (TAPS-1)/2` and a 4-term Blackman-Harris window; normalise so `sum(h) == 1`. For output index `k`, compute `y[k] = Σ h[j] * x[3k + M - j]` treating out-of-range samples as 0 (this compensates the group delay `M`). Precompute `h` once per call (the function is called per build, not per frame).
- [ ] **Step 4:** tests PASS + clippy. If the 60 dB rejection fails, raise `TAPS` (e.g. 255) — do not relax the test.
- [ ] **Step 5: Commit** `feat(model): add a 48 to 16 kHz polyphase-quality decimator`.

### Task M3: WAV 16-bit mono (`crates/model/src/wav.rs`)

**Interfaces:**
```rust
#[derive(Debug, PartialEq, Eq)]
pub enum WavError { TooShort, NotRiffWave, UnsupportedFormat, BadDataChunk }
pub fn encode_wav_pcm16_mono(samples: &[f32], sample_rate: u32) -> Vec<u8>;     // clamps to [-1,1], 44-byte header
pub fn decode_wav_pcm16_mono(bytes: &[u8]) -> Result<(Vec<f32>, u32), WavError>; // PCM(1), mono, 16-bit only
```

- [ ] **Step 1: Failing tests**

```rust
#[test] fn header_is_44_bytes_and_riff() {
    let w = encode_wav_pcm16_mono(&[0.0; 10], 48_000);
    assert_eq!(&w[0..4], b"RIFF"); assert_eq!(&w[8..12], b"WAVE"); assert_eq!(w.len(), 44 + 20);
}
#[test] fn round_trip_is_within_one_lsb() {
    let x: Vec<f32> = (0..1000).map(|i| (i as f32 / 1000.0 * 2.0 - 1.0) * 0.9).collect();
    let (y, sr) = decode_wav_pcm16_mono(&encode_wav_pcm16_mono(&x, 48_000)).unwrap();
    assert_eq!(sr, 48_000);
    assert!(x.iter().zip(&y).all(|(a, b)| (a - b).abs() <= 1.0 / 32768.0 + 1e-6));
}
#[test] fn out_of_range_samples_are_clamped_not_wrapped() {
    let (y, _) = decode_wav_pcm16_mono(&encode_wav_pcm16_mono(&[2.0, -2.0], 48_000)).unwrap();
    assert!(y[0] > 0.99 && y[1] < -0.99);
}
#[test] fn rejects_garbage_and_non_pcm16() {
    assert_eq!(decode_wav_pcm16_mono(b"nope"), Err(WavError::TooShort));
    let mut w = encode_wav_pcm16_mono(&[0.0; 4], 48_000); w[20] = 3; // format tag 3 = float
    assert_eq!(decode_wav_pcm16_mono(&w), Err(WavError::UnsupportedFormat));
}
```

- [ ] **Step 2:** run → FAIL. **Step 3: Implement** (little-endian fields; `fmt ` chunk size 16, tag 1, channels 1, byte rate `sr*2`, block align 2, bits 16; `data` chunk; scale `(x.clamp(-1,1) * 32767.0).round() as i16`; decode walks chunks, tolerates extra chunks before `data`, rejects odd data length with `BadDataChunk`). **Step 4:** PASS + clippy. **Step 5: Commit** `feat(model): add a dependency-free 16-bit mono WAV codec`.

### Task M4: validator cap and development archive loader (`crates/model/src/enrollment.rs`)

**Changes:**
1. `ENROLLMENT_MAX_DURATION_SECS = 90.0` and `ENROLLMENT_MAX_SAMPLES = 1_440_000`; update doc comments: the model was trained on 6–12 s, durations up to 90 s were measured stable (spec §3), 90 s is an engineering cap enforced by the speech budget.
2. New loader:
```rust
#[cfg(feature = "tract")]
impl OnnxEnrollmentModel {
    /// `expected_sha256_hex`: lowercase hex of the WHOLE archive. Only the allowlisted members are read.
    pub fn from_dev_archive(archive_tar_gz: &[u8], expected_sha256_hex: &str) -> Result<Self, EnrollmentError>;
}
```
It hashes the full bytes with `sha2` (already a dependency), compares, decompresses (`flate2`), walks `tar` entries, rejects any member not in `DEV_ARCHIVE_MEMBER_ALLOWLIST`, extracts `enrollment.onnx`, and calls the existing `from_onnx_bytes`. Failures map to `EnrollmentError::Model(String)` with fixed messages (no hash echo).

- [ ] **Step 1: Failing tests**

```rust
#[test] fn sixty_seconds_are_accepted_and_ninety_one_are_not() {
    let ok = vec![0.1_f32; 16_000 * 60]; /* use the existing helper that makes valid speech-like audio */
    // build EnrollmentAudio{samples,&ok, sample_rate:16000}; assert validate(...) is Ok
    // then 91 s => Err(EnrollmentError::TooLong{..})
}
#[cfg(feature = "tract")] #[test] fn dev_archive_rejects_wrong_hash_and_unlisted_members() {
    // build a tar.gz in memory with `tar` + `flate2` containing enrollment.onnx (fixture bytes) and "evil.bin"
    // assert from_dev_archive(.., correct_hash) fails because of the unlisted member
    // assert from_dev_archive(good_archive, "00"*32) fails on the hash
}
#[cfg(feature = "tract")] #[test] fn dev_archive_loads_the_fixture_model() {
    // archive with ONLY enrollment.onnx (fixture) + its real sha256 => Ok, and enrolling works end to end
}
```
(Reuse `onnx_bytes()` and the valid-audio helper that already exist in this file's tests; fill in the bodies with them — the three behaviors above are the contract.)

- [ ] **Step 2:** `cargo test --offline --locked -p realtime-noise-model --features tract --lib enrollment` → the three new tests FAIL.
- [ ] **Step 3:** First run `tar tzf /home/joaorura/orca/projects/clearcore-train/runs/m3/voice-enrollment-asset-v1.tar.gz` (read-only) and put the **exact** member names in `DEV_ARCHIVE_MEMBER_ALLOWLIST`; implement the loader and the constant changes; fix the existing duration tests (`duration_bounds_are_inclusive`) to the new bounds.
- [ ] **Step 4:** PASS (with and without `--features tract`) + clippy. **Step 5: Commit** `feat(model): raise the enrollment cap to 90 s and add a hash-verified development archive loader`.

### Task S1: speech budget (`crates/service/src/voice_budget.rs`) — pure

**Full implementation:**

```rust
use crate::voice_samples::VoiceSample;

pub const MAX_SPEECH_SECONDS: f32 = 90.0;
pub const MIN_TAKE_MARGIN_SECONDS: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Budget { pub used_seconds: f32, pub max_seconds: f32, pub remaining_seconds: f32 }

/// Device group of the most recent sample (by numeric `timestamp`, ties broken by the later list position).
#[must_use] pub fn selected_device_hash(samples: &[VoiceSample]) -> Option<String> {
    samples.iter().enumerate()
        .max_by_key(|(i, s)| (s.timestamp.parse::<u64>().unwrap_or(0), *i))
        .map(|(_, s)| s.device_id_hash.clone())
}
#[must_use] pub fn budget_for(samples: &[VoiceSample], device_hash: &str) -> Budget {
    let used: f32 = samples.iter()
        .filter(|s| s.is_active && s.device_id_hash == device_hash && s.audio_path.is_some())
        .map(|s| s.speech_seconds).sum();
    Budget { used_seconds: used, max_seconds: MAX_SPEECH_SECONDS, remaining_seconds: (MAX_SPEECH_SECONDS - used).max(0.0) }
}
#[must_use] pub fn fits_manual(b: &Budget, speech_seconds: f32) -> bool { b.used_seconds + speech_seconds <= b.max_seconds + 1e-4 }
#[must_use] pub fn fits_take(b: &Budget, speech_seconds: f32) -> bool {
    b.remaining_seconds >= MIN_TAKE_MARGIN_SECONDS && speech_seconds <= b.remaining_seconds + 1e-4
}
```

- [ ] **Step 1: Failing tests**

```rust
fn s(id: &str, ts: &str, dev: &str, secs: f32, wav: bool) -> VoiceSample { /* VoiceSample::new(..) then set device_id_hash, speech_seconds; audio_path = wav.then(|| "x.wav".into()) */ }
#[test] fn sums_only_active_samples_of_the_group_with_audio() { /* 30 + 20 on dev A, 40 on dev B, 10 without wav on A => used 50 for A */ }
#[test] fn most_recent_sample_selects_the_group() { /* timestamps "10" A, "20" B => B */ }
#[test] fn manual_sample_must_fit_exactly_or_less() { /* used 80: 10 fits, 10.01 does not */ }
#[test] fn take_needs_five_seconds_margin_and_must_fit() {
    /* used 86 => remaining 4 => fits_take false even for 1 s;  used 80 => remaining 10: 6 s fits, 11 s does not */
}
#[test] fn remaining_never_negative() { /* used 95 => remaining 0 */ }
```
- [ ] **Step 2–5:** run FAIL → implement the code above → PASS + clippy → commit `feat(service): add the 90 s speech budget rules`.

### Task S2: enrollment configuration (`crates/service/src/enrollment_config.rs`)

```rust
use std::path::PathBuf;
pub const ENV_ASSET: &str = "CLEARCORE_DEV_ENROLLMENT_ASSET";
pub const ENV_SHA256: &str = "CLEARCORE_DEV_ENROLLMENT_SHA256";
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnrollmentConfig { pub archive_path: Option<PathBuf>, pub expected_sha256: Option<String> }
impl EnrollmentConfig {
    #[must_use] pub fn from_lookup(get: impl Fn(&str) -> Option<String>) -> Self { /* trim, empty => None, lowercase sha */ }
    #[must_use] pub fn from_env() -> Self { Self::from_lookup(|k| std::env::var(k).ok()) }
    /// true only if both are present AND the sha is 64 lowercase hex chars.
    #[must_use] pub fn is_configured(&self) -> bool { /* … */ }
}
```
- [ ] **Step 1: Failing tests:** `from_lookup` with both set → configured; empty/missing/short sha/non-hex → not configured; sha is lowercased; `Default` is unconfigured; no panic on non-UTF8 (lookup returns `None`).
- [ ] **Step 2–5:** run FAIL → implement → PASS + clippy → commit `feat(service): read the development enrollment asset configuration`.

### Task S3: storage safety and migration (`voice_storage_migration.rs` + methods in `voice_samples.rs`)

**Interfaces:**
```rust
// voice_samples.rs (methods only; T0 already added the fields)
impl VoiceSampleManager {
    pub fn samples_dir(&self) -> PathBuf;                                   // <dir>/samples
    pub fn ensure_samples_dir(&self) -> io::Result<()>;                     // 0700, refuses symlinks
    /// Writes <samples_dir>/<id>.wav with mode 0600 atomically; id must match [A-Za-z0-9_-]{1,64}.
    pub fn write_sample_wav(&self, id: &str, wav: &[u8]) -> Result<PathBuf, VoiceSampleError>;
    /// delete_sample(.., true) now removes audio_path ONLY when it resolves inside samples_dir.
}
// voice_storage_migration.rs
pub struct MigrationReport { pub migrated_samples: usize, pub marked_needs_reenroll: usize }
pub fn migrate_legacy_samples(legacy_dir: &Path, target: &mut VoiceSampleManager) -> io::Result<MigrationReport>;
```
Migration copies `voice_samples.json` entries from the legacy dir (`$HOME/.clearcore/profiles`) that are not yet in `target`, keeps them with `audio_path = None` (their only audio was a `blob:` URL), `speech_seconds = 0`; never deletes the legacy files.

- [ ] **Step 1: Failing tests:** (a) `write_sample_wav` creates `samples/` mode 0700 and the file 0600; (b) id with `../` or `/` is rejected; (c) a symlinked `samples/` is refused; (d) `delete_sample(id, true)` does **not** delete a file outside `samples_dir` even if `audio_path` points there; (e) migration copies two legacy samples once (idempotent on a second run) and reports `marked_needs_reenroll == 2`; (f) migration with no legacy dir is a no-op.
- [ ] **Step 2–5:** FAIL → implement (reuse the permission/symlink helpers already in `voice_samples.rs`) → PASS + clippy → commit `feat(service): confine sample audio to a private directory and migrate legacy samples`.

### Task S5: job table (`crates/service/src/enrollment_jobs.rs`)

**Interfaces:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum JobState { Running, Done, Failed }
#[derive(Debug, Clone)]
pub struct JobInfo { pub job_id: String, pub state: JobState, pub stage: &'static str,
                     pub error_code: Option<&'static str>, pub remaining_seconds: Option<f32> }
pub struct JobTable<T: Send + 'static> { /* next_id counter, HashMap<String, JobInfo>, mpsc::Receiver<Completion<T>> , Sender clone */ }
pub struct Completion<T> { pub job_id: String, pub result: Result<T, JobFailure> }
pub struct JobFailure { pub error_code: &'static str, pub stage: &'static str, pub remaining_seconds: Option<f32> }
impl<T: Send + 'static> JobTable<T> {
    pub fn new() -> Self;
    /// Registers a Running job "job-<n>" and runs `work` on a new thread; the result is queued for `drain`.
    pub fn spawn<F>(&mut self, stage: &'static str, work: F) -> String where F: FnOnce() -> Result<T, JobFailure> + Send + 'static;
    /// Moves every finished job to its final state and returns the successful payloads in completion order.
    pub fn drain(&mut self) -> Vec<(String, T)>;
    pub fn mark_failed(&mut self, job_id: &str, failure: JobFailure); // for failures found while applying a drained result
    pub fn mark_done(&mut self, job_id: &str);
    pub fn info(&self, job_id: &str) -> Option<&JobInfo>;
}
```
Keeps at most the last 64 finished jobs.

- [ ] **Step 1: Failing tests**

```rust
#[test] fn spawned_job_completes_and_drain_returns_its_payload() {
    let mut t = JobTable::<u32>::new();
    let id = t.spawn("denoise", || Ok(7));
    let got = wait_until(|| { let d = t.drain(); if d.is_empty() { None } else { Some(d) } });
    assert_eq!(got, vec![(id.clone(), 7)]);
}
#[test] fn failing_job_is_reported_with_its_code_and_not_returned() {
    let mut t = JobTable::<u32>::new();
    let id = t.spawn("enroll", || Err(JobFailure { error_code: "ENROLL_FAILED", stage: "enroll", remaining_seconds: None }));
    wait_until(|| { t.drain(); (t.info(&id).unwrap().state == JobState::Failed).then_some(()) });
    assert_eq!(t.info(&id).unwrap().error_code, Some("ENROLL_FAILED"));
}
#[test] fn job_ids_are_unique_and_unknown_id_is_none() { /* two spawns differ; info("nope") == None */ }
#[test] fn only_the_last_64_finished_jobs_are_kept() { /* spawn 70 instant jobs, drain, info(first) == None, info(last) is Some */ }
```
(`wait_until` is a local helper polling every 5 ms up to 5 s.)
- [ ] **Step 2–5:** FAIL → implement → PASS + clippy → commit `feat(service): add a thread-backed enrollment job table`.

### Task I1: line cap and parse-error masking (`crates/ipc/src/line_limit.rs`, `server.rs`)

**Interface:**
```rust
pub enum LineRead { Eof, Line, TooLong }
/// Reads one '\n'-terminated line into `buf` (cleared first) without ever holding more than `max_bytes`;
/// on overflow it discards the rest of the line (so the connection stays in sync) and returns TooLong.
pub fn read_line_limited<R: std::io::BufRead>(reader: &mut R, buf: &mut String, max_bytes: usize) -> std::io::Result<LineRead>;
```
`IpcServer::handle_stream` uses it with `MAX_REQUEST_LINE_BYTES` and answers `TooLong` with `invalid_command` carrying code `ENROLL_PAYLOAD_TOO_LARGE`. `JSON_PARSE_ERROR` responses carry the fixed message `"malformed request"` instead of `e.to_string()`.

- [ ] **Step 1: Failing tests:** (a) a short line is returned; (b) a line of `max+1` bytes returns `TooLong`, and the **next** line is read correctly; (c) EOF without newline returns the partial line then `Eof`; (d) `handle_stream` with an oversized line answers `ENROLL_PAYLOAD_TOO_LARGE` and keeps serving; (e) a malformed JSON line answers `JSON_PARSE_ERROR` whose message does not contain any of the input text (use input `{"Secret":"TOKEN123"`).
- [ ] **Step 2–5:** FAIL → implement (loop over `fill_buf`/`consume`, cap by bytes, validate UTF-8 at the end with a fixed error) → PASS + `cargo test -p realtime-noise-ipc` + clippy → commit `feat(ipc): cap request lines and stop echoing parse errors`.

### Task D1: diagnostics must exclude audio and profiles (`crates/diagnostics/**`)

- [ ] **Step 1:** read how the diagnostics bundle collects files (grep `profiles`, `samples`, `.wav`, `active_profile`, `voice_samples`).
- [ ] **Step 2: Write a failing test** in `crates/diagnostics/tests/` (new file `voice_exclusion.rs`): create a temp profile dir with `active_profile.json`, `voice_samples.json`, `samples/a.wav`, `intake_suggestions.json`; build a diagnostics bundle pointing at it (use the crate's public API); assert none of those names or their bytes appear in the output. If the crate never reads that directory, the test documents it by asserting the bundle contains no path under the profile dir.
- [ ] **Step 3:** fix only if the test fails (exclusion list in the collector). If it passes immediately, say so in the report (no code change) and keep the test.
- [ ] **Step 4:** `cargo test --offline --locked -p realtime-noise-diagnostics`; **Step 5: Commit** `test(diagnostics): lock the exclusion of voice samples, WAVs and profiles`.

---

## Wave 2 (parallel; start after Wave 1 is cherry-picked and builds)

### Task E1: profile builder (`crates/model/src/enrollment_builder.rs`)

**Interfaces:**
```rust
#[derive(Debug)] pub enum BuildError { Eq(MicrophoneEqError), Enroll(EnrollmentError), TooMuchSpeech }
pub const MAX_JOINED_SPEECH_SECONDS: f32 = 90.0;
/// `joined_48k`: already trimmed, level-matched and joined (48 kHz mono). Steps: reject > 90 s;
/// EQ via estimate_microphone_eq(&SpeechTargetCurve::default(), &MicrophoneEqConfig::default());
/// decimate_48k_to_16k; EnrollmentRecording::new; engine.enroll(&mut rec, metadata, Some(eq)).
pub fn build_profile<M: SpeakerEmbeddingModel>(
    engine: &mut SpeakerEnrollmentEngine<M>, joined_48k: &[f32], metadata: &ProfileMetadata,
) -> Result<VoiceProfile, BuildError>;
```
- [ ] **Step 1: Failing tests:** (a) with a fake `SpeakerEmbeddingModel` returning fixed valid vectors (copy the fake from `enrollment.rs` tests), a 20 s speech-like signal yields a `VoiceProfile` with `eq == Some(_)` and a valid `integrity_hash`; (b) 91 s → `BuildError::TooMuchSpeech`; (c) 3 s → `BuildError::Enroll(EnrollmentError::TooShort{..})`; (d) the model receives exactly `len/3` samples (record the length in the fake); (e) `#[cfg(feature = "tract")]` full path with `enrollment-contract-test.onnx` via `OnnxEnrollmentModel::from_onnx_bytes` (`pub(crate)` is fine here, same crate).
- [ ] **Step 2–5:** FAIL → implement → PASS (both feature sets) + clippy → commit `feat(model): add the enrollment profile builder`.

### Task S4: ingestion and the real denoiser (`crates/service/src/enrollment_ingest.rs`)

**Interfaces (the `Denoiser` trait from T0 stays):**
```rust
pub struct IngestResult { pub wav_bytes: Vec<u8>, pub speech_seconds: f32, pub peak: f32,
                          pub rms_dbfs: f32, pub active_fraction: f32 }
pub const MIN_SPEECH_RMS_DBFS: f32 = -40.0;       // same as ENROLLMENT_MIN_RMS_DBFS
pub const MAX_PEAK: f32 = 0.99;                    // same as ENROLLMENT_MAX_PEAK
/// Steps: reject non-finite input (InvalidAudio) and a RAW peak >= 0.99 (Clipping); denoise; measure with
/// speech_trim::trim_speech; reject active-speech RMS < -40 dBFS (TooQuiet) and zero trimmed speech
/// (TooLittleSpeech); encode the DENOISED, UNTRIMMED take to WAV (the sample file is the full denoised take;
/// trimming is recomputed at build time); ALWAYS zero `pcm48` before returning, on success and on error.
pub fn ingest_sample(denoiser: &mut dyn Denoiser, pcm48: &mut [f32]) -> Result<IngestResult, EnrollError>;
#[cfg(feature = "tract")]
pub struct TractDenoiser { repo_root: PathBuf }
#[cfg(feature = "tract")] impl TractDenoiser { pub fn new(repo_root: PathBuf) -> Self }
// impl Denoiser for TractDenoiser:
//  - ApprovedAssetManifest::verify(&repo_root) then a FRESH TractBackend::new(&manifest, CpuProfile::Avx2Minimum) per call
//    (no reset exists; fresh state per sample is the contract);
//  - pad input with 1440 + (480 - len % 480) % 480 zeros, process 480-sample frames, drop the first 1440 output samples,
//    truncate to the input length.
```
- [ ] **Step 1: Failing tests** (fake `Denoiser`: identity with a configurable delay-compensation check):
```rust
#[test] fn raw_pcm_is_zeroed_even_on_error() { /* clipping input: Err(Clipping) and the slice is all zeros afterwards */ }
#[test] fn clipping_is_judged_on_the_raw_peak() { /* peak 0.995 raw -> Err(Clipping) although the fake denoiser halves it */ }
#[test] fn quiet_take_is_rejected() { /* tone at -60 dBFS -> Err(TooQuiet) */ }
#[test] fn silence_is_too_little_speech() { /* zeros -> Err(TooLittleSpeech) (or TooQuiet; assert it is one of the two documented codes) */ }
#[test] fn good_take_returns_a_decodable_wav_and_speech_seconds() {
    /* 3 s tone bursts at 0.3: Ok; decode_wav_pcm16_mono(&r.wav_bytes) gives 48 kHz and len == input len;
       r.speech_seconds > 0 && r.speech_seconds <= 3.0 */
}
#[test] fn non_finite_input_is_invalid_audio() { /* NaN -> Err(InvalidAudio) */ }
#[cfg(feature = "tract")] #[test] #[ignore = "uses the approved DFNet3 asset; run locally"]
fn tract_denoiser_keeps_length_and_is_finite() { /* 2 s noise -> same length, all finite */ }
```
- [ ] **Step 2–5:** FAIL → implement → PASS (`--features tract` for the ignored one: `-- --ignored`) + clippy → commit `feat(service): ingest samples through the base denoiser and store the denoised WAV`.

---

## Wave 3

### Task S6: handlers and daemon wiring (serial — owns `lib.rs`)

**Files:** `crates/service/src/ipc_voice.rs`, `crates/service/src/lib.rs`, `crates/service/src/voice_intake.rs`, `crates/service/tests/enrollment_*.rs`.

**Behavior (spec §4.3, §4.4, §5):**
1. `ServiceDaemon` gains `sample_jobs: JobTable<IngestOutcome>`, `profile_jobs: JobTable<VoiceProfile>`, `enrollment_config: EnrollmentConfig`, `denoiser_factory: Box<dyn Fn() -> Box<dyn Denoiser> + Send + Sync>` (default: `TractDenoiser` when `repo_root` is known and the `tract` feature is on; tests inject a fake), `model_factory` (default: `OnnxEnrollmentModel::from_dev_archive` from the config; tests inject the synthetic ONNX through a test-only constructor `ServiceDaemon::with_enrollment_hooks(..)`, `pub` but `#[doc(hidden)]`).
2. In `serve_client`, **before** each request: `self.drain_enrollment_jobs()` and read the line with `ipc::line_limit::read_line_limited(.., MAX_REQUEST_LINE_BYTES)`; `TooLong` → `ENROLL_PAYLOAD_TOO_LARGE`.
3. `AddVoiceSample`: decode base64 (`ENROLL_INVALID_AUDIO` on failure; require `sample_rate == 48_000`), reject payload bigger than 90 s, spawn the ingest job (worker owns the PCM, zeroes it). **On drain** (daemon thread): compute `budget_for(.., device_hash_of_new_sample)`; if `!fits_manual` → job failed `ENROLL_BUDGET_EXCEEDED` with `remaining_seconds`, nothing written; else `write_sample_wav`, add the `VoiceSample` (`embedding` empty, `speech_seconds`, device fields, `audio_path`), mark job done with quality + `sample_id`. Immediately after a successful add, the daemon does **not** rebuild the profile (the app calls `BuildVoiceProfile`).
4. `BuildVoiceProfile { name }`: select the device group (`selected_device_hash`), read its active WAVs on the worker thread, `decode_wav_pcm16_mono` each, `trim_speech` each, `active_rms_dbfs` per sample → group median → `apply_gain_limited(.., median - own, 12.0)` → `join_crossfade(.., 48_000, 20)` → if joined speech > 90 s fail `ENROLL_BUDGET_EXCEEDED` → `build_profile` with the model from `EnrollmentConfig` (`ENROLL_MODEL_NOT_CONFIGURED` when missing) → result drained on the daemon thread and applied through the **existing transaction**, refactored into `fn apply_profile(&mut self, profile: &VoiceProfile) -> Result<(), ApplyError>` (the JSON entry point `set_voice_profile` calls it after `from_json`). Failure keeps the previous profile (Stage 1 behavior).
5. `GetEnrollmentJob`: `{ job_id, state, stage, error_code, remaining_seconds, sample_id, quality{peak,rms_dbfs,active_fraction,speech_seconds}, profile_id }` (null where absent); unknown id → `ENROLL_JOB_NOT_FOUND`.
6. `ListVoiceSamples` items add `device_label`, `used_in_profile` (device == selected group and active and has WAV), `speech_seconds`, `needs_reenroll` (no `audio_path`), `other_microphone`; reply adds `budget`. `DeleteVoiceSample` is unchanged but now removes the WAV through the S3-hardened path.
7. `AddIntakeSuggestion { take_json }`: `take_json` may carry `pcm_f32_le_b64`, `sample_rate`, `device_label`, `device_id_hash`; it runs the same ingest job; on drain, `fits_take` decides: not fitting → **no suggestion recorded and no error** (job `Done`, `recorded:false`, `reason:"budget"`); fitting → store the suggestion (WAV in `samples_dir`). `ApproveIntakeSuggestion` rechecks `fits_manual`; on failure returns `ENROLL_BUDGET_EXCEEDED` **without removing the take** (reverse the current remove-then-add order in `voice_intake.rs`).
8. `attach_profile_store` calls `migrate_legacy_samples` once (S3).
9. `GetStatus` stays as in Stage 1 (add a comment that `has_voice_profile` no longer depends on the obsolete embedding).

- [ ] **Step 1: Failing integration tests** (`crates/service/tests/enrollment_flow.rs`; helpers: `common::{TempDir, send}`, a fake denoiser (identity), the synthetic ONNX through `with_enrollment_hooks`, and an injectable `ProfileBackend` that supports profiles, as in `tests/voice_profile.rs`):
```rust
// helper: speech_pcm(secs) -> base64 of a modulated-tone signal at 48 kHz that passes ingestion
#[test] fn add_sample_then_job_done_then_sample_listed_with_quality() { /* AddVoiceSample -> job_id; poll GetEnrollmentJob until done; ListVoiceSamples shows speech_seconds>0, device_label, budget.used_seconds>0; no .wav bytes equal to the raw PCM; WAV mode 0600 */ }
#[test] fn manual_sample_over_budget_is_refused_and_nothing_is_stored() { /* fill 88 s then add 5 s => job failed ENROLL_BUDGET_EXCEEDED with remaining_seconds≈2; sample count unchanged */ }
#[test] fn take_with_four_seconds_of_margin_is_silently_not_recorded() { /* used 86 s => AddIntakeSuggestion => job done, recorded:false; ListIntakeSuggestions empty */ }
#[test] fn take_longer_than_the_margin_is_not_recorded() { /* used 80 s (remaining 10), take 11 s => not recorded */ }
#[test] fn approving_a_take_over_budget_keeps_the_take() { /* approve fails ENROLL_BUDGET_EXCEEDED; the take is still listed */ }
#[test] fn build_profile_applies_it_through_the_transaction() { /* 3 samples same device, BuildVoiceProfile, poll to done, GetStatus: is_voice_profile_active true, active id == stored id; active_profile.json mode 0600 */ }
#[test] fn build_without_the_development_model_fails_closed() { /* no config => job failed ENROLL_MODEL_NOT_CONFIGURED; previous profile untouched */ }
#[test] fn samples_of_other_microphones_are_excluded_from_the_profile() { /* 2 samples dev A (older), 1 sample dev B (newer) with <6 s speech => build fails ENROLL_TOO_LITTLE_SPEECH; list shows other_microphone for A */ }
#[test] fn job_result_is_applied_on_the_next_request_and_the_loop_is_not_blocked() { /* GetStatus returns immediately while the build job runs (fake model sleeps 300 ms) */ }
#[test] fn oversized_request_line_is_rejected_and_the_connection_survives() { /* ENROLL_PAYLOAD_TOO_LARGE then a normal GetStatus works */ }
#[test] fn no_raw_audio_is_written_anywhere() { /* walk the temp dir after ingestion; the only audio file is samples/<id>.wav and it decodes to the denoised signal (fake denoiser attenuates by 0.5 so raw != stored) */ }
```
- [ ] **Step 2:** `cargo test --offline --locked -p realtime-noise-service --test enrollment_flow` → FAIL.
- [ ] **Step 3:** implement in this order (commit after each): (a) `apply_profile` refactor with the Stage 1 tests still green; (b) drain + line cap; (c) `AddVoiceSample` + `GetEnrollmentJob`; (d) `ListVoiceSamples`/delete; (e) `BuildVoiceProfile`; (f) takes + approve order fix; (g) migration hook.
- [ ] **Step 4:** `cargo test --offline --locked -p realtime-noise-service` (all files), `cargo fmt --all -- --check`, `cargo clippy --offline --locked -p realtime-noise-service --lib --test enrollment_flow -- -D warnings`.
- [ ] **Step 5: Commit** per sub-step, e.g. `feat(service): ingest voice samples through background jobs`, … , `feat(service): enforce the speech budget for takes and approvals`.

### Task D2: docs (parallel with S6; owns only `docs/ipc-v1.md`, `CHANGELOG.md`)
- [ ] Document `AddVoiceSample`, `BuildVoiceProfile`, `GetEnrollmentJob`, the changed `ListVoiceSamples`/`AddIntakeSuggestion`/`ApproveIntakeSuggestion`, every `ENROLL_*` code, the 32 MiB line cap, the job lifecycle and "applied on the next request", and the dev-model configuration variables. CHANGELOG `[Unreleased]`: enrollment pipeline (development-integrated, not end-to-end, no quality claim). Commit `docs: document the voice enrollment IPC and budget`.

---

## Wave 4: Task V1 — verification and final review
- [ ] Read-only agent runs: `cargo fmt --all -- --check`; `cargo clippy --offline --locked --workspace --features tract --all-targets --keep-going -- -D warnings` (only the two known pre-existing failures allowed); `cargo test --offline --locked --workspace --features tract`; `./scripts/check-offline.sh`; and, locally, `cargo test --offline --locked -p realtime-noise-service --features tract -- --ignored` with `CLEARCORE_DEV_ENROLLMENT_ASSET=/home/joaorura/orca/projects/clearcore-train/runs/m3/voice-enrollment-asset-v1.tar.gz` and `CLEARCORE_DEV_ENROLLMENT_SHA256=bd4d6dd941f8527b5011a2bae78169148f33155e25d30c5707e88963e7ea824d` for the real-model path.
- [ ] Whole-branch review (Opus) against spec §12 criteria 1–10.

## Self-Review

- **Spec coverage:** §4.1 → S4; §4.2 steps 0–7 → M1, M2, E1, S6(4); §4.3 → S5, S6(2,3,4); §4.4 → S1, S6(3,7); §5 → T0, I1, S6, D2; §6.2 → M4; §6.3 → S3, S6(8); §7 → M4, S2; §8 → S3, S4, D1, S6 tests; §11 → test lists above; §12 criteria 1–10 → S6 tests + V1. App-side items (§6.1, §9, UI) are in the companion plan.
- **Known decisions made here:** clipping is judged on the **raw** peak and level on the **denoised** active speech (the spec lists the measures after denoising; clipping is only observable before); the stored WAV is the **full denoised take**, trimming is recomputed at build time so a later change of trimming rules needs no re-recording; the 12 s validator maximum becomes the 90 s cap instead of being deleted, which keeps `TooLong` and the public constants.
- **Risks to watch while executing:** the `film_supported` value of the base DFNet3 asset was not verified (a fresh `TractBackend` is the neutral state either way); `TractBackend::new` re-parses three ONNX files per sample (measure before tuning); real-time cost of the base denoiser per second of audio has no number yet (V1 should record one from the ignored test).
