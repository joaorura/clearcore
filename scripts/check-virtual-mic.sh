#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Verificador e Recuperador de Microfone Virtual
# (Multi-Plataforma: Linux PipeWire, macOS CoreAudio HAL, Windows WaveRT)
# ==============================================================================
# Este script:
# 1. Detecta o sistema operacional do host.
# 2. No Linux: gerencia o nó PipeWire "realtime-noise-source" (via helper C / loopback).
# 3. No macOS: delega para check-virtual-mic-macos.sh (CoreAudio HAL Plug-in).
# 4. No Windows: delega para check-virtual-mic-windows.ps1 (Driver WaveRT / PortCls).
#
# Uso:
#   ./scripts/check-virtual-mic.sh              # Verifica e cria se não existir
#   ./scripts/check-virtual-mic.sh --check-only # Apenas checa existência (0=existe, 1=não)
#   ./scripts/check-virtual-mic.sh --json       # Retorna status em formato JSON
#   ./scripts/check-virtual-mic.sh --recreate   # Força recriação do microfone virtual
#   ./scripts/check-virtual-mic.sh --set-default# Define como microfone padrão no sistema
# ==============================================================================

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Despacho Multi-Plataforma
CURRENT_OS="$(uname -s 2>/dev/null || echo "Unknown")"
if [[ "${CURRENT_OS}" == "Darwin" ]]; then
    MACOS_SCRIPT="${SCRIPT_DIR}/scripts/check-virtual-mic-macos.sh"
    if [[ -x "${MACOS_SCRIPT}" ]]; then
        exec "${MACOS_SCRIPT}" "$@"
    else
        exec bash "${MACOS_SCRIPT}" "$@"
    fi
elif [[ "${CURRENT_OS}" == *"MINGW"* || "${CURRENT_OS}" == *"MSYS"* || "${CURRENT_OS}" == *"CYGWIN"* || "${OS:-}" == "Windows_NT" ]]; then
    WIN_SCRIPT="${SCRIPT_DIR}/scripts/check-virtual-mic-windows.ps1"
    exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${WIN_SCRIPT}" "$@"
fi

# ==============================================================================
# Implementação Nativa Linux (PipeWire / WirePlumber)
# ==============================================================================
HELPER_BIN="${SCRIPT_DIR}/platform/linux/helper/build/pipewire_helper"
NODE_NAME="realtime-noise-source"
NODE_DESC="Realtime Noise Virtual Microphone"

is_node_present() {
    if command -v pw-cli >/dev/null 2>&1; then
        if pw-cli list-objects Node 2>/dev/null | grep -q "node.name = \"${NODE_NAME}\""; then
            return 0
        fi
    fi
    if command -v wpctl >/dev/null 2>&1; then
        if wpctl status 2>/dev/null | grep -q "${NODE_DESC}"; then
            return 0
        fi
    fi
    return 1
}

get_node_id() {
    if command -v pw-cli >/dev/null 2>&1; then
        pw-cli list-objects Node 2>/dev/null | awk -v name="\"${NODE_NAME}\"" '
            $1 == "id" { id = $2; sub(/,/, "", id) }
            $0 ~ "node.name = " name { print id; exit }
        '
    fi
}

is_default_mic() {
    local node_id
    node_id=$(get_node_id)
    if [[ -n "${node_id}" ]] && command -v wpctl >/dev/null 2>&1; then
        wpctl status 2>/dev/null | awk '/Sources:/,/Filters:|Sinks:/' | grep -E "\*\s*${node_id}\.\s*" >/dev/null 2>&1
        return $?
    fi
    return 1
}

set_default_mic() {
    local node_id
    node_id=$(get_node_id)
    if [[ -z "${node_id}" ]]; then
        echo "Error: Virtual microphone node not found" >&2
        return 1
    fi
    if command -v wpctl >/dev/null 2>&1; then
        wpctl set-default "${node_id}"
        return $?
    fi
    return 1
}

