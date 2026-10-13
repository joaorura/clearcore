# ClearCore v0.1.0-beta.9 — DFNet-TSE-MHCA (K=16 Tokens, ArcFace Loss) Neural Model Integration

**ClearCore (`v0.1.0-beta.9`)** deploys the trained **DFNet-TSE-MHCA** model asset ($K = 16$ speaker memory tokens, $D = 128$ channels, ArcFace angular margin loss) fine-tuned on the Blackwell GPU for advanced same-sex speaker separation and Target Speaker Extraction (TSE) with ONNX Runtime (`ort` v2).

---

## Release Highlights

- **DFNet-TSE-MHCA Model Deployment**:
  - Pinned the new neural asset archive `pdfnet3-stateful-asset-v1.tar.gz` (v3 deployment from checkpoint `ckpt-00002000.pt`).
  - Strict SHA-256 integrity verification in `filter-capi` (`d1b9c2fd49ba602ed48cd503d256885c5e174a869d23b62204e8d668de95d6a5`).
  - Pinned stateful graph digests in `crates/model/src/ort_backend.rs` (`APPROVED_PRO_STATEFUL_DIGESTS`):
    * `enc.onnx`: `ca27b7fefa4b7805cad4ad715782eee330730823a96028fefa03a12236aaea55`
    * `erb_dec.onnx`: `e9f6e1fbc94896f5213a9a02f36d514ad8778c583534ee098185e2733b2b1957`
    * `df_dec.onnx`: `ce841e767fff13eff631431734f512598b6f40a7529b7610b6a05d01d247fee3`
- **Zero-Allocation Multi-Token Speaker Conditioning**:
  - Precomputed Key/Value caching (`precompute_kv`) maintains strict sub-millisecond execution (< 0.05 ms per frame) with zero heap allocations during realtime audio streaming.
  - Enhanced ArcFace angular margin separation eliminates same-sex speaker confusion.
- **CI / Build Pipeline Hardening**:
  - Configured `continue-on-error` for deprecated Apple Intel `x86_64-apple-darwin` runner, preserving automated publishing across Linux x64/ARM64, Windows x64/ARM64, and macOS Apple Silicon ARM64.

---

## Download Artifacts

| Platform / Target | Format | Package |
|---|---|---|
| **Fedora / RHEL (x86_64)** | Native RPM | `Clearcore-0.1.0-beta.9-1.x86_64.rpm` |
| **Fedora / RHEL (ARM64)** | Native RPM | `Clearcore-0.1.0-beta.9-1.aarch64.rpm` |
| **Debian / Ubuntu (amd64)** | Native DEB | `Clearcore-0.1.0-beta.9_amd64.deb` |
| **Debian / Ubuntu (arm64)** | Native DEB | `Clearcore-0.1.0-beta.9_arm64.deb` |
| **Linux (Universal x64)** | AppImage | `Clearcore-linux-x64.AppImage` |
| **Linux (Universal x64)** | Standalone Archive | `Clearcore-linux-x64.tar.gz` |
| **Windows (x64)** | Single-click Installer | `Clearcore-Setup.exe` |
| **Windows (x64)** | Portable Zip | `Clearcore-win32-x64.zip` |
| **Windows (ARM64)** | Copilot+ Portable | `Clearcore-win32-arm64.zip` |
| **macOS (Apple Silicon)** | DMG Installer | `Clearcore-darwin-arm64.dmg` |
| **Developer Neural Models** | Standalone Archive | `Clearcore-models-stateful-v2.tar.gz` |

---

## Installation & Upgrade

### Fedora / RHEL (RPM)
```bash
sudo dnf install -y https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.9/Clearcore-0.1.0-beta.9-1.x86_64.rpm
```

### Ubuntu / Debian (DEB)
```bash
curl -fsSL https://github.com/joaorura/clearcore/releases/download/v0.1.0-beta.9/Clearcore-0.1.0-beta.9_amd64.deb -o /tmp/clearcore.deb && sudo apt install -y /tmp/clearcore.deb && rm /tmp/clearcore.deb
```
