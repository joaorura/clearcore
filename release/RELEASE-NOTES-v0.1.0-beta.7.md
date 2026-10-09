# ClearCore v0.1.0-beta.7 — Stage 2: pDFNet3 Pro v2 Virtual Mic & Live Voice Profile Integration

**ClearCore (`v0.1.0-beta.7`)** delivers Stage 2: native integration of the **pDFNet3 Pro v2** neural model (Target Speaker Extraction with FiLM conditioning) directly into the PipeWire virtual microphone audio pipeline, applying user voice profiles in real time with zero fallbacks to legacy base models.

---

## Release Highlights

- **Stage 2 Native Virtual Microphone Integration**:
  - The Linux PipeWire virtual microphone bridge (`pipewire_helper` + `libclearcore_filter.so`) directly runs the latest **pDFNet3 Pro v2** acoustic model from `clearcore-train` (`runs/m3_deploy_pro_v2_20261009/`).
  - Native C-ABI extensions (`clearcore_filter_set_voice_profile`, `clearcore_filter_clear_voice_profile`, `clearcore_filter_reload_active_profile`, `clearcore_filter_supports_voice_profile`, `clearcore_filter_is_voice_profile_active`).
- **Strict pDFNet3 Pro Execution (No Legacy Fallback)**:
  - The virtual microphone filter enforces strict loading of the pDFNet3 Pro v2 model asset (`pdfnet3-release-asset-v1.tar.gz`), eliminating fallback to the unconditioned base DeepFilterNet3 model.
  - Neutral identity FiLM tensors ($\gamma = 1.0, \beta = 0.0$) provide pristine general denoising when no profile is active; speaker-isolated FiLM vectors isolate target speech when a profile is active.
- **Dynamic Profile Hot-Reloading**:
  - `pipewire_helper` loads the active profile at startup and monitors profile generation changes via a 250 ms main loop timer and `SIGUSR1` signal handling.
  - Audio callback (`on_capture_process`) remains 100% zero-allocation, lock-free, and real-time safe.
- **Updated Desktop Telemetry & Copy**:
  - UI status indicators and locale strings updated in `pt-BR` and `en-US` to truthfully confirm real-time virtual microphone voice isolation.

---

## Download Artifacts

| Platform / Target | Format | Package |
|---|---|---|
| **Fedora / RHEL (x86_64)** | Native RPM | `Clearcore-0.1.0-beta.7-1.x86_64.rpm` |
| **Fedora / RHEL (ARM64)** | Native RPM | `Clearcore-0.1.0-beta.7-1.aarch64.rpm` |
| **Debian / Ubuntu (amd64)** | Native DEB | `Clearcore-0.1.0-beta.7_amd64.deb` |
| **Debian / Ubuntu (arm64)** | Native DEB | `Clearcore-0.1.0-beta.7_arm64.deb` |
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
sudo dnf install -y https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.7/Clearcore-0.1.0-beta.7-1.x86_64.rpm
```

### Ubuntu / Debian (DEB)
```bash
curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.7/Clearcore-0.1.0-beta.7_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb
```
