# ClearCore v0.1.0-beta.5 — pDFNet3 Pro v2, Dual-Path Architecture & Multi-Token Speaker Memory

**ClearCore (`v0.1.0-beta.5`)** introduces the next-generation neural model **pDFNet3 Pro v2** featuring a Dual-Path Frequency DW-Conv Mixer, Multi-Token Speaker Memory Dictionary ($K=8$ tokens), advanced anti-confusion loss functions, standalone neural model exports for developers, and upgraded TensorRT Blackwell acceleration.

---

## Release Highlights

- **Neural Architecture (pDFNet3 Pro v2)**:
  - **Dual-Path Frequency Mixer (`FrequencyDWConvMixer`)**: Depthwise 2D Convolution across 32 ERB frequency bands ($k=9$) + Pointwise 1×1 Conv + BatchNorm + GELU + residual connections before the temporal GRU backbone.
  - **Multi-Token Speaker Memory Dictionary**: Scaled dot-product causal cross-attention with $K=8$ tokens $\times$ 64 dimensions, eliminating single-token attention collapse and enabling fine-grained phonetic matching.
  - **Advanced Loss Formulation**:
    - $\mathcal{L}_{\text{abs}}$ (*Target-Absence Loss*): Complete energy zeroing during absence of target speech.
    - $\mathcal{L}_{\text{conf}}$ (*Interferer Rejection InfoNCE*): Explicit speaker contrastive penalty pushing away background voices.
    - $\mathcal{L}_{\text{DF-reg}}$ (*Deep Filter Temporal Regularization*): First-order $L_1$ smoothness on complex filter coefficients frame-by-frame, completely eliminating metallic swirling and phase artifacts.
    - *Asymmetric Formant Loss*: Psychoacoustic frequency weighting (300 Hz – 3.4 kHz) preventing over-suppression.
- **Objective Evaluation Metrics (Benchmarked on Holding-Out Test Sets)**:
  - **Headset Microphone (Background Voices)**: Pure noise/leakage suppression raised to **20.03 dB** (silence pauses: **67.8%**, +13.0% vs baseline), with $\Delta$SI-SDR of **+4.93 dB**.
  - **Bluetooth Microphone (Baseus Bass BC1 / HFP Leakage)**: Rejection of **23.43 dB** (silence pauses: **88.2%**, +7.6% vs baseline), $\Delta$SI-SDR of **+4.57 dB**, and $\Delta$STOI of **+0.034**.
  - **Laptop Built-in Microphone**: Noise attenuation of **18.71 dB** (silence pauses: **67.7%**, +9.4% vs baseline).
  - **Environmental Room Noise**: Total attenuation of **22.62 dB** (silence pauses: **72.9%**, +9.7% vs baseline).
- **Standalone Neural Models & Multi-Runtime Export**:
  - Independent distribution tarball (`Clearcore-models-stateful-v2.tar.gz`) containing stateful ONNX graphs and pre-compiled NVIDIA TensorRT Blackwell engines for direct developer integration without the full desktop wrapper.
  - Verified real-time latency: **0.025 ms** per frame on GPU (well below the 10.0 ms audio hop deadline).

---

## Download Artifacts

| Platform / Target | Format | Package |
|---|---|---|
| **Fedora / RHEL (x86_64)** | Native RPM | `Clearcore-0.1.0-beta.5-1.x86_64.rpm` |
| **Fedora / RHEL (ARM64)** | Native RPM | `Clearcore-0.1.0-beta.5-1.aarch64.rpm` |
| **Debian / Ubuntu (amd64)** | Native DEB | `Clearcore-0.1.0-beta.5_amd64.deb` |
| **Debian / Ubuntu (arm64)** | Native DEB | `Clearcore-0.1.0-beta.5_arm64.deb` |
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
sudo dnf install -y https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.5/Clearcore-0.1.0-beta.5-1.x86_64.rpm
```

### Ubuntu / Debian (DEB)
```bash
curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.5/Clearcore-0.1.0-beta.5_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb
```
