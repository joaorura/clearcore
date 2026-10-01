# Clearcore Windows Installer (Setup)

This directory contains production installer configurations for building a single-click, turnkey Windows setup installer (`Clearcore-Setup.exe`).

## Features
- Installs the standalone `Clearcore` application into `%LOCALAPPDATA%\Programs\Clearcore` or `Program Files\Clearcore`.
- Automatically installs the Clearcore WaveRT virtual audio driver (`RealtimeNoise.inf`) via `pnputil /add-driver RealtimeNoise.inf /install`.
- Creates Start Menu and Desktop shortcuts configured to start minimized in the tray (`--tray`).
- Registers Windows startup if selected by the user.
- Provides a clean uninstaller in Windows Settings / Add-Remove Programs.

## Build Formats

### 1. NSIS (Nullsoft Scriptable Install System)
File: `Clearcore-Setup.nsi`
Compile with:
```cmd
makensis Clearcore-Setup.nsi
```
Produces: `release\Clearcore-Setup.exe`

### 2. Inno Setup
File: `Clearcore-Setup.iss`
Compile with:
```cmd
iscc Clearcore-Setup.iss
```
Produces: `release\Clearcore-Setup.exe`
