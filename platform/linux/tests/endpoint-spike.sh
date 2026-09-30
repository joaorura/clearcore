#!/usr/bin/env bash
set -euo pipefail

# Project Hippocamp - Task 10: Linux PipeWire Native Helper Spike
# Endpoint Verification Test Runner

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
HELPER_DIR="${REPO_ROOT}/platform/linux/helper"
BUILD_DIR="${HELPER_DIR}/build"
NODE_NAME="realtime-noise-source"
NODE_DESC="Realtime Noise Virtual Microphone"

# Setup environment for pkg-config and compiler tools
export PKG_CONFIG_PATH="/home/joaorura/.local/usr/lib64/pkgconfig:${PKG_CONFIG_PATH:-}"
export PATH="/home/joaorura/miniconda3/bin:/home/joaorura/.local/bin:${PATH}"

MODE="${1:-}"

if [[ -z "${MODE}" || ("${MODE}" != "red" && "${MODE}" != "green") ]]; then
    echo "Usage: $0 {red|green}"
    exit 2
fi

echo "========================================================"
echo "Project Hippocamp - Linux PipeWire Native Endpoint Spike"
echo "Mode: ${MODE}"
echo "========================================================"

# Host Diagnostics
OS_INFO=$(cat /etc/os-release | grep PRETTY_NAME | cut -d= -f2 | tr -d '"')
KERNEL_INFO=$(uname -r)
PW_VERSION=$(pipewire --version | head -n 2 | tr '\n' ' ' || echo "unknown")
WP_VERSION=$(wireplumber --version 2>&1 | head -n 2 | tr '\n' ' ' || echo "unknown")

echo "Host OS:         ${OS_INFO}"
echo "Kernel:          ${KERNEL_INFO}"
echo "PipeWire:        ${PW_VERSION}"
echo "WirePlumber:     ${WP_VERSION}"
echo "Target Node:     ${NODE_NAME}"
echo "========================================================"

if [[ "${MODE}" == "red" ]]; then
    echo "[RED] Checking for virtual capture node '${NODE_NAME}' before helper is running..."
    
    NODE_FOUND=0
    if pw-cli list-objects Node 2>/dev/null | grep -F "node.name = \"${NODE_NAME}\"" > /dev/null; then
        NODE_FOUND=1
    fi

    if [[ "${NODE_FOUND}" -eq 1 ]]; then
        echo "[RED FAILURE] Unexpected: Virtual node '${NODE_NAME}' already exists on host!"
        exit 1
    else
        echo "[RED EXPECTED] Node '${NODE_NAME}' does not exist in PipeWire graph."
        echo "[RED EXPECTED] Querying wpctl status:"
        wpctl status | grep -E "Sources:|realtime-noise" || true
        echo "[RED EXPECTED] Querying pactl sources:"
        pactl list sources short | grep -F "${NODE_NAME}" || true
        echo "[RED VERIFIED] Assertion failed as expected in RED phase: capture endpoint is absent."
        # Exit with non-zero code to confirm RED test state
        exit 1
    fi
fi

if [[ "${MODE}" == "green" ]]; then
    echo "[GREEN] Step 1: Building native PipeWire helper..."
    if [[ ! -d "${BUILD_DIR}" ]]; then
        meson setup "${BUILD_DIR}" "${HELPER_DIR}"
    fi
    ninja -C "${BUILD_DIR}"

    echo "[GREEN] Step 2: Running RT callback contract & unit tests..."
    "${BUILD_DIR}/test_callback_contract"
    echo "[GREEN] Unit tests passed."

    echo "[GREEN] Step 3: Launching pipewire_helper..."
    "${BUILD_DIR}/pipewire_helper" &
    HELPER_PID=$!
    echo "[GREEN] Helper launched with PID ${HELPER_PID}."

    cleanup() {
        echo "[GREEN] Cleaning up helper PID ${HELPER_PID}..."
        if kill -0 "${HELPER_PID}" 2>/dev/null; then
            kill -TERM "${HELPER_PID}" 2>/dev/null || true
            wait "${HELPER_PID}" 2>/dev/null || true
        fi
    }
    trap cleanup EXIT

    echo "[GREEN] Step 4: Waiting for virtual node registration..."
    WAIT_SEC=0
    NODE_ID=""
    while [[ ${WAIT_SEC} -lt 50 ]]; do
        NODE_LINE=$(pw-cli list-objects Node 2>/dev/null | grep -B 5 -F "node.name = \"${NODE_NAME}\"" || true)
        if [[ -n "${NODE_LINE}" ]]; then
            NODE_ID=$(echo "${NODE_LINE}" | grep "id " | head -n 1 | awk '{print $2}' | tr -d ',')
            break
        fi
        sleep 0.1
        WAIT_SEC=$((WAIT_SEC + 1))
    done

    if [[ -z "${NODE_ID}" ]]; then
        echo "[GREEN FAILURE] Timeout: Node '${NODE_NAME}' was not registered within 5s."
        exit 1
    fi
    echo "[GREEN] Node registered with PipeWire Node ID: ${NODE_ID}"

    echo "[GREEN] Step 5: Validating PipeWire node properties..."
    NODE_PROPS=$(pw-cli info "${NODE_ID}" 2>/dev/null)
    echo "${NODE_PROPS}" | grep -F "media.class = \"Audio/Source\""
    echo "${NODE_PROPS}" | grep -F "node.name = \"${NODE_NAME}\""
    echo "${NODE_PROPS}" | grep -F "node.description = \"${NODE_DESC}\""

    echo "[GREEN] Step 6: Validating WirePlumber visibility..."
    WPCTL_OUTPUT=$(wpctl status)
    echo "${WPCTL_OUTPUT}" | grep -F "${NODE_DESC}"

    echo "[GREEN] Step 7: Validating pipewire-pulse compatibility..."
    PA_SOURCES=$(pactl list sources short)
    echo "${PA_SOURCES}" | grep -F "${NODE_NAME}"

    echo "[GREEN] Step 8: Capturing audio stream and validating fail-closed digital silence..."
    TEST_RAW="/tmp/spike_green_silence.raw"
    rm -f "${TEST_RAW}"
    # Capture 4800 samples (100 ms @ 48 kHz Float32 mono)
    pw-record --target "${NODE_NAME}" --rate 48000 --channels 1 --format f32 --sample-count 4800 "${TEST_RAW}" || true
    
    RAW_SIZE=$(stat -c%s "${TEST_RAW}")
    EXPECTED_SIZE=$((4800 * 4)) # 19200 bytes
    if [[ "${RAW_SIZE}" -ne "${EXPECTED_SIZE}" ]]; then
        echo "[GREEN FAILURE] Captured file size ${RAW_SIZE} != expected ${EXPECTED_SIZE}"
        exit 1
    fi

    # Verify that all captured bytes are exactly 0 (pure digital silence)
    NON_ZERO_COUNT=$(python3 -c "import sys; data = open(sys.argv[1], 'rb').read(); print(sum(1 for b in data if b != 0))" "${TEST_RAW}")
    if [[ "${NON_ZERO_COUNT}" -ne 0 ]]; then
        echo "[GREEN FAILURE] Stream leaked non-zero audio samples! (${NON_ZERO_COUNT} non-zero bytes)"
        exit 1
    fi
    echo "[GREEN] Verified pure digital silence: 4800 Float32 samples (19200 bytes), zero raw audio leakage."

    echo "========================================================"
    echo "[GREEN SUCCESS] PipeWire virtual capture endpoint fully validated!"
    echo "========================================================"
    exit 0
fi
