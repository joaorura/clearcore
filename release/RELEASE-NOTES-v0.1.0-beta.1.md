# ClearCore v0.1.0-beta.1 — modo isolamento com o pDFNet3 Pro v7

Esta é a **v0.1.0-beta.1 republicada**: o mesmo nome de versão, agora com o novo modelo de isolamento de voz. O ClearCore continua rodando inteiramente offline: sem nuvem, sem conta, nenhum áudio sai do computador.

---

## Novidades

- **Modo isolamento com o modelo v7**: com um perfil de voz aplicado (intensidade > 0), o daemon e o microfone virtual passam a usar o **pDFNet3 Pro v7** (A2-300k, com gate de silêncio C), que preserva a voz do usuário no headset e remove as outras vozes e o ruído de fundo. Sem perfil, ou com intensidade 0, segue rodando o Pro v3 full em modo neutro.
- **Latência**: com perfil, a latência total é de **240 ms**; sem perfil, 30 ms.
- **Integridade**: o arquivo do modelo v7 (`models/pdfnet3/pdfnet3-pro-v7-a2-300k-stateful-asset-v1.tar.gz`) vai em todos os pacotes com SHA-256 fixado no código e conferido antes de carregar (`b0f125d71d8764e3b73e91b667b2cb7aba1ef1e4b6987c53e9019958f5369608`).

## Atenção ao atualizar

- **Perfis antigos precisam ser recadastrados.** Um perfil gravado numa versão anterior não é aplicado pelo v7: a voz fica neutra e o app pede um novo cadastro. Grave as amostras e gere o perfil de novo.
- **Quem tinha a beta.2 instalada deve desinstalar e instalar de novo.** A versão `0.1.0-beta.1` é numericamente menor que a `0.1.0-beta.2`, então o gerenciador de pacotes não a trata como atualização. No Fedora: `sudo dnf remove clearcore && sudo dnf install clearcore`; nos demais sistemas, desinstale e instale o pacote desta release.

---

## 📦 Included Packages & Installers

| Platform / Distro | Architecture | Package Format | Direct Command / Installation |
| :--- | :--- | :--- | :--- |
| **Fedora / RHEL / openSUSE** | x86_64 / AArch64 | Native `.rpm` | `sudo dnf copr enable joaorura/clearcore && sudo dnf install -y clearcore` |
| **Debian / Ubuntu / Mint** | amd64 / arm64 | Native `.deb` | `curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.1/Clearcore-0.1.0-beta.1_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb` |
| **Linux (Universal)** | x86_64 / ARM64 | `.AppImage` / `.tar.gz` | `chmod +x Clearcore-*.AppImage && ./Clearcore-*.AppImage` |
| **Windows** | x64 | `.exe` (NSIS / Inno) & `.zip` | `winget install joaorura.Clearcore` (or run `Clearcore-Setup.exe`) |
| **Windows ARM64** | ARM64 (Snapdragon X) | `.zip` | Extract and run `Clearcore.exe` |
| **macOS** | Apple Silicon ARM64 | `.dmg` & `.tar.gz` | Open `Clearcore.dmg` or run `./install.sh` |

---

## 🔒 SHA-256 Integrity Verification

To ensure your downloaded binaries have not been corrupted or tampered with, verify the SHA-256 checksums before installation.

### Verification Commands

- **Linux**:
  ```bash
  sha256sum Clearcore-linux-x64.tar.gz
  ```

- **macOS**:
  ```bash
  shasum -a 256 Clearcore-darwin-arm64.tar.gz
  ```

- **Windows (PowerShell)**:
  ```powershell
  Get-FileHash -Algorithm SHA256 .\Clearcore-win32-x64.zip
  ```

---

## ⚡ Quick Start Guide

### Linux
1. Download and extract the archive:
   ```bash
   tar -xzf Clearcore-linux-x64.tar.gz
   cd Clearcore-linux-x64
   ```
2. Run directly or install to your user environment:
   ```bash
   # Run directly (standalone)
   ./clearcore

   # Or install system menu shortcut and autostart
   ./install.sh
   ```
3. Open your voice application (Discord, Zoom, OBS Studio) and select **"Realtime Noise Virtual Microphone"** (`realtime-noise-source`) as your input device.

