# Clearcore Release v0.1.0-beta.3

Multi-platform production release featuring native OS installers for **Fedora / RHEL (`.rpm`)**, **Ubuntu / Debian (`.deb`)**, **Windows ARM64**, alongside AppImage, Windows x64 and macOS bundles, plus integrated in-app **Auto-Update**.

---

## What's New in v0.1.0-beta.3

### 1. Direct OS Installation: Fedora RPM & Debian/Ubuntu DEB
- **Fedora / RHEL / openSUSE**: Native `.rpm` package (`Clearcore-*.x86_64.rpm` and `Clearcore-*.aarch64.rpm`) for direct single-command installation:
  ```bash
  sudo dnf install Clearcore-0.1.0_beta.3-1.x86_64.rpm
  ```
- **Debian / Ubuntu / Linux Mint / Pop!_OS**: Native `.deb` package (`Clearcore-*_amd64.deb` and `Clearcore-*_arm64.deb`) for direct installation:
  ```bash
  sudo apt install ./Clearcore-0.1.0_beta.3_amd64.deb
  ```
- **Linux Universal**: Portable `.AppImage` (runs with no root privileges) and standalone `.tar.gz`.

### 2. Windows ARM64 Support (Snapdragon X Elite & Copilot+ PCs)
- Full native build for `aarch64-pc-windows-msvc` targeting modern ARM-based Windows 11 devices with native efficiency.
- Portable standalone zip package for Windows ARM64.

### 3. Built-In In-App Auto-Updater
- Real-time background checking for GitHub releases.
- Distro-aware matching: automatically presents the user with the 1-click **"Instalar Direto (RPM)"** on Fedora, **"Instalar Direto (DEB)"** on Ubuntu/Debian, or native installer on Windows/macOS.
- In-app download progress indicator with seamless transition to package installation.

### 4. Acoustic Calibration & Voice Profile UX
- Full multi-sample guided enrollment (5 conversational prompts + fallback short sentences).
- Cumulative Voice Sample Gallery with audio playback, sample deletion, and L2 mean vector profile generation.
- Studio DSP presets: *Natural*, *Warm*, *Radio / Broadcast*, and *Off* with smooth crossfades.
- Native ERB speech target LTAS estimation and Neural EQ calibration integration.

---

## Included Packages & Checksums

| Platform / Distro | Architecture | Package Format | Direct Command |
| :--- | :--- | :--- | :--- |
| **Fedora / RHEL / openSUSE** | x86_64 / AArch64 | `.rpm` | `sudo dnf install Clearcore-*.rpm` |
| **Debian / Ubuntu / Mint** | x86_64 / ARM64 | `.deb` | `sudo apt install ./Clearcore-*.deb` |
| **Linux (Universal)** | x86_64 / ARM64 | `.AppImage` / `.tar.gz` | `chmod +x Clearcore-*.AppImage && ./Clearcore-*.AppImage` |
| **Windows** | x64 | `.exe` (NSIS / Inno) & `.zip` | Run `Clearcore-Setup.exe` |
| **Windows ARM64** | ARM64 | `.zip` | Extract and run `Clearcore.exe` |
| **macOS** | Intel x64 & Apple Silicon | `.dmg` & `.tar.gz` | Open `Clearcore.dmg` |
