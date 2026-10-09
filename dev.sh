#!/usr/bin/env bash
set -euo pipefail

export PATH="${HOME}/.cargo/bin:${PATH}"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "=========================================================="
echo " Clearcore Desktop - Starting Development Environment"
echo "=========================================================="

# O daemon roda o denoiser em tempo real e no cadastro de voz: sem otimização (debug)
# ele não acompanha o áudio. Compila em release (incremental; a 1ª vez demora mais).
echo "🔧 Compilando o daemon (realtime-noise-service, release)..."
if ! (cd "${SCRIPT_DIR}" && cargo build --release -p realtime-noise-service); then
    echo "❌ Falha ao compilar realtime-noise-service; abortando." >&2
    exit 1
fi

# Em dev, este script é dono do daemon: o app usa o binário release recém-compilado
# e substitui qualquer daemon anterior.
export CLEARCORE_DAEMON_BIN="${SCRIPT_DIR}/target/release/realtime-noise-service"
export CLEARCORE_DEV_OWN_DAEMON=1

# Modelo de cadastro de voz e modelos de inferência nativos embutidos no Clearcore.
echo "🎙️  Utilizando modelos nativos embutidos: models/enrollment/ e models/stateful/"

export CLEARCORE_DEV_PDFNET3_ASSET="${CLEARCORE_DEV_PDFNET3_ASSET:-/home/joaorura/orca/projects/clearcore-train/runs/m3_deploy_pro_v2_20261009/pdfnet3-release-asset-v1.tar.gz}"
export CLEARCORE_DEV_PDFNET3_SHA256="${CLEARCORE_DEV_PDFNET3_SHA256:-a3db32ae85a1c9dc81d97a548cc1d5c0c411186ff95fab8591d91d553453bdb2}"

export CLEARCORE_DEV_ENROLLMENT_ASSET="${CLEARCORE_DEV_ENROLLMENT_ASSET:-/home/joaorura/orca/projects/clearcore-train/runs/m3_deploy_pro_v2_20261009/voice-enrollment-asset-v1.tar.gz}"
export CLEARCORE_DEV_ENROLLMENT_SHA256="${CLEARCORE_DEV_ENROLLMENT_SHA256:-d1d4c9d113db12ef4dd3266d2a46960e954c4a45a52943782409f04d4489c581}"

# Core dump desligado neste shell e em tudo que ele inicia (Electron e o daemon, que
# segura PCM cru em memória durante o cadastro de voz). O Electron também aplica
# RLIMIT_CORE=0 ao iniciar o daemon.
ulimit -c 0

# Rede de segurança (crash do Electron): encerra só o daemon deste worktree.
# O padrão é regex ancorado, com o caminho escapado.
# Limitação: o npm roda em primeiro plano (sem exec, para o trap rodar; em
# background o Ctrl+C deixaria de funcionar). Para encerrar: Ctrl+C no
# terminal; `kill` só no PID do bash não propaga ao npm — use `kill -- -<PGID>`.
cleanup_daemon() {
    local escaped
    escaped="$(printf '%s' "${SCRIPT_DIR}" | sed 's/[][\.*^$+?(){}|]/\\&/g')"
    pkill -f -- "^${escaped}/target/release/realtime-noise-service --run" || true
}
trap cleanup_daemon EXIT INT TERM

cd "${SCRIPT_DIR}/crates/app-tauri"
if [[ ! -f "node_modules/.bin/electron" ]]; then
    echo "📦 Dependencies missing. Running npm install..."
    npm install
fi
npm run dev
