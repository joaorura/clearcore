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
# Localizacao do helper, primeiro EXECUTAVEL vence:
#   1. CLEARCORE_HELPER_BIN        (override explicito)
#   2. <raiz>/bin/pipewire_helper  (layout do pacote: resources/scripts/ + resources/bin/)
#   3. <raiz>/platform/linux/helper/build/pipewire_helper (desenvolvimento)
# Nenhum existe: fica o caminho de desenvolvimento (mantem mensagens e o ramo de compilacao).
HELPER_BIN="${SCRIPT_DIR}/platform/linux/helper/build/pipewire_helper"
for _helper_candidate in \
    "${CLEARCORE_HELPER_BIN:-}" \
    "${SCRIPT_DIR}/bin/pipewire_helper" \
    "${SCRIPT_DIR}/platform/linux/helper/build/pipewire_helper"; do
    if [[ -n "${_helper_candidate}" && -x "${_helper_candidate}" && ! -d "${_helper_candidate}" ]]; then
        HELPER_BIN="${_helper_candidate}"
        break
    fi
done
unset _helper_candidate
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

# Liga TODAS as portas de um nó fonte (por node.name) às entradas do stream de captura.
# Estéreo: FL(_1) -> entrada esquerda (input_FL, senão input_MONO) e FR(_2) -> input_FR.
# Mono (ex.: headset Bluetooth, única porta capture_MONO): a porta alimenta TODAS as entradas.
link_source_node_to_capture() {
    local node="$1" p ch
    local -a src_ports=() in_ports=()
    while IFS= read -r p; do
        [[ "${p}" == "${node}:"* ]] && src_ports+=("${p}")
    done < <(pw-link -o 2>/dev/null | sed 's/^[[:space:]]*//')
    while IFS= read -r p; do
        [[ "${p}" == realtime-noise-capture:input_* ]] && in_ports+=("${p}")
    done < <(pw-link -i 2>/dev/null | sed 's/^[[:space:]]*//')
    [[ ${#src_ports[@]} -gt 0 && ${#in_ports[@]} -gt 0 ]] || return 0

    if [[ ${#src_ports[@]} -eq 1 ]]; then
        for p in "${in_ports[@]}"; do
            pw-link "${src_ports[0]}" "${p}" >/dev/null 2>&1 || true
        done
        return 0
    fi

    local in_left="${in_ports[0]}" in_right=""
    for p in "${in_ports[@]}"; do
        [[ "${p}" == "realtime-noise-capture:input_FR" ]] && in_right="${p}"
        [[ "${p}" == "realtime-noise-capture:input_FL" ]] && in_left="${p}"
    done
    for p in "${src_ports[@]}"; do
        ch="${p##*_}"
        if [[ "${ch}" == "FL" || "${ch}" == "1" ]]; then
            pw-link "${p}" "${in_left}" >/dev/null 2>&1 || true
        elif [[ ( "${ch}" == "FR" || "${ch}" == "2" ) && -n "${in_right}" ]]; then
            pw-link "${p}" "${in_right}" >/dev/null 2>&1 || true
        fi
    done
}

# Garante a ligação microfone físico -> stream de captura.
# $1 = node.name do microfone ESCOLHIDO (o mesmo que o helper recebeu em --target).
# Só esse nó é ligado: nunca "o primeiro nó cuja porta casa" (webcam v4l2 capture_1,
# Bluetooth capture_MONO etc. também casam e seriam somados ao mic escolhido). Sem nome
# conhecido não se liga nada às cegas; o AUTOCONNECT do WirePlumber decide.
ensure_capture_link() {
    local phys_name="${1:-}"
    if command -v pw-link >/dev/null 2>&1; then
        local p
        # As portas de entrada seguem o formato negociado (input_FL/input_FR ou input_MONO)
        # Sever self-loop if present
        while IFS= read -r p; do
            [[ "${p}" == realtime-noise-capture:input_* ]] || continue
            pw-link -d "${NODE_NAME}:capture_MONO" "${p}" >/dev/null 2>&1 || true
        done < <(pw-link -i 2>/dev/null | sed 's/^[[:space:]]*//')
        # Garantir link saudável entre microfone físico e stream de captura
        if ! pw-link -l 2>/dev/null | awk -v n="${NODE_NAME}:" '
                /^[^ \t]/ { cur = $0 }
                /\|<-/ && cur ~ /^realtime-noise-capture:input_/ && index($0, n) == 0 { found = 1 }
                END { exit !found }'; then
            if [[ -n "${phys_name}" && "${phys_name}" != "${NODE_NAME}" ]]; then
                link_source_node_to_capture "${phys_name}"
            else
                echo "[check-virtual-mic] microfone escolhido desconhecido: nao ligo nenhuma fonte as cegas (WirePlumber decide)" >&2
            fi
        fi
    fi
}

# node.name de um nó a partir do id (aceita também um nome, devolvido como está).
node_name_from_id() {
    local ref="$1" name=""
    [[ -n "${ref}" ]] || return 0
    if [[ ! "${ref}" =~ ^[0-9]+$ ]]; then
        printf '%s\n' "${ref}"
        return 0
    fi
    if command -v wpctl >/dev/null 2>&1; then
        name=$(wpctl inspect "${ref}" 2>/dev/null | sed -n 's/^[[:space:]*]*node\.name = "\(.*\)"$/\1/p' | head -n1)
    fi
    if [[ -z "${name}" ]] && command -v pw-cli >/dev/null 2>&1; then
        name=$(pw-cli info "${ref}" 2>/dev/null | sed -n 's/^[[:space:]*]*node\.name = "\(.*\)"$/\1/p' | head -n1)
    fi
    printf '%s\n' "${name}"
}

# node.name do primeiro microfone físico (Audio/Source que não seja o virtual).
physical_source_name() {
    local phys_id="" name=""
    if command -v wpctl >/dev/null 2>&1; then
        phys_id=$(wpctl status 2>/dev/null | awk '/Sources:/,/Filters:|Streams:/' | grep -v 'Realtime Noise' | grep -E '[0-9]+\.' | head -n1 | grep -o -E '[0-9]+' | head -n1)
        name=$(node_name_from_id "${phys_id}")
    fi
    if [[ -z "${name}" ]] && command -v pw-cli >/dev/null 2>&1; then
        name=$(pw-cli list-objects Node 2>/dev/null | awk '
            function flush() {
                if (is_source && name != "" && name !~ /realtime-noise/ && !done) { print name; done = 1 }
                is_source = 0; name = ""
            }
            $1 == "id" { flush() }
            $0 ~ "media.class = \"Audio/Source\"" { is_source = 1 }
            $1 == "node.name" { name = $3; gsub(/"/, "", name) }
            END { flush() }
        ')
    fi
    printf '%s\n' "${name}"
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
        sleep 0.2
        ensure_capture_link "$(physical_source_name)"
        return 0
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
    systemctl --user reset-failed realtime-noise-helper.service >/dev/null 2>&1 || true
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
                "${SCRIPT_DIR}/platform/linux/helper/src/noise_suppressor.c" \
                $(pkg-config --cflags --libs libpipewire-0.3 2>/dev/null || echo "-I/home/joaorura/.local/usr/include/pipewire-0.3 -I/home/joaorura/.local/usr/include/spa-0.2 -L/home/joaorura/.local/usr/lib64 -lpipewire-0.3") \
                -lm -ldl -o "${HELPER_BIN}" 2>/dev/null || true
        fi
    fi

    # Detectar microfone físico para passar como alvo explícito
    # O alvo vai por node.name: o PipeWire/WirePlumber lê um target.object numérico como
    # object.serial, não como id de nó, então um id nunca casa (e só sai silêncio).
    local -a target_args=()
    local phys_name=""
    phys_name=$(physical_source_name)
    if [[ -n "${phys_name}" ]]; then
        target_args=(--target "${phys_name}")
    fi

    # Iniciar pipewire_helper via systemd user unit ou nohup
    if command -v systemd-run >/dev/null 2>&1; then
        systemctl --user reset-failed realtime-noise-helper >/dev/null 2>&1 || true
        systemd-run --user --unit=realtime-noise-helper \
            "${HELPER_BIN}" ${target_args[@]+"${target_args[@]}"} >/dev/null 2>&1 || true
    else
        nohup "${HELPER_BIN}" ${target_args[@]+"${target_args[@]}"} > /tmp/realtime-noise-helper.log 2>&1 & disown $!
    fi

    # Polling até 3 segundos
    for _ in {1..15}; do
        sleep 0.2
        if is_node_present; then
            # Garantir link saudável sem autoconexão circular (estéreo e mono),
            # só com o mesmo mic que o helper recebeu em --target
            ensure_capture_link "${phys_name}"
            return 0
        fi
    done

    # Fallback via pw-loopback se o helper demorou
    if command -v pw-loopback >/dev/null 2>&1; then
        if [[ ! -x "${HELPER_BIN}" ]]; then
            echo "[check-virtual-mic] AVISO: pipewire_helper nao encontrado; usando loopback SEM supressao de ruido" >&2
        fi
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
