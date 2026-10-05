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

# Check if already running, and whether it runs an older binary than the current build
# (cargo replaces the file: the running one shows as "(deleted)" or started before it).
if RUNNING_PID="$(pgrep -o -f "realtime-noise-service --run")"; then
    RUNNING_EXE="$(readlink "/proc/${RUNNING_PID}/exe" 2>/dev/null || true)"
    OUTDATED=0
    if [[ "${RUNNING_EXE}" == "${BIN_PATH} (deleted)" ]]; then
        OUTDATED=1
    elif [[ "${RUNNING_EXE}" == "${BIN_PATH}" && -d "/proc/${RUNNING_PID}" ]] \
        && (( $(stat -c %Y "${BIN_PATH}") > $(stat -c %Y "/proc/${RUNNING_PID}") )); then
        OUTDATED=1
    fi
    if (( OUTDATED )); then
        echo "AVISO: o daemon em execução (PID ${RUNNING_PID}) é de um binário mais antigo que ${BIN_PATH}."
        echo "Reiniciando para carregar a versão atual..."
        "${SCRIPT_DIR}/stop-realtime-noise.sh"
    elif [[ -n "${RUNNING_EXE}" && "${RUNNING_EXE%" (deleted)"}" != "${BIN_PATH}" ]]; then
        echo "Service daemon is already running from another binary: ${RUNNING_EXE}"
        echo "AVISO: não é o binário deste checkout (${BIN_PATH}); ele pode estar desatualizado."
        echo "Para usar este: ./stop-realtime-noise.sh && ./start-realtime-noise.sh"
        exit 0
    else
        echo "Service daemon is already running."
        exit 0
    fi
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

# Fallback: Run daemon in background via nohup, with core dumps disabled (the daemon holds
# raw PCM in memory during voice enrollment; the systemd unit sets LimitCORE=0 itself).
if command -v prlimit >/dev/null 2>&1; then
    NO_CORE=(prlimit --core=0 --)
else
    NO_CORE=(sh -c 'ulimit -c 0; exec "$0" "$@"')
fi
nohup "${NO_CORE[@]}" "${BIN_PATH}" --run > /tmp/realtime-noise-service.log 2>&1 &
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
