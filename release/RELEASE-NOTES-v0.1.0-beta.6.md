# ClearCore v0.1.0-beta.6 — Native Bundled Voice Enrollment & Resilient Model Discovery

**ClearCore (`v0.1.0-beta.6`)** bundles the native vocal biometrics speaker enrollment model (`models/enrollment/enrollment.onnx`, 82 MB) directly into all multi-platform application releases, installers, and packages, eliminating the need for manual configuration or environment overrides.

---

## Release Highlights

- **Native Voice Enrollment Model Packaging**:
  - `models/enrollment/enrollment.onnx` is now officially bundled into all installation packages across Linux (RPM, DEB, AppImage, tar.gz), Windows (Inno Setup, NSIS, portable zip), and macOS (DMG, tar.gz).
  - Voice Profile Creation in the desktop app ("Cadastro de Voz" / "Gerar perfil no serviço") works out-of-the-box on clean installations without requiring external model downloads or dev flags.
- **Robust Multi-Path Model Discovery**:
  - `find_default_enrollment_model()` in the Rust backend service now dynamically checks:
    - Pre-configured `CLEARCORE_ENROLLMENT_MODEL` environment variable.
    - Application executable and parent directory resource trees (`resources/models/enrollment/`).
    - User local application data directories (`$HOME/.local/share/clearcore/models/enrollment/enrollment.onnx` and `XDG_DATA_DIRS`).
    - Standard system installation locations (`/opt/clearcore/models/enrollment/`, `/opt/clearcore/resources/models/enrollment/`, `/usr/share/clearcore/models/enrollment/`).
  - Electron runtime `detectDevModels()` updated to inspect packaged resources and system locations before launching the background daemon.
- **Automated Package Integrity Verification**:
  - CI release workflow asserts the presence of `models/enrollment/enrollment.onnx` in release archives before publication.

---

## Download Artifacts

| Platform / Target | Format | Package |
|---|---|---|
| **Fedora / RHEL (x86_64)** | Native RPM | `Clearcore-0.1.0-beta.6-1.x86_64.rpm` |
| **Fedora / RHEL (ARM64)** | Native RPM | `Clearcore-0.1.0-beta.6-1.aarch64.rpm` |
| **Debian / Ubuntu (amd64)** | Native DEB | `Clearcore-0.1.0-beta.6_amd64.deb` |
| **Debian / Ubuntu (arm64)** | Native DEB | `Clearcore-0.1.0-beta.6_arm64.deb` |
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
sudo dnf install -y https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.6/Clearcore-0.1.0-beta.6-1.x86_64.rpm
```

### Ubuntu / Debian (DEB)
```bash
curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.6/Clearcore-0.1.0-beta.6_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb
```
