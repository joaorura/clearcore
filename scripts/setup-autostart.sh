#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Configurador de Autostart (Inicio Automatico na Bandeja)
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_DIR="${SCRIPT_DIR}/crates/app-tauri"
AUTOSTART_DIR="${HOME}/.config/autostart"
DESKTOP_FILE="${AUTOSTART_DIR}/clearcore.desktop"
LEGACY_DESKTOP_FILE="${AUTOSTART_DIR}/realtime-noise.desktop"

COMMAND="${1:-status}"

# Discover clearcore executable
CLEARCORE_BIN=""
if command -v clearcore >/dev/null 2>&1; then
    CLEARCORE_BIN="$(command -v clearcore)"
elif [[ -x "${HOME}/.local/bin/clearcore" ]]; then
    CLEARCORE_BIN="${HOME}/.local/bin/clearcore"
elif [[ -x "/usr/local/bin/clearcore" ]]; then
    CLEARCORE_BIN="/usr/local/bin/clearcore"
elif [[ -x "/opt/clearcore/clearcore" ]]; then
    CLEARCORE_BIN="/opt/clearcore/clearcore"
elif [[ -x "${SCRIPT_DIR}/release/Clearcore-linux-x64/clearcore" ]]; then
    CLEARCORE_BIN="${SCRIPT_DIR}/release/Clearcore-linux-x64/clearcore"
fi

if [[ -n "${CLEARCORE_BIN}" ]]; then
    EXEC_CMD="${CLEARCORE_BIN} --tray"
    ICON_PATH="clearcore"
elif command -v npm >/dev/null 2>&1 && [[ -d "${APP_DIR}" ]]; then
    EXEC_CMD="/usr/bin/npm run start:tray --prefix ${APP_DIR}"
    ICON_PATH="${APP_DIR}/assets/icon.png"
else
    EXEC_CMD="clearcore --tray"
    ICON_PATH="clearcore"
fi

case "${COMMAND}" in
    enable|--enable)
        mkdir -p "${AUTOSTART_DIR}"
        cat > "${DESKTOP_FILE}" << EOF
[Desktop Entry]
Type=Application
Name=Clearcore
Comment=Clearcore Realtime AI Noise Suppression (Tray Companion)
Exec=${EXEC_CMD}
Icon=${ICON_PATH}
Terminal=false
Categories=AudioVideo;Audio;
X-GNOME-Autostart-enabled=true
EOF
        chmod 755 "${DESKTOP_FILE}"
        rm -f "${LEGACY_DESKTOP_FILE}" 2>/dev/null || true
        echo "[OK] Inicialização automática ativada com sucesso!"
        echo "     O Clearcore iniciará minimizado na bandeja do sistema ao fazer login."
        echo "     Arquivo: ${DESKTOP_FILE}"
        echo "     Comando: ${EXEC_CMD}"
        ;;
    disable|--disable)
        REMOVED=0
        if [[ -f "${DESKTOP_FILE}" ]]; then
            rm -f "${DESKTOP_FILE}"
            REMOVED=1
        fi
        if [[ -f "${LEGACY_DESKTOP_FILE}" ]]; then
            rm -f "${LEGACY_DESKTOP_FILE}"
            REMOVED=1
        fi
        if [[ ${REMOVED} -eq 1 ]]; then
            echo "[OK] Inicialização automática desativada."
        else
            echo "[INFO] Inicialização automática já estava desativada."
        fi
        ;;
    status|--status)
        if [[ -f "${DESKTOP_FILE}" || -f "${LEGACY_DESKTOP_FILE}" ]]; then
            ACTIVE_FILE="${DESKTOP_FILE}"
            [[ -f "${ACTIVE_FILE}" ]] || ACTIVE_FILE="${LEGACY_DESKTOP_FILE}"
            echo "[ATIVADO] Clearcore está configurado para iniciar automaticamente na bandeja."
            echo "          Arquivo: ${ACTIVE_FILE}"
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
