#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Configurador de Autostart (Inicio Automatico na Bandeja)
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${SCRIPT_DIR}/crates/app-tauri"
AUTOSTART_DIR="${HOME}/.config/autostart"
DESKTOP_FILE="${AUTOSTART_DIR}/realtime-noise.desktop"

COMMAND="${1:-status}"

case "${COMMAND}" in
    enable|--enable)
        mkdir -p "${AUTOSTART_DIR}"
        cat > "${DESKTOP_FILE}" << EOF
[Desktop Entry]
Type=Application
Name=Clearcore Realtime Noise Suppression
Comment=Audio Noise Suppression Virtual Microphone (Tray Companion)
Exec=/usr/bin/npm start --prefix ${APP_DIR} -- --tray
Icon=${APP_DIR}/assets/icon.png
Terminal=false
Categories=AudioVideo;Audio;
X-GNOME-Autostart-enabled=true
EOF
        chmod 755 "${DESKTOP_FILE}"
        echo "[OK] Inicialização automática ativada com sucesso!"
        echo "     O Clearcore iniciará minimizado na bandeja do sistema ao fazer login."
        echo "     Arquivo: ${DESKTOP_FILE}"
        ;;
    disable|--disable)
        if [[ -f "${DESKTOP_FILE}" ]]; then
            rm -f "${DESKTOP_FILE}"
            echo "[OK] Inicialização automática desativada."
        else
            echo "[INFO] Inicialização automática já estava desativada."
        fi
        ;;
    status|--status)
        if [[ -f "${DESKTOP_FILE}" ]]; then
            echo "[ATIVADO] Clearcore está configurado para iniciar automaticamente na bandeja."
            echo "          Arquivo: ${DESKTOP_FILE}"
        else
            echo "[DESATIVADO] Inicialização automática não está configurada."
            echo "             Para ativar, execute: ./scripts/setup-autostart.sh enable"
        fi
        ;;
    *)
        echo "Uso: $0 [enable|disable|status]"
        exit 1
        ;;
esac
