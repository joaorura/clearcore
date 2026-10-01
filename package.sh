#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=========================================================="
echo " Clearcore Desktop Application - Build & Package"
echo "=========================================================="

# 1. Build release binaries
if command -v cargo >/dev/null 2>&1; then
    echo "🔨 Building Rust realtime-noise-service daemon and filter C-API..."
    cargo build --release -p realtime-noise-service -p realtime-noise-filter-capi -p realtime-noise-app-tauri
fi

# 2. Build Linux PipeWire helper
if [[ "$(uname -s)" == "Linux" ]]; then
    if command -v ninja >/dev/null 2>&1 && command -v meson >/dev/null 2>&1; then
        echo "🔨 Building PipeWire helper..."
        if [[ ! -d "${SCRIPT_DIR}/platform/linux/helper/build" ]]; then
            meson setup "${SCRIPT_DIR}/platform/linux/helper/build" "${SCRIPT_DIR}/platform/linux/helper"
        fi
        ninja -C "${SCRIPT_DIR}/platform/linux/helper/build"
    fi
fi

# 3. Build macOS CoreAudio HAL driver if missing
if [[ "$(uname -s)" == "Darwin" ]] && [[ ! -d "${SCRIPT_DIR}/platform/macos/HAL/RealtimeNoiseHAL.driver" ]]; then
    if command -v xcodebuild >/dev/null 2>&1; then
        echo "🔨 Building macOS CoreAudio HAL driver..."
        xcodebuild -project "${SCRIPT_DIR}/platform/macos/HAL/RealtimeNoiseHAL.xcodeproj" \
                   -scheme RealtimeNoiseHAL \
                   -configuration Release \
                   -derivedDataPath "${SCRIPT_DIR}/platform/macos/HAL/build" \
                   CODE_SIGN_IDENTITY="" CODE_SIGNING_REQUIRED=NO build || true
        
        if [[ -d "${SCRIPT_DIR}/platform/macos/HAL/build/Build/Products/Release/RealtimeNoiseHAL.driver" ]]; then
            cp -R "${SCRIPT_DIR}/platform/macos/HAL/build/Build/Products/Release/RealtimeNoiseHAL.driver" "${SCRIPT_DIR}/platform/macos/HAL/RealtimeNoiseHAL.driver"
        fi
    fi
fi

cd "${SCRIPT_DIR}/crates/app-tauri"
npm run package

echo ""
echo "Packaging complete! Standalone packages located in: ${SCRIPT_DIR}/release"
