#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================================="
echo " Parando Orca / Clearcore (Daemon + App UI)"
echo "=========================================================="

# 1. Parar o daemon de audio
"${SCRIPT_DIR}/stop-realtime-noise.sh"

# 2. Parar a interface UI
systemctl --user stop realtime-noise-ui.service 2>/dev/null || true
pkill -f "vite --host 127.0.0.1" || true

echo "Todos os servicos foram finalizados com sucesso."
echo "=========================================================="
