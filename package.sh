#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=========================================================="
echo " Clearcore Desktop Application - Build & Package"
echo "=========================================================="

# 1. Build release binaries if missing
if [[ ! -f "${SCRIPT_DIR}/target/release/realtime-noise-service" ]] && command -v cargo >/dev/null 2>&1; then
    echo "🔨 Building Rust realtime-noise-service daemon and filter C-API..."
    cargo build --release -p realtime-noise-service -p realtime-noise-filter-capi
fi

# 2. Build Linux PipeWire helper if missing
if [[ "$(uname -s)" == "Linux" ]] && [[ ! -f "${SCRIPT_DIR}/platform/linux/helper/build/pipewire_helper" ]]; then
    if command -v ninja >/dev/null 2>&1 && command -v meson >/dev/null 2>&1; then
        echo "🔨 Building PipeWire helper..."
        if [[ ! -d "${SCRIPT_DIR}/platform/linux/helper/build" ]]; then
            meson setup "${SCRIPT_DIR}/platform/linux/helper/build" "${SCRIPT_DIR}/platform/linux/helper"
        fi
        ninja -C "${SCRIPT_DIR}/platform/linux/helper/build"
    fi
fi

cd "${SCRIPT_DIR}/crates/app-tauri"
npm run package

echo ""
echo "Packaging complete! Standalone packages located in: ${SCRIPT_DIR}/release"