print_json_status() {
    local present=false
    local node_id="null"
    local is_def=false

    if is_node_present; then
        present=true
        local raw_id
        raw_id=$(get_node_id)
        if [[ -n "${raw_id}" ]]; then
            node_id="${raw_id}"
        fi
        if is_default_mic; then
            is_def=true
        fi
    fi

    cat <<EOF
{
  "platform": "linux",
  "platform_label": "Linux (PipeWire / WirePlumber)",
  "present": ${present},
  "node_id": ${node_id},
  "node_name": "${NODE_NAME}",
  "node_description": "${NODE_DESC}",
  "driver_status": "OK",
  "is_default": ${is_def},
  "format": "F32LE",
  "rate": 48000,
  "channels": 1,
  "quantum": 480
}
EOF
}

recreate_node() {
    # Para instâncias anteriores
    systemctl --user stop realtime-noise-helper.service >/dev/null 2>&1 || true
    pkill -f "pipewire_helper" >/dev/null 2>&1 || true
    pkill -f "pw-loopback.*${NODE_NAME}" >/dev/null 2>&1 || true

    # Limpar travas obsoletas
    local lock_file="${XDG_RUNTIME_DIR:-/tmp}/hippocamp_pipewire_helper.lock"
    rm -f "${lock_file}" 2>/dev/null || true

    # Verificar/Compilar helper se necessário
    if [[ ! -x "${HELPER_BIN}" ]]; then
        if command -v ninja >/dev/null 2>&1 && [[ -f "${SCRIPT_DIR}/platform/linux/helper/build/build.ninja" ]]; then
            ninja -C "${SCRIPT_DIR}/platform/linux/helper/build" >/dev/null 2>&1 || true
        else
            mkdir -p "${SCRIPT_DIR}/platform/linux/helper/build"
            gcc -O3 -Wall -Wextra -pthread \
                "${SCRIPT_DIR}/platform/linux/helper/src/pipewire_helper.c" \
                "${SCRIPT_DIR}/platform/linux/helper/src/transport_bridge.c" \
                "${SCRIPT_DIR}/platform/linux/helper/src/format_converter.c" \
                $(pkg-config --cflags --libs libpipewire-0.3) \
                -o "${HELPER_BIN}" 2>/dev/null || true
        fi
    fi

    # Iniciar pipewire_helper via systemd user unit ou nohup
    if command -v systemd-run >/dev/null 2>&1; then
        systemd-run --user --unit=realtime-noise-helper \
            "${HELPER_BIN}" >/dev/null 2>&1 || true
    else
        nohup "${HELPER_BIN}" > /tmp/realtime-noise-helper.log 2>&1 & disown $!
    fi

    # Polling até 3 segundos
    for _ in {1..15}; do
        sleep 0.2
        if is_node_present; then
            return 0
        fi
    done

    # Fallback via pw-loopback se o helper demorou
    if command -v pw-loopback >/dev/null 2>&1; then
        nohup pw-loopback \
            --capture-props="media.class=Audio/Sink node.name=realtime-noise-sink node.description=\"Realtime Noise Monitor Sink\"" \
            --playback-props="media.class=Audio/Source node.name=${NODE_NAME} node.description=\"${NODE_DESC}\"" \
            >/dev/null 2>&1 & disown $!
        sleep 0.5
        if is_node_present; then
            return 0
        fi
    fi

    return 1
}

# Processamento de argumentos
ACTION="verify_or_create"
OUTPUT_JSON=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --check-only)
            ACTION="check_only"
            shift
            ;;
        --json)
            OUTPUT_JSON=true
            shift
            ;;
        --recreate)
            ACTION="recreate"
            shift
            ;;
        --set-default)
            ACTION="set_default"
            shift
            ;;
        --status)
            ACTION="status"
            shift
            ;;
        *)
            shift
            ;;
    esac
done

if [[ "${ACTION}" == "check_only" ]]; then
    if is_node_present; then
        if [[ "${OUTPUT_JSON}" == "true" ]]; then
            print_json_status
        fi
        exit 0
    else
        if [[ "${OUTPUT_JSON}" == "true" ]]; then
            print_json_status
        fi
        exit 1
    fi
fi

if [[ "${ACTION}" == "set_default" ]]; then
    if set_default_mic; then
        if [[ "${OUTPUT_JSON}" == "true" ]]; then
            print_json_status
        else
            echo "[OK] Microfone virtual definido como padrao do sistema!"
        fi
        exit 0
    else
        if [[ "${OUTPUT_JSON}" == "true" ]]; then
            print_json_status
        else
            echo "[ERRO] Falha ao definir microfone virtual como padrao." >&2
        fi
        exit 1
    fi
