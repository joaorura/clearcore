#!/usr/bin/env bash
# ==============================================================================
# Clearcore - Desinstalador Completo do Sistema (macOS)
# ==============================================================================
# Remove completamente o Clearcore do macOS:
# - Encerra todos os processos em execução (Clearcore.app e daemon supervisor)
# - Descarrega e remove os LaunchAgents de inicialização automática
# - Remove o bundle do driver CoreAudio HAL (RealtimeNoiseHAL.driver)
# - Reinicia o coreaudiod para remover o dispositivo virtual imediatamente do sistema
# - Remove Clearcore.app de /Applications ou ~/Applications
# - Remove arquivos de preferências, Application Support e caches
# ==============================================================================

set -uo pipefail

echo "=========================================================="
echo " Clearcore Desktop - Desinstalação Completa do Sistema (macOS)"
echo "=========================================================="

CURRENT_UID=$(id -u)

# 1. Encerrar processos em execução
echo "🛑 Encerrando processos ativos do Clearcore..."
killall "Clearcore" >/dev/null 2>&1 || true
killall "realtime-noise-service" >/dev/null 2>&1 || true
pkill -f "Clearcore.app" >/dev/null 2>&1 || true

# 2. Descarregar e remover LaunchAgents
echo "⚙️ Descarregando e removendo LaunchAgents..."
PLIST_USER="${HOME}/Library/LaunchAgents/com.clearcore.realtime-noise.plist"
PLIST_SYS="/Library/LaunchAgents/com.clearcore.realtime-noise.plist"

if [[ -f "${PLIST_USER}" ]]; then
    launchctl bootout "gui/${CURRENT_UID}" "${PLIST_USER}" >/dev/null 2>&1 || true
    launchctl unload "${PLIST_USER}" >/dev/null 2>&1 || true
    rm -f "${PLIST_USER}"
fi

if [[ -f "${PLIST_SYS}" ]]; then
    launchctl bootout "gui/${CURRENT_UID}" "${PLIST_SYS}" >/dev/null 2>&1 || true
    launchctl unload "${PLIST_SYS}" >/dev/null 2>&1 || true
    sudo rm -f "${PLIST_SYS}" 2>/dev/null || rm -f "${PLIST_SYS}" 2>/dev/null || true
fi

# 3. Remover driver CoreAudio HAL
echo "🎤 Removendo driver virtual CoreAudio HAL..."
DRIVER_NAME="RealtimeNoiseHAL.driver"
USER_HAL_DIR="${HOME}/Library/Audio/Plug-Ins/HAL"
SYS_HAL_DIR="/Library/Audio/Plug-Ins/HAL"

if [[ -d "${USER_HAL_DIR}/${DRIVER_NAME}" ]]; then
    rm -rf "${USER_HAL_DIR}/${DRIVER_NAME}"
fi

if [[ -d "${SYS_HAL_DIR}/${DRIVER_NAME}" ]]; then
    sudo rm -rf "${SYS_HAL_DIR}/${DRIVER_NAME}" 2>/dev/null || rm -rf "${SYS_HAL_DIR}/${DRIVER_NAME}" 2>/dev/null || true
fi

# 4. Reiniciar daemon do CoreAudio para descarregar o driver sem reiniciar o Mac
echo "🔄 Reiniciando coreaudiod para atualizar dispositivos de áudio..."
sudo killall coreaudiod >/dev/null 2>&1 || killall coreaudiod >/dev/null 2>&1 || true

# 5. Remover aplicação
echo "📁 Removendo aplicação Clearcore.app..."
rm -rf "/Applications/Clearcore.app" 2>/dev/null || sudo rm -rf "/Applications/Clearcore.app" 2>/dev/null || true
rm -rf "${HOME}/Applications/Clearcore.app" 2>/dev/null || true

# 6. Limpar suporte de aplicação, preferências e logs
echo "🧹 Limpando configurações, logs e caches..."
rm -rf "${HOME}/Library/Application Support/Clearcore" 2>/dev/null || true
rm -rf "${HOME}/Library/Application Support/clearcore" 2>/dev/null || true
rm -rf "${HOME}/Library/Preferences/com.clearcore.*" 2>/dev/null || true
rm -rf "${HOME}/Library/Logs/Clearcore" 2>/dev/null || true
rm -rf "${HOME}/Library/Caches/com.clearcore.*" 2>/dev/null || true
rm -rf /tmp/com.clearcore.* 2>/dev/null || true

echo ""
echo "=========================================================="
echo "✅ Clearcore foi completamente desinstalado do macOS!"
echo "   - Processos encerrados"
echo "   - LaunchAgents removidos"
echo "   - Driver CoreAudio HAL desinstalado"
echo "   - coreaudiod reiniciado (dispositivo virtual removido)"
echo "   - Clearcore.app e preferências excluídos"
echo "=========================================================="
