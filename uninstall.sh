#!/usr/bin/env bash
# ==============================================================================
# Clearcore - Desinstalador Universal Multi-Plataforma
# ==============================================================================
# Detecta o sistema operacional e executa o fluxo completo de desinstalação:
# - Linux: scripts/uninstall-linux.sh (Processos, systemd, PipeWire, arquivos)
# - macOS: scripts/uninstall-macos.sh (Processos, LaunchAgents, HAL CoreAudio, coreaudiod)
# - Windows: scripts/uninstall-windows.ps1 (Processos, Driver WaveRT, serviços, Registro)
# ==============================================================================

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CURRENT_OS="$(uname -s 2>/dev/null || echo "Unknown")"

if [[ "${CURRENT_OS}" == "Darwin" ]]; then
    exec "${SCRIPT_DIR}/scripts/uninstall-macos.sh" "$@"
elif [[ "${CURRENT_OS}" == *"MINGW"* || "${CURRENT_OS}" == *"MSYS"* || "${CURRENT_OS}" == *"CYGWIN"* || "${OS:-}" == "Windows_NT" ]]; then
    WIN_SCRIPT="${SCRIPT_DIR}/scripts/uninstall-windows.ps1"
    exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${WIN_SCRIPT}" "$@"
else
    exec "${SCRIPT_DIR}/scripts/uninstall-linux.sh" "$@"
fi
