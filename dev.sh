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

# Modelo de cadastro de voz (só desenvolvimento). O daemon é iniciado pelo Electron e
# herda estas variáveis do ambiente deste script; sem elas, "Gerar perfil" termina em
# ENROLL_MODEL_NOT_CONFIGURED (as amostras continuam sendo gravadas e denoisadas).
if [[ -n "${CLEARCORE_DEV_ENROLLMENT_ASSET:-}" && -n "${CLEARCORE_DEV_ENROLLMENT_SHA256:-}" ]]; then
    export CLEARCORE_DEV_ENROLLMENT_ASSET CLEARCORE_DEV_ENROLLMENT_SHA256
    echo "🧪 Modelo de cadastro (dev): ${CLEARCORE_DEV_ENROLLMENT_ASSET}"
else
    echo "ℹ️  CLEARCORE_DEV_ENROLLMENT_ASSET/_SHA256 não definidas: gerar perfil ficará ENROLL_MODEL_NOT_CONFIGURED."
    echo "    Veja a seção de desenvolvimento do AGENTS.md para defini-las."
fi

# Modelo de isolamento pDFNet3 com FiLM (só desenvolvimento; checkpoint NO-GO do M2). Com as
# duas variáveis o daemon força o backend tract com esse modelo e o perfil de voz é aplicado;
# sem elas usa o modelo base (que não aplica perfil). Uma só é erro reportado no GetStatus.
if [[ -n "${CLEARCORE_DEV_PDFNET3_ASSET:-}" && -n "${CLEARCORE_DEV_PDFNET3_SHA256:-}" ]]; then
    export CLEARCORE_DEV_PDFNET3_ASSET CLEARCORE_DEV_PDFNET3_SHA256
    echo "🧪 Modelo de isolamento (dev, pDFNet3 NO-GO do M2): ${CLEARCORE_DEV_PDFNET3_ASSET}"
elif [[ -n "${CLEARCORE_DEV_PDFNET3_ASSET:-}" || -n "${CLEARCORE_DEV_PDFNET3_SHA256:-}" ]]; then
    export CLEARCORE_DEV_PDFNET3_ASSET CLEARCORE_DEV_PDFNET3_SHA256
    echo "⚠️  Só uma de CLEARCORE_DEV_PDFNET3_ASSET/_SHA256 definida: o daemon usará o modelo base (DEV_MODEL_CONFIG_INCOMPLETE)."
else
    echo "ℹ️  CLEARCORE_DEV_PDFNET3_ASSET/_SHA256 não definidas: modelo base, o perfil de voz não será aplicado."
    echo "    Veja a seção de desenvolvimento do AGENTS.md para defini-las."
fi

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
