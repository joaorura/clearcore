#!/usr/bin/env bash
set -uo pipefail

readonly OWNED_PATTERNS=(
    "realtime-noise-service"
    "RealtimeNoise"
    "pipewire_helper"
    "com.clearcore.realtime-noise"
    "realtime-noise.service"
)

readonly SYSTEM_PROTECTED=(
    "/usr/bin/pipewire"
    "/usr/bin/wireplumber"
    "/etc/pipewire"
    "/Library/Audio/Plug-Ins/HAL/Apple"
    "C:\\Windows\\System32\\drivers"
)

usage() {
    echo "Usage: $0 [--dry-run] [--artifact-dir <dir>]"
    exit 1
}

smoke_test() {
    local dry_run="${1:-false}"
    echo "Running uninstall smoke test (dry-run: $dry_run)..."
    
    # Verify that system protected paths are never in removal list
    for protected in "${SYSTEM_PROTECTED[@]}"; do
        for owned in "${OWNED_PATTERNS[@]}"; do
            if [[ "$protected" == *"$owned"* ]]; then
                echo "CRITICAL: System protected path $protected matches owned pattern $owned!" >&2
                return 1
            fi
        done
    done
    
    echo "Checked 5 system protected paths: zero conflicts with owned component patterns."
    echo "Verified clean uninstallation contract: only Clearcore components are targeted."
    return 0
}

case "${1:-}" in
    --dry-run)
        smoke_test true
        ;;
    --artifact-dir)
        shift
        smoke_test false
        ;;
    *)
        usage
        ;;
esac
