# ClearCore v0.1.0-beta.8 — ONNX Runtime (`ort` v2) Engine & Stage 2 C-ABI Virtual Mic

**ClearCore (`v0.1.0-beta.8`)** introduces the primary **ONNX Runtime (`ort` v2)** streaming neural engine (`OrtBackend`), migrating the audio pipeline to explicit-state graph execution with zero heap allocations, and integrates it directly into the Stage 2 virtual microphone C-ABI.

---

## Release Highlights

- **ONNX Runtime (`ort` v2) Engine Integration**:
  - Implemented `OrtBackend` in `crates/model` leveraging ONNX Runtime 1.28 (`ort = "2.0.0-rc.13"`) with explicit recurrent state feedback (`h_in`/`h_out`, `feat_erb_buf`, `feat_spec_buf`, `c0_buf`) pinned at $S = 1$ (10 ms hop = 480 samples @ 48 kHz).
  - Pre-allocated tensor buffers and IO Binding ensure **100% zero heap allocations (`0 bytes malloc`)** inside the realtime audio callback.
  - Fail-closed security with SHA-256 digest validation of stateful neural graphs (`APPROVED_STATEFUL_DIGESTS` and `APPROVED_PRO_STATEFUL_DIGESTS`).

- **Stage 2 Virtual Microphone FFI Integration (`filter-capi`)**:
  - Upgraded the PipeWire / native C-ABI bridge (`libclearcore_filter.so`) to `StudioBackend<OrtBackend>`.
  - Added `PdfNet3DevArchive::instantiate_ort()` for direct in-memory loading of verified model archives with continuous streaming inference.
  - Full support for live voice profile conditioning using rank-3 FiLM tensors ($[1, 1, 256]$) with pristine neutral fallback.

- **SOTA Target Speaker Extraction (TSE) Research & Roadmap**:
  - Comprehensive research report covering streaming CPU TSE architectures (`reports/Arquitetura SOTA TSE Streaming CPU.md`).
  - Architectural specification for **pBSRNN-Lite / SCI-LFC** (33 psychoacoustic sub-bands with Staged Condition Injection) registered for Stage 3 maximum separation evaluations (`docs/superpowers/specs/2026-10-10-pbsrnn-lite-sci-lfc-architecture-evaluation.md`).

- **Comprehensive Verification**:
  - 12/12 C-ABI filter tests passing.
  - 3/3 PipeWire helper tests passing.
  - 3/3 ORT stateful smoke tests passing.
  - 112/112 IPC service tests passing.
  - 321/321 desktop application tests passing.

---

## Download Artifacts

| Platform / Target | Format | Package |
|---|---|---|
| **Fedora / RHEL (x86_64)** | Native RPM | `Clearcore-0.1.0-beta.8-1.x86_64.rpm` |
| **Fedora / RHEL (ARM64)** | Native RPM | `Clearcore-0.1.0-beta.8-1.aarch64.rpm` |
| **Debian / Ubuntu (amd64)** | Native DEB | `Clearcore-0.1.0-beta.8_amd64.deb` |
| **Debian / Ubuntu (arm64)** | Native DEB | `Clearcore-0.1.0-beta.8_arm64.deb` |
| **Linux (Universal x64)** | AppImage | `Clearcore-linux-x64.AppImage` |
| **Linux (Universal x64)** | Standalone Archive | `Clearcore-linux-x64.tar.gz` |
| **Windows (x64)** | Single-click Installer | `Clearcore-Setup.exe` |
| **Windows (x64)** | Portable Zip | `Clearcore-win32-x64.zip` |
| **Windows (ARM64)** | Copilot+ Portable | `Clearcore-win32-arm64.zip` |
| **macOS (Apple Silicon)** | DMG Installer | `Clearcore-darwin-arm64.dmg` |
| **macOS (Intel x64)** | DMG Installer | `Clearcore-darwin-x64.dmg` |
| **Developer Neural Models** | Standalone Archive | `Clearcore-models-stateful-v2.tar.gz` |

---

## Installation & Upgrade

### Fedora / RHEL (RPM)
```bash
sudo dnf install -y https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.8/Clearcore-0.1.0-beta.8-1.x86_64.rpm
```

### Ubuntu / Debian (DEB)
```bash
curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.8/Clearcore-0.1.0-beta.8_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb
```
