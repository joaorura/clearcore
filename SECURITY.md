# Security Policy

ClearCore is a realtime noise-suppression virtual microphone engineered for sensitive audio environments. Because our software operates as a privileged virtual audio driver and handles live, uncompressed microphone streams, security, confidentiality, and data privacy are core architectural requirements.

---

## Supported Versions

Security updates and critical vulnerability patches are provided for the following versions:

| Version | Status | Supported Platforms | Notes |
|---|---|---|---|
| **0.1.x (GA)** | Supported | Windows 11 (22H2+), macOS 13+ (Ventura/Sonoma/Sequoia), Ubuntu 24.04 LTS, Fedora 42+ | Active release stream |
| **< 0.1.0** | Unsupported | Pre-release milestones (M0–M6) | Not eligible for backported security patches |

---

## Reporting a Vulnerability

We appreciate the efforts of the security research community in responsibly disclosing potential security vulnerabilities. If you discover a vulnerability in ClearCore, please report it privately.

### Preferred Reporting Method

1. **GitHub Private Vulnerability Reporting**:
   - Navigate to the repository's **Security** tab and click **Report a vulnerability**. This creates a confidential disclosure advisory.
2. **Encrypted Email**:
   - Alternatively, email our security response team directly at:  
     **[security@clearcore.ai](mailto:security@clearcore.ai)**
   - To encrypt your submission, you may request our PGP Public Key or sign with our security team key (Fingerprint: `CC4E 9A72 8B31 F02E 5D17  68C3 1E0B 49D7 721A A90C`).

### What to Include

Please provide:
- A detailed description of the vulnerability and its potential impact.
- Affected component (`engine`, `service`, `filter-capi`, `wavert`, `pipewire_helper`, `hal`, or `app-tauri`).
- Exact steps to reproduce (or a minimal Proof of Concept).
- Target OS, architecture, driver version, and execution environment.
- Any observed memory corruption logs, ASan reports, or crash dumps (ensuring no sensitive personal info is attached).

### Response Time Commitment (SLA)

Our security response team commits to:
- **Initial Acknowledgment**: Within **48 hours** of receiving your report.
- **Triage & Assessment**: Within **5 business days**, confirming reproduction and assigning a severity rating (CVSS v3.1/v4.0).
- **Remediation & Patching**: Critical and high-severity issues are prioritized for emergency patch release within **14 business days**.
- **Coordinated Disclosure**: We follow a standard **90-day coordinated disclosure policy** or coordinate mutually agreed timelines before publicly releasing details and Common Vulnerabilities and Exposures (CVE) advisories.

---

## Security & Privacy Design Guarantees

ClearCore is engineered with defense-in-depth principles to protect user privacy and system integrity:

### 1. Zero Telemetry of Audio Content or Speech

- **Zero PCM Audio Leakage**: ClearCore never captures, records, serializes, or transmits raw audio waveforms, PCM float buffers, or intermediate spectral bins to any disk file, socket, or remote endpoint.
- **Zero Neural Embedding Extraction**: Neural latent states, speaker embeddings, and d-vectors are never retained or logged.
- **Strict Privacy in Diagnostics**: When diagnostic archives (`realtime-noise.diagnostics.v1`) are exported, they are restricted to:
  - Anonymized processing latency percentiles (`p50`, `p95`, `p99` in microseconds).
  - Hop budget breakdowns (DSP vs. system jitter).
  - Cryptographically salted device hashes (`device_id_hash` using per-installation random salts and SHA-256).
  - Sanitized engine generation counters and crash codes.
- **Fail-Closed Envelope Verification**: The diagnostic exporter inspects outgoing payloads and immediately aborts if forbidden keys (such as `pcm_samples` or `speech_transcript`) are detected.

### 2. Session Exclusivity & Driver Access Control

ClearCore interfaces with low-level audio subsystems and implements strict isolation:
- **Windows (WaveRT / PortCls SysVAD Driver)**:
  - Audio stream memory pins and DMA cyclics are protected by kernel-mode access control lists (ACLs).
  - Inter-process communication between user-mode services and kernel drivers uses validated IOCTL handlers with bounded buffer lengths.
- **macOS (CoreAudio HAL AudioServerPlugIn)**:
  - Plug-in operates strictly within Apple's sandboxed Audio Server boundary.
  - Verifies caller PID session permissions and enforces exclusive ownership on capture nodes.
- **Linux (PipeWire / WirePlumber)**:
  - The native C bridge (`pipewire_helper`) creates source and sink nodes constrained to the user's active session bus (`XDG_RUNTIME_DIR/pipewire-0`).
  - WirePlumber node rules restrict audio routing to authorized client applications, preventing unprivileged cross-user eavesdropping.

### 3. Memory Safety & Rigorous Hardening

- **Rust Workspace Memory Safety**: All core modules (`supervisor`, `engine`, `service`, `ipc`, `model`, `contracts`) enforce `#![forbid(unsafe_code)]`. Memory safety violations like use-after-free, buffer overflows, and double frees are eliminated by compiler-verified ownership semantics.
- **Audited C ABI Boundaries**: Where native C FFI is mandatory (`crates/filter-capi` and platform drivers), boundaries are strictly bounded, checked for null/alignment preconditions, and audited with `clippy::undocumented_unsafe_blocks`.
- **Driver Verifier & Sanitizers**:
  - Windows kernel drivers undergo testing with **Microsoft Driver Verifier** to prevent deadlock, memory corruption, and IRQL violations.
  - Linux and macOS native bridges are routinely profiled under **AddressSanitizer (ASan)**, **MemorySanitizer (MSan)**, and **UndefinedBehaviorSanitizer (UBSan)**.
- **Fail-Closed Digital Silence**: If any thread crashes, halts, or encounters an internal inconsistency, the virtual microphone automatically outputs pure digital silence (zeros), guaranteeing that unprocessed background microphone audio is never leaked.

---

## Known Scope & Non-Vulnerabilities

The following are outside the scope of our security vulnerability program:
- Physical attacks requiring direct physical access to an unlocked device without existing OS-level credentials.
- Attacks that require local root / administrator compromise prior to attacking ClearCore.
- Reports from automated scanners that do not demonstrate an exploitable PoC.
- Intentional denial of service caused by terminating ClearCore processes via Task Manager / `kill -9` by the local administrative user.
