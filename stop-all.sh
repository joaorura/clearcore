#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================================="
echo " Parando ClearCore (Daemon + App UI)"
echo "=========================================================="

# 1. Parar o microfone virtual e helper nativo PipeWire
if [[ -f "${SCRIPT_DIR}/scripts/check-virtual-mic.sh" ]]; then
    "${SCRIPT_DIR}/scripts/check-virtual-mic.sh" --stop >/dev/null 2>&1 || true
fi

# 2. Parar o daemon de audio
"${SCRIPT_DIR}/stop-realtime-noise.sh"

# 3. Parar o Desktop Companion (Electron) e servicos UI
systemctl --user stop realtime-noise-electron.service 2>/dev/null || true
systemctl --user stop realtime-noise-ui.service 2>/dev/null || true
pkill -f "electron.*main.cjs" || true
pkill -f "vite --host 127.0.0.1" || true

echo "Todos os servicos e companion da bandeja foram finalizados com sucesso."
echo "=========================================================="
