# ClearCore v0.1.0-beta.4 — pDFNet3 Pro Integration, Voice Isolation & Dynamic AGC Leveler

**ClearCore (`v0.1.0-beta.4`)** introduces the new neural model **pDFNet3 Pro** (featuring Dual-Conditioning, Causal Cross-Attention, and Asymmetric Formant Loss), active voice profile isolation toggle, and dynamic speech modulation AGC leveler.

---

## Release Highlights

- **Neural Architecture (pDFNet3 Pro v1.5.0)**:
  - Dual-Conditioning combining macro FiLM scaling with causal ERB sub-band cross-attention (300 Hz – 4 kHz).
  - Asymmetric formant loss prioritising speech intelligibility (+0.0749 Δ STOI C0) and high noise attenuation (+7.15 dB Δ SI-SDR).
  - Re-trained on real environmental noise and conversational leakage datasets.
- **Voice Profile & Vocal Isolation**:
  - Voice Profile card now features an interactive **Voice Isolation Toggle** allowing users to switch personalized speech filtering on and off in real time without deleting their stored biometric profile.
  - Multi-sample enrollment gallery and acoustic calibration.
- **Studio DSP & Dynamic Voice Leveler (AGC)**:
  - Voice Auto-Leveler with target loudness compensation and smoothness limiter.
  - Dedicated sliders for suppression intensity and average volume modulation.
- **Multi-Platform Native Distribution**:
  - Full support for Fedora/RHEL (`.rpm`), Debian/Ubuntu (`.deb`), Linux AppImage, Windows (`.exe` NSIS installer + portable `.zip`), Windows ARM64 (Copilot+ / Snapdragon X Elite), and macOS (`.dmg` + `.app`).

---

## Download Artifacts

| Platform | Format | Package |
|---|---|---|
| **Fedora / RHEL (x86_64)** | Native RPM | `Clearcore-0.1.0-beta.4-1.x86_64.rpm` |
| **Fedora / RHEL (ARM64)** | Native RPM | `Clearcore-0.1.0-beta.4-1.aarch64.rpm` |
| **Debian / Ubuntu (amd64)** | Native DEB | `Clearcore-0.1.0-beta.4_amd64.deb` |
| **Debian / Ubuntu (arm64)** | Native DEB | `Clearcore-0.1.0-beta.4_arm64.deb` |
| **Linux (Universal x64)** | AppImage | `Clearcore-linux-x64.AppImage` |
| **Linux (Universal x64)** | Standalone Archive | `Clearcore-linux-x64.tar.gz` |
| **Windows (x64)** | Single-click Installer | `Clearcore-Setup.exe` |
| **Windows (x64)** | Portable Zip | `Clearcore-win32-x64.zip` |
| **Windows (ARM64)** | Copilot+ Portable | `Clearcore-win32-arm64.zip` |
| **macOS (Apple Silicon)** | DMG Installer | `Clearcore-darwin-arm64.dmg` |
| **macOS (Intel x64)** | DMG Installer | `Clearcore-darwin-x64.dmg` |

---

## Installation & Upgrade

### Fedora / RHEL (RPM)
```bash
sudo dnf install -y https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.4/Clearcore-0.1.0-beta.4-1.x86_64.rpm
```

### Ubuntu / Debian (DEB)
```bash
curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.4/Clearcore-0.1.0-beta.4_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb
```
