#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_PATH="${SCRIPT_DIR}/target/release/realtime-noise-service"
HELPER_PATH="${SCRIPT_DIR}/platform/linux/helper/build/pipewire_helper"

if [[ ! -x "${BIN_PATH}" ]]; then
    echo "Error: Binary not found or not executable at ${BIN_PATH}" >&2
    exit 1
fi

if [[ ! -x "${HELPER_PATH}" ]]; then
    echo "Error: PipeWire helper not found at ${HELPER_PATH}" >&2
    exit 1
fi

echo "=========================================================="
echo "Starting Clearcore Realtime Noise Daemon"
echo "=========================================================="
echo "Host OS:         $(cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2 | tr -d '\"')"
echo "PipeWire:        $(pipewire --version | head -n 1)"
echo "WirePlumber:     $(wireplumber --version 2>&1 | head -n 1)"
echo "=========================================================="

# Check if already running
if pgrep -f "realtime-noise-service --run" >/dev/null 2>&1; then
    echo "Service daemon is already running."
    exit 0
fi

# Prefer systemd user service if configured
if systemctl --user is-enabled realtime-noise.service >/dev/null 2>&1 || [[ -f "${HOME}/.config/systemd/user/realtime-noise.service" ]]; then
    echo "Starting via systemd user unit..."
    systemctl --user daemon-reload
    systemctl --user start realtime-noise.service
    sleep 0.5
    if systemctl --user is-active --quiet realtime-noise.service; then
        echo "Status: RUNNING (systemd managed)"
        "${SCRIPT_DIR}/scripts/check-virtual-mic.sh"
        echo "To stop: ./stop-realtime-noise.sh"
        exit 0
    fi
fi

# Fallback: Run daemon in background via nohup
nohup "${BIN_PATH}" --run > /tmp/realtime-noise-service.log 2>&1 &
SERVICE_PID=$!
disown "${SERVICE_PID}" 2>/dev/null || true

echo "Service daemon launched with PID: ${SERVICE_PID}"
echo "Logs: /tmp/realtime-noise-service.log"

sleep 1

if kill -0 "${SERVICE_PID}" 2>/dev/null; then
    echo "Status: RUNNING"
    "${SCRIPT_DIR}/scripts/check-virtual-mic.sh"
    echo "To stop: ./stop-realtime-noise.sh"
else
    echo "Status: FAILED TO START. Inspecting logs:"
    cat /tmp/realtime-noise-service.log
    exit 1
fi
