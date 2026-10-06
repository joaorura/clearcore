#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PATH="${HOME}/.cargo/bin:${HOME}/.local/bin:${PATH}"

# Auto-detecta modelos treinados no M3 se disponíveis localmente
if [[ -f "${SCRIPT_DIR}/scripts/detect-dev-models.sh" ]]; then
    # shellcheck source=scripts/detect-dev-models.sh
    source "${SCRIPT_DIR}/scripts/detect-dev-models.sh"
fi

BIN_HELPER="${SCRIPT_DIR}/platform/linux/helper/build/pipewire_helper"
BIN_CLI="${SCRIPT_DIR}/target/release/realtime-noise-app-tauri"

echo "=========================================================="
echo " Iniciando ClearCore (Daemon + App UI)"
echo "=========================================================="

# 1. Compilar binários automaticamente se faltarem
if [[ ! -x "${BIN_HELPER}" ]]; then
    echo "[1/4] Compilando helper C do PipeWire..."
    ninja -C "${SCRIPT_DIR}/platform/linux/helper/build"
fi

# Sempre compila (incremental, rapido quando nada mudou): um binario existente pode estar
# desatualizado em relacao ao codigo. O start-realtime-noise.sh reinicia um daemon antigo.
echo "[2/4] Compilando binarios Rust (release, incremental)..."
cargo build --release -p realtime-noise-service -p realtime-noise-app-tauri

# 2. Iniciar o daemon de supressao de ruido
echo "[3/4] Iniciando daemon de audio PipeWire..."
"${SCRIPT_DIR}/start-realtime-noise.sh"

# 3. Validar comunicacao IPC
sleep 0.5
if [[ -x "${BIN_CLI}" ]]; then
    echo "Status do servico (IPC):"
    "${BIN_CLI}" --status || true
fi

# 4. Iniciar a interface Electron na Bandeja do Sistema
echo "[4/4] Iniciando Desktop Companion (Electron Tray)..."
if [[ ! -f "${SCRIPT_DIR}/crates/app-tauri/dist/index.html" ]]; then
    echo "Compilando frontend React para producao..."
    (cd "${SCRIPT_DIR}/crates/app-tauri" && npm run build >/dev/null 2>&1)
fi

if pgrep -f "electron.*main.cjs" >/dev/null 2>&1; then
    echo "Desktop Companion Electron ja esta em execucao na bandeja."
elif command -v systemd-run >/dev/null 2>&1; then
    systemctl --user reset-failed realtime-noise-electron 2>/dev/null || true
    systemd-run --user --unit=realtime-noise-electron \
        --working-directory="${SCRIPT_DIR}/crates/app-tauri" \
        /usr/bin/npm start >/dev/null 2>&1 || true
    sleep 1
else
    (cd "${SCRIPT_DIR}/crates/app-tauri" && nohup npm start > /tmp/realtime-noise-electron.log 2>&1 & disown $!)
fi

echo "=========================================================="
echo " Tudo pronto e em execucao!"
echo " - Daemon Audio:       Ativo no PipeWire / WirePlumber"
echo " - Microfone Virtual:  Ativo (Node: realtime-noise-source / 48kHz Mono)"
echo " - Socket IPC:         /run/user/$(id -u)/realtime-noise.sock"
echo " - Bandeja do Sistema: Icone ativo (Electron Tray Companion)"
echo " - Auto-inicializacao: ./scripts/setup-autostart.sh status"
echo " - Checagem Microfone: ./scripts/check-virtual-mic.sh --status"
echo " - Para parar tudo:    ./stop-all.sh"
echo "=========================================================="
