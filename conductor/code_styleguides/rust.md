# Rust Code Style Guide

## 1. General Principles
- **Rust Edition:** 2024 (compiler toolchain pinned to `1.90.0`).
- **Idiomatic Rust:** Follow standard Rust API guidelines (`C-CONV`, `C-GETTER`, `C-ITER`, etc.).
- **Formatting:** Code MUST be formatted with `cargo fmt --all`.

## 2. Safety & Compiler Lints
- **No Unsafe Code:** `#![forbid(unsafe_code)]` is strictly enforced across workspace crates.
- **Fail-Closed Error Handling:**
  - `unwrap()`, `expect()`, and `panic!()` are forbidden in production code (enforced by Clippy).
  - Use structured errors with `Result<T, E>` and `thiserror` for library error definitions.
- **Strict Clippy Compliance:**
  - `clippy::all` is denied.
  - Address all warnings; builds run with `-D warnings`.
  - Unused must-use values are strictly denied (`#[warn(unused_must_use)]` / `deny`).

## 3. Real-Time Audio & Memory Constraints
- **Zero Allocations on Audio Path:**
  - Real-time callbacks and frame processing loops MUST NOT allocate (`Vec::push`, `Box::new`, `format!`, etc.).
  - Use fixed-size buffers (`[f32; HOP_SAMPLES]`) and pre-allocated ring buffers.
- **Deterministic Latency:**
  - No blocking operations (mutex contention, file I/O, network requests, synchronous logging) within audio processing threads.
  - Non-blocking lock-free channels or atomic indices for thread-safe state synchronization.

## 4. Testing & Verification
- **Test-Driven Development (TDD):** Every feature, contract, and bug fix begins with a failing automated test (RED) before implementation (GREEN).
- **Offline Determinism:** Tests must run without network access (`--locked --offline`).
- **Golden Testing:** Numerical tolerance bounds must be explicitly specified and verified against frozen reference fixtures.
