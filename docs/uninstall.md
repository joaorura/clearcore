# Uninstallation & Cleanup Protocol

## Overview

Clearcore Realtime Noise strictly adheres to isolated component lifecycle management. Uninstallation removes only Clearcore components and leaves system and third-party configurations completely untouched.

---

## Windows

### MSI / Settings Uninstaller
- Unregisters the background user service (`realtime-noise-service --uninstall-user-service`).
- Removes the SysVAD WaveRT driver from DriverStore (`pnputil /delete-driver RealtimeNoise.inf /uninstall`).
- Removes binaries from `C:\Program Files\Clearcore\RealtimeNoise\`.
- Leaves Windows Audio service, WASAPI endpoints, and device registry intact.

---

## macOS

### Package Cleanup
- Unloads and removes the per-user LaunchAgent:
  ```bash
  launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.clearcore.realtime-noise.plist
  rm ~/Library/LaunchAgents/com.clearcore.realtime-noise.plist
  ```
- Removes the CoreAudio HAL plug-in:
  ```bash
  sudo rm -rf /Library/Audio/Plug-Ins/HAL/RealtimeNoiseHAL.driver
  sudo killall coreaudiod
  ```
- Removes application binaries from `/Applications/Clearcore/`.
- Leaves all system Apple HAL plug-ins and user audio preferences untouched.

---

## Linux (Ubuntu & Fedora)

### APT / DNF Package Removal
- Unregisters and stops the user systemd unit:
  ```bash
  systemctl --user stop realtime-noise.service
  systemctl --user disable realtime-noise.service
  ```
- Removes package:
  ```bash
  # Debian/Ubuntu:
  sudo apt remove realtime-noise
  # Fedora/RHEL:
  sudo dnf remove realtime-noise
  ```
- PipeWire graph automatically removes the virtual `realtime-noise-source` node without restarting PipeWire or WirePlumber.
- Audio streams seamlessly fallback to default physical microphones.
