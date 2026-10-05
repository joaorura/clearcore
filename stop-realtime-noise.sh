#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Stopping Clearcore Realtime Noise Daemon & Virtual Microphone..."

if [[ -f "${SCRIPT_DIR}/scripts/check-virtual-mic.sh" ]]; then
    "${SCRIPT_DIR}/scripts/check-virtual-mic.sh" --stop >/dev/null 2>&1 || true
fi

systemctl --user stop realtime-noise-helper.service 2>/dev/null || true
systemctl --user stop realtime-noise.service 2>/dev/null || true
pkill -f "realtime-noise-service --run" || true
pkill -f "pipewire_helper" || true

sleep 0.5
if pgrep -f "realtime-noise-service" >/dev/null 2>&1; then
    pkill -9 -f "realtime-noise-service" || true
fi

echo "Service daemon and virtual microphone stopped."
