#!/usr/bin/env bash
# ==============================================================================
# Clearcore - Desinstalador Completo do Sistema (Linux)
# ==============================================================================
# Remove completamente o Clearcore do sistema operacional:
# - Encerra todos os processos em execução (Clearcore, daemon, helper, loopbacks)
# - Desativa e remove serviços systemd de usuário
# - Remove entradas de autostart na inicialização
# - Remove o microfone virtual do grafo do PipeWire / WirePlumber
# - Remove arquivos binários, atalhos de menu desktop (.desktop) e ícones
# - Limpa arquivos de estado compartilhado, travas de execução e sockets IPC
# ==============================================================================

set -uo pipefail

echo "=========================================================="
echo " Clearcore Desktop - Desinstalação Completa do Sistema"
echo "=========================================================="

# 1. Encerrar todos os processos do Clearcore
echo "🛑 Encerrando processos ativos do Clearcore..."
pkill -x "clearcore" >/dev/null 2>&1 || true
pkill -x "pipewire_helper" >/dev/null 2>&1 || true
pkill -x "realtime-noise-service" >/dev/null 2>&1 || true
pkill -f "^pw-loopback.*realtime-noise" >/dev/null 2>&1 || true

# 2. Desativar e remover serviços systemd de usuário
echo "⚙️ Desativando serviços systemd de usuário..."
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user stop realtime-noise-helper.service >/dev/null 2>&1 || true
    systemctl --user stop realtime-noise.service >/dev/null 2>&1 || true
    systemctl --user disable realtime-noise-helper.service >/dev/null 2>&1 || true
    systemctl --user disable realtime-noise.service >/dev/null 2>&1 || true
    rm -f "${HOME}/.config/systemd/user/realtime-noise-helper.service" 2>/dev/null || true
    rm -f "${HOME}/.config/systemd/user/realtime-noise.service" 2>/dev/null || true
    systemctl --user daemon-reload >/dev/null 2>&1 || true
fi

# 3. Remover entradas de inicialização automática (Autostart)
echo "🚀 Removendo inicialização automática..."
rm -f "${HOME}/.config/autostart/clearcore.desktop" 2>/dev/null || true
rm -f "${HOME}/.config/autostart/realtime-noise.desktop" 2>/dev/null || true

# 4. Remover links e nós do PipeWire
echo "🎤 Desconectando nó virtual do PipeWire..."
if command -v pw-cli >/dev/null 2>&1; then
    NODE_ID=$(pw-cli list-objects Node 2>/dev/null | awk -v name="\"realtime-noise-source\"" '
        $1 == "id" { id = $2; sub(/,/, "", id) }
        $0 ~ "node.name = " name { print id; exit }
    ')
    if [[ -n "${NODE_ID}" ]]; then
        pw-cli destroy "${NODE_ID}" >/dev/null 2>&1 || true
    fi
fi

# 5. Limpar travas, sockets e arquivos de estado de tempo real
echo "🧹 Limpando soquetes IPC e arquivos de memória compartilhada..."
RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp}"
rm -f "${RUNTIME_DIR}/clearcore_state" 2>/dev/null || true
rm -f "${RUNTIME_DIR}/hippocamp_pipewire_helper.lock" 2>/dev/null || true
rm -f "${RUNTIME_DIR}/realtime-noise.sock" 2>/dev/null || true
rm -f /tmp/realtime-noise-helper.log 2>/dev/null || true
rm -f /tmp/realtime-noise-service.log 2>/dev/null || true
rm -f /tmp/clearcore-*.log 2>/dev/null || true

# 6. Remover arquivos instalados (Sistema ou Usuário)
echo "📁 Removendo arquivos de instalação, atalhos e ícones..."
if [[ $EUID -eq 0 ]]; then
    # Instalação em nível de sistema
    rm -rf "/opt/clearcore" 2>/dev/null || true
    rm -f "/usr/local/bin/clearcore" 2>/dev/null || true
    rm -f "/usr/share/applications/clearcore.desktop" 2>/dev/null || true
    find "/usr/share/icons/hicolor" -path '*/apps/clearcore.png' -delete 2>/dev/null || true
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "/usr/share/applications" 2>/dev/null || true
    fi
else
    # Instalação em nível de usuário
    rm -rf "${HOME}/.local/share/clearcore" 2>/dev/null || true
    rm -f "${HOME}/.local/bin/clearcore" 2>/dev/null || true
    rm -f "${HOME}/.local/share/applications/clearcore.desktop" 2>/dev/null || true
    find "${HOME}/.local/share/icons/hicolor" -path '*/apps/clearcore.png' -delete 2>/dev/null || true
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "${HOME}/.local/share/applications" 2>/dev/null || true
    fi
fi

echo ""
echo "=========================================================="
echo "✅ Clearcore foi completamente desinstalado do Linux!"
echo "   - Processos encerrados"
echo "   - Serviços e autostart desativados"
echo "   - Microfone virtual removido do PipeWire"
echo "   - Binários, atalhos e arquivos temporários removidos"
echo "=========================================================="