### Windows
1. Download and extract **`Clearcore-win32-x64.zip`**.
2. Run **`Clearcore.exe`**:
   - The application automatically launches the companion background daemon (`realtime-noise-service.exe`) and minimizes to the system tray.
   - *SmartScreen*: In this open-source beta, Windows SmartScreen may show an "Unknown Publisher" prompt. Click **"More info"** -> **"Run anyway"**.
3. **Autostart on Boot**:
   ```powershell
   powershell .\resources\scripts\setup-autostart.ps1 -Action enable
   ```
4. **Virtual Microphone Driver (Kernel WaveRT)**:
   - Kernel drivers (`.sys`) in Windows require strict signing. For community beta testing:
     1. Open PowerShell or Command Prompt as **Administrator** and enable test signing:
        ```cmd
        bcdedit /set testsigning on
        ```
     2. Restart your computer.
     3. You can sign or check the driver with the bundled tools:
        ```powershell
        powershell .\resources\scripts\sign-windows-binaries.ps1
        powershell .\resources\scripts\check-virtual-mic-windows.ps1 -Recreate
        ```
   *(Note: The all-in-one `Clearcore-Setup.exe` with EV/WHQL signed driver pipeline is scheduled for General Availability).*

### macOS
1. Download and extract **`Clearcore-darwin-arm64.tar.gz`**.
2. Run the included `install.sh` with administrative privileges to place the CoreAudio HAL driver in `/Library/Audio/Plug-Ins/HAL/`:
   ```bash
   sudo ./install.sh
   ```
3. Launch **`Clearcore.app`** from your Applications folder.

---

## 💻 System Requirements

### Supported Operating Systems
- **Linux**: Ubuntu 22.04 LTS+, Fedora 38+, Arch Linux, Debian 12+, or any system with **PipeWire 0.3+** and **WirePlumber**.
- **Windows**: Windows 10 (build 19041+) or Windows 11 (22H2+) 64-bit.
- **macOS**: macOS 13 (Ventura), macOS 14 (Sonoma), or macOS 15 (Sequoia).

### Hardware Requirements
- **Processor**: 64-bit x86 processor with **AVX2** support (Intel Haswell / AMD Zen or newer), or Apple Silicon (M1/M2/M3/M4 via Rosetta 2 or native HAL).
- **RAM**: Minimum 512 MB free RAM (typical resident set size ~75 MB).
- **Disk Space**: ~250 MB free disk space.
- **CPU**: the pDFNet3 Pro runs on the CPU and requires x86-64 with AVX2. Known limitation of this beta: on ARM64 builds (Linux ARM64, Windows ARM64, macOS Apple Silicon) the isolation model does not load and the app reports `MODEL_LOAD_FAILED`. Hardware accelerators are not used by this release.

---

## 📋 Mudanças desta republicação

### Adicionado
- pDFNet3 Pro v7 (A2-300k, gate de silêncio C) como modelo do modo isolamento quando há perfil de voz: daemon em tract e microfone virtual em ONNX Runtime (1 thread). Contrato em `docs/pdfnet3-pro-v7-stream.md`.
- Perfis de voz gerados a partir desta versão são perfis v7 (`model_id` `pdfnet3-pro-v7-a2-300k`); `GetStatus` informa `isolation_model_variant` (`pdfnet3-pro-v7` / `pdfnet3-pro-v3`).

### Mantido
- Pro v3 full sem perfil, cadastro de voz guiado, cadeia DSP de estúdio, microfone virtual nativo (PipeWire, WaveRT, CoreAudio) e runtime OpenVINO embutido nos pacotes Linux x64 e Windows x64.

## 💬 Feedback & Bug Reports

As this is a Beta release, your feedback is crucial in helping us reach General Availability (GA):
- **Issue Tracker**: File bug reports and performance feedback on [GitHub Issues](https://github.com/joaorura/clearcore/issues).
- **Audio Diagnostics**: You can export diagnostic logs directly from the desktop application tray menu -> **Diagnostics**.
- **Security Vulnerabilities**: Please review [`SECURITY.md`](../SECURITY.md) for private reporting procedures.
