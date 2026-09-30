# Product Guidelines: Hippocamp

## 1. Brand Identity & Voice
- **Technical & Trustworthy:** Hippocamp is a high-performance system utility. Communication should be precise, factual, and free of hype.
- **Privacy-Forward:** Emphasize 100% offline, on-device processing. Never imply cloud dependencies or hidden telemetry.
- **Developer & Enthusiast Friendly:** Detailed diagnostic reporting, clear metric explanations (PESQ, SNR, latencies in milliseconds), and transparent system status.

## 2. UX Principles & Invariants
- **Non-Interfering Audio Continuity:** The desktop GUI (Tauri) is strictly decoupled from the audio daemon. Closing the window or minimizing to tray must never disrupt active communications.
- **Fail-Closed Silence:** If an engine error or underrun occurs, the user must experience clean digital silence, never audio clicks, pops, or raw unsuppressed audio leak.
- **Instant System Observability:**
  - Status indicator (Active, Bypassed, Muted, Recovering).
  - Real-time p50/p95/p99 latency breakdown (capture, model inference, buffer, output).
  - CPU/Hardware accelerator utilization gauges.
- **Zero Raw-Audio Persistence:** Logs, diagnostic traces, and crash dumps must strictly omit audio sample data. Only metrics, metadata, and timestamps are recorded.

## 3. Visual & Interface Design
- **Platform Native Feel:** Responsive, high-contrast dark and light modes matching host OS themes (Windows Mica/Fluent, macOS Liquid Glass/Vibrant, Linux desktop environments).
- **System Tray First:** Quick toggles for Denoise Level (Off / Balanced / Aggressive), Mute, and Device Selection from the system tray menu.
