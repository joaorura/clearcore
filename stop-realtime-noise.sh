#!/usr/bin/env bash
set -uo pipefail

echo "Stopping Clearcore Realtime Noise Daemon..."

pkill -f "realtime-noise-service --run" || true
pkill -f "pipewire_helper" || true

sleep 0.5
if pgrep -f "realtime-noise-service" >/dev/null 2>&1; then
    pkill -9 -f "realtime-noise-service" || true
fi

echo "Service daemon stopped."