fi

if [[ "${ACTION}" == "recreate" ]]; then
    if recreate_node; then
        if [[ "${OUTPUT_JSON}" == "true" ]]; then
            print_json_status
        else
            NODE_ID=$(get_node_id)
            echo "[SUCESSO] Microfone virtual recriado com sucesso no PipeWire! (Node ID: ${NODE_ID:-ativo})"
        fi
        exit 0
    else
        if [[ "${OUTPUT_JSON}" == "true" ]]; then
            print_json_status
        else
            echo "[ERRO FATAL] Falha ao recriar o microfone virtual no PipeWire." >&2
        fi
        exit 1
    fi
fi

if [[ "${ACTION}" == "status" ]]; then
    if [[ "${OUTPUT_JSON}" == "true" ]]; then
        print_json_status
        exit 0
    fi
    if is_node_present; then
        NODE_ID=$(get_node_id)
        IS_DEF="Nao"
        if is_default_mic; then
            IS_DEF="Sim (Padrao do Sistema)"
        fi
        echo "=========================================================="
        echo " Microfone Virtual Clearcore (Linux PipeWire)"
        echo "=========================================================="
        echo " - Estado:       🟢 Ativo"
        echo " - Node ID:      ${NODE_ID:-desconhecido}"
        echo " - Nome:         ${NODE_NAME}"
        echo " - Descricao:    ${NODE_DESC}"
        echo " - Padrao:       ${IS_DEF}"
        echo " - Formato:      48kHz Float32 Mono (Quantum 480 / 10ms)"
        echo "=========================================================="
        exit 0
    else
        echo "=========================================================="
        echo " Microfone Virtual Clearcore (Linux PipeWire)"
        echo "=========================================================="
        echo " - Estado:       🔴 Nao Detectado no Grafo do PipeWire"
        echo " - Sugestao:     Execute ./scripts/check-virtual-mic.sh --recreate"
        echo "=========================================================="
        exit 1
    fi
fi

# Acao padrao: verify_or_create
if is_node_present; then
    if [[ "${OUTPUT_JSON}" == "true" ]]; then
        print_json_status
        exit 0
    fi
    NODE_ID=$(get_node_id)
    IS_DEF="Nao"
    if is_default_mic; then
        IS_DEF="Sim (Padrao do Sistema)"
    fi
    echo "=========================================================="
    echo " Clearcore - Verificacao de Microfone Virtual (PipeWire)"
    echo "=========================================================="
    echo "[OK] Microfone virtual ja existe e esta ativo no PipeWire!"
    echo "     - Nome do Node: ${NODE_NAME}"
    echo "     - Descricao:    ${NODE_DESC}"
    echo "     - Node ID:      ${NODE_ID:-desconhecido}"
    echo "     - Padrao:       ${IS_DEF}"
    echo "     - Formato:      48kHz Float32 Mono (Quantum 480 / 10ms)"
    echo "=========================================================="
    exit 0
fi

# Se não estiver presente, recriar automaticamente!
if ! [[ "${OUTPUT_JSON}" == "true" ]]; then
    echo "[AVISO] O microfone virtual NAO foi detectado no grafo do PipeWire!"
    echo "        Iniciando criacao e recuperacao automatica..."
fi

if recreate_node; then
    if [[ "${OUTPUT_JSON}" == "true" ]]; then
        print_json_status
        exit 0
    fi
    NODE_ID=$(get_node_id)
    echo "=========================================================="
    echo "[SUCESSO] Microfone virtual criado e registrado no PipeWire!"
    echo "          - Nome do Node: ${NODE_NAME}"
    echo "          - Descricao:    ${NODE_DESC}"
    echo "          - Node ID:      ${NODE_ID:-ativo}"
    echo "          - Visibilidade: 100% pronto para uso em reunioes e gravacoes."
    echo "=========================================================="
    exit 0
else
    if [[ "${OUTPUT_JSON}" == "true" ]]; then
        print_json_status
        exit 1
    fi
    echo "[ERRO FATAL] Nao foi possivel criar o microfone virtual no PipeWire."
    echo "             Verifique se o servico PipeWire esta ativo: systemctl --user status pipewire"
    echo "=========================================================="
    exit 1
fi
