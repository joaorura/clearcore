# Specification: Task 14 UI Tauri, Tray & Consented Diagnostics

## Overview
Implement the desktop control UI and system tray using Tauri 2.0, along with the privacy-first `realtime-noise-diagnostics` crate.

## Key Requirements
1. **Zero Audio in UI:** The UI is strictly a control client via IPC (`realtime-noise.v1`). Never process audio or run inference in the UI.
2. **Privacy Diagnostics:** `realtime-noise-diagnostics` exports metrics, logs, device hashes (salted per install), and latency percentiles (p50/p95/p99). It strictly excludes PCM audio samples, speaker embeddings, meeting titles, or raw transcripts.
3. **Tray Integration:** System tray provides quick mode switching (Active, Bypass, Mute), engine status indicator, and exit option.
4. **Sidecar / Service Lifecycle:** UI connects to the independently supervised `realtime-noise-service`. If the UI closes, the service continues running.
5. **Offline Gating:** If offline npm/cargo dependencies are absent, record `BLOCKED_OFFLINE_DEPENDENCY`.
