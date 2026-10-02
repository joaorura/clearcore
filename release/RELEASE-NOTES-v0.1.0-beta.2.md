# ClearCore v0.1.0-beta.2 — Linux Package Fix and Accelerator Selector Cleanup

**ClearCore (`v0.1.0-beta.2`)** is a bug-fix beta on top of `v0.1.0-beta.1`. Its main purpose is to fix the Linux package, where the virtual microphone could silently run without noise suppression. It also makes the hardware accelerator selector honest about what the audio engine actually does today.

> **Linux users of `v0.1.0-beta.1`: please reinstall.** In `v0.1.0-beta.1` the app looked for `pipewire_helper` in a path that is not shipped in the package. As a result, "Recreate virtual mic" silently fell back to a loopback **without noise suppression** while the UI still showed *Active*. `v0.1.0-beta.2` looks for the helper in `resources/bin`, where it is shipped.

ClearCore is a cross-platform, realtime AI noise-suppression virtual microphone that runs offline on your machine. For the overall description, architecture and system requirements, see the [`v0.1.0-beta.1` release notes](RELEASE-NOTES-v0.1.0-beta.1.md); none of that changed in this release.

---

## Release Highlights

- **Linux package fix**: the app now finds `pipewire_helper` in `resources/bin`.
- **Microphone selection by node name**: the helper selects the capture microphone by node name instead of numeric id. WirePlumber reads a numeric `target.object` as `object.serial`, which left the virtual mic with no input.
- **Mono microphones**: mono microphones (e.g. Bluetooth headsets) are now linked.
- **Honest accelerator selector**: only **Auto** and **CPU (Tract)** can be selected. Other accelerators are marked "Preview — does not process audio yet", and the false "backend changed" toast was removed.
- **Linux install polish**: the application icon is installed in the hicolor directory matching its real size, and the installer prints a hint when GNOME has no AppIndicator extension (no packages are installed automatically).

---

## Download Artifacts

| Artifact | Platform | Package Type |
|---|---|---|
| **`Clearcore-linux-x64.tar.gz`** | Linux (x86_64) | Standalone Archive with `./install.sh` |
| **`Clearcore-win32-x64.zip`** | Windows (x64) | Portable Application |
| **`Clearcore-darwin-arm64.tar.gz`** | macOS (Apple Silicon) | Application Bundle |

Verify the SHA-256 checksum before installation:

- **Linux**: `sha256sum Clearcore-linux-x64.tar.gz`
- **macOS**: `shasum -a 256 Clearcore-darwin-arm64.tar.gz`
- **Windows (PowerShell)**: `Get-FileHash -Algorithm SHA256 .\Clearcore-win32-x64.zip`

### Upgrading on Linux

Download and extract the new archive, then run the installer again:

```bash
tar -xzf Clearcore-linux-x64.tar.gz
cd Clearcore-linux-x64
./install.sh
```

Installation and usage steps for Windows and macOS are unchanged; see the [`v0.1.0-beta.1` release notes](RELEASE-NOTES-v0.1.0-beta.1.md).

---

## Known Limitations

- The fixes were validated with unit tests and fake PipeWire tools on Fedora 44, not on real hardware of other distributions.
- The Linux fix for microphone selection was not verified on Fedora 41 / PipeWire 1.2.
- macOS and Windows packages contain no functional change other than the shared UI changes described above.
- The OpenVINO, CUDA and NPU backends are not wired to the audio engine in this release. Audio is always processed by Tract on CPU.
- The studio DSP chain and speaker-personalised denoising are not active.

---

## Full Changelog for v0.1.0-beta.2

### Fixed
- Linux package: the app now finds `pipewire_helper` in `resources/bin` (`v0.1.0-beta.1` looked in a path that is not shipped, so "Recreate virtual mic" silently fell back to a loopback WITHOUT noise suppression while the UI showed Active; users of `v0.1.0-beta.1` on Linux should reinstall).
- The helper now selects the capture microphone by node name instead of numeric id (WirePlumber reads a numeric `target.object` as `object.serial`, which left the virtual mic with no input).
- Mono microphones (e.g. Bluetooth headsets) are now linked.
- The application icon is installed in the hicolor directory matching its real size.
- The installer prints a hint when GNOME has no AppIndicator extension (no packages are installed automatically).

### Changed
- Hardware accelerator selector: only Auto and CPU (Tract) can be selected; other accelerators are marked "Preview — does not process audio yet" and the false "backend changed" toast was removed (the OpenVINO/CUDA/NPU backends are not wired to the audio engine in this release; audio is always processed by Tract on CPU).
- Hardware detection now finds OpenVINO installed under `/opt/intel/openvino` or via `INTEL_OPENVINO_DIR` and no longer relies on developer-specific paths.
- Hardware JSON parsing is tolerant to extra output and surfaces detection errors.
- `install-openvino.sh` lists the Fedora packages that exist.
- CI: the nightly hardware-staging workflows are no longer scheduled (no self-hosted runners are registered).
- CI: the release workflow now verifies the Linux package contents and requires per-tag release notes.

### Added
- `studio-dsp` crate (a pure-Rust studio DSP chain: high-pass, EQ, de-esser, compressor, loudness AGC, limiter) with its tests. It is NOT wired into the audio path yet and has no user-visible effect in this release.

---

## Feedback & Bug Reports

- **Issue Tracker**: File bug reports and performance feedback on [GitHub Issues](https://github.com/joaorura/clearcore/issues).
- **Audio Diagnostics**: You can export diagnostic logs directly from the desktop application tray menu -> **Diagnostics**.
- **Security Vulnerabilities**: Please review [`SECURITY.md`](../SECURITY.md) for private reporting procedures.
