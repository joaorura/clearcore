#!/usr/bin/env bash
set -uo pipefail

readonly BLOCKED_STATUS="BLOCKED_UNLISTED_ASSET"

# Approved production assets list
readonly APPROVED_ASSETS=(
    "realtime-noise-service"
    "realtime-noise-service.exe"
    "RealtimeNoise.sys"
    "RealtimeNoise.inf"
    "RealtimeNoiseHAL.driver"
    "pipewire_helper"
    "com.clearcore.realtime-noise.plist"
    "realtime-noise.service"
    "register-user-service.ps1"
    "unregister-user-service.ps1"
    "realtime-noise-app-tauri"
    "realtime-noise-app-tauri.exe"
)

usage() {
    echo "Usage: $0 [--artifact <file>] [--artifact-dir <dir>]"
    exit 1
}

is_approved() {
    local target="$1"
    local base
    base="$(basename "$target")"
    
    # Strip target triple suffix if present (e.g. realtime-noise-service-x86_64-unknown-linux-gnu)
    local stripped="${base%%-x86_64*}"
    stripped="${stripped%%-aarch64*}"
    stripped="${stripped%%-arm64*}"
    
    for approved in "${APPROVED_ASSETS[@]}"; do
        if [[ "$base" == "$approved"* ]] || [[ "$stripped" == "$approved"* ]]; then
            return 0
        fi
    done
    return 1
}

scan_file() {
    local file="$1"
    if ! is_approved "$file"; then
        echo "$BLOCKED_STATUS: unlisted or unauthorized asset found: $file" >&2
        return 1
    fi
    echo "OK: $file approved"
    return 0
}

scan_dir() {
    local dir="$1"
    local unapproved=0
    
    if [[ ! -d "$dir" ]]; then
        echo "Error: Directory $dir does not exist" >&2
        return 1
    fi
    
    while IFS= read -r -d '' file; do
        if ! is_approved "$file"; then
            echo "$BLOCKED_STATUS: unlisted asset: $file" >&2
            ((unapproved++))
        fi
    done < <(find "$dir" -maxdepth 3 -type f ! -name "*.md" ! -name "*.json" ! -name "*.sha256" -print0)
    
    if ((unapproved > 0)); then
        echo "Scan failed: $unapproved unlisted assets detected" >&2
        return 1
    fi
    echo "All assets in $dir approved"
    return 0
}

if [[ $# -eq 0 ]]; then
    usage
fi

case "$1" in
    --artifact)
        shift
        [[ $# -eq 1 ]] || usage
        scan_file "$1"
        ;;
    --artifact-dir)
        shift
        [[ $# -eq 1 ]] || usage
        scan_dir "$1"
        ;;
    *)
        usage
        ;;
esac
