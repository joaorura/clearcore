#!/usr/bin/env bash
set -euo pipefail

export PATH="${HOME}/.cargo/bin:${PATH}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=========================================================="
echo " Clearcore Desktop - Starting Development Environment"
echo "=========================================================="

echo "🔧 Compilando o daemon (realtime-noise-service, debug)..."
if ! (cd "${SCRIPT_DIR}" && cargo build -p realtime-noise-service); then
    echo "❌ Falha ao compilar realtime-noise-service; abortando." >&2
    exit 1
fi

# Em dev, este script é dono do daemon: o app usa o binário debug recém-compilado
# e substitui qualquer daemon anterior.
export CLEARCORE_DAEMON_BIN="${SCRIPT_DIR}/target/debug/realtime-noise-service"
export CLEARCORE_DEV_OWN_DAEMON=1

# Rede de segurança (crash do Electron): encerra só o daemon deste worktree.
# O padrão é regex ancorado, com o caminho escapado.
# Limitação: o npm roda em primeiro plano (sem exec, para o trap rodar; em
# background o Ctrl+C deixaria de funcionar). Para encerrar: Ctrl+C no
# terminal; `kill` só no PID do bash não propaga ao npm — use `kill -- -<PGID>`.
cleanup_daemon() {
    local escaped
    escaped="$(printf '%s' "${SCRIPT_DIR}" | sed 's/[][\.*^$+?(){}|]/\\&/g')"
    pkill -f -- "^${escaped}/target/debug/realtime-noise-service --run" || true
}
trap cleanup_daemon EXIT INT TERM

cd "${SCRIPT_DIR}/crates/app-tauri"
if [[ ! -f "node_modules/.bin/electron" ]]; then
    echo "📦 Dependencies missing. Running npm install..."
    npm install
fi
npm run dev
