# Implementation Plan: Task 14 UI Tauri, Tray & Consented Diagnostics

## Phase 1: Diagnostics Crate & Privacy Contract (RED)
- [x] Task: Create `crates/diagnostics/Cargo.toml` and register in root `Cargo.toml`
- [x] Task: Write failing privacy test `diagnostic_export_excludes_pcm_embeddings_and_meeting_names`
- [x] Task: Verify RED phase in Docker

## Phase 2: Diagnostics Implementation (GREEN)
- [x] Task: Implement `crates/diagnostics/src/lib.rs`, `privacy.rs`, `export.rs`
- [x] Task: Verify diagnostics tests pass (GREEN) in Docker
- [x] Task: Create `docs/diagnostics-schema.md`

## Phase 3: Tauri UI & Frontend Layout
- [x] Task: Create `crates/app-tauri/package.json`, `tsconfig.json`, `vite.config.ts`, `index.html`
- [x] Task: Create frontend components (`src/main.tsx`, `App.tsx`, `diagnostics.tsx`, `styles.css`)
- [x] Task: Create Tauri backend (`src-tauri/Cargo.toml`, `build.rs`, `tauri.conf.json`, `src/main.rs`, `src/commands.rs`, `capabilities/default.json`, `binaries/README.md`)
- [x] Task: Create `scripts/prepare-sidecar.mjs`
- [x] Task: Test offline dependency resolution or record `BLOCKED_OFFLINE_DEPENDENCY` gate

## Phase 4: Track Completion & Evidence
- [x] Task: Verify workspace integrity and complete track
