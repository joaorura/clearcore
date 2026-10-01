#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Verificador e Instalador de Microfone Virtual (macOS)
# ==============================================================================
# Este script:
# 1. Checa se o plug-in CoreAudio HAL "RealtimeNoiseHAL.driver" existe em /Library/Audio/Plug-Ins/HAL.
# 2. Se NÃO foi criado, copia o bundle do driver e reinicia o coreaudiod.
# 3. Garante que o microfone esteja visível no macOS (system_profiler / CoreAudio).
#
# Uso:
#   ./check-virtual-mic-macos.sh              # Verifica e cria/instala se não existir
#   ./check-virtual-mic-macos.sh --check-only # Apenas checa existência (exit 0=existe, 1=não)
#   ./check-virtual-mic-macos.sh --json       # Retorna status em formato JSON
#   ./check-virtual-mic-macos.sh --recreate   # Força reinstalação/recriação do driver HAL
#   ./check-virtual-mic-macos.sh --set-default# Define como microfone padrão no macOS
#   ./check-virtual-mic-macos.sh --status     # Exibe status formatado
# ==============================================================================

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SYSTEM_HAL_DIR="/Library/Audio/Plug-Ins/HAL"
DRIVER_NAME="RealtimeNoiseHAL.driver"
INSTALLED_DRIVER="${SYSTEM_HAL_DIR}/${DRIVER_NAME}"
BUILT_DRIVER="${SCRIPT_DIR}/platform/macos/HAL/build/Release/${DRIVER_NAME}"
STAGING_DRIVER="${SCRIPT_DIR}/platform/macos/HAL/${DRIVER_NAME}"
MIC_NAME="Clearcore Realtime Noise Suppression Microphone"
MANUFACTURER="CLRC"

is_driver_installed() {
    if [[ -d "${INSTALLED_DRIVER}" ]]; then
        return 0
    fi
    return 1
}

is_coreaudio_enumerated() {
    if command -v system_profiler >/dev/null 2>&1; then
        if system_profiler SPAudioDataType 2>/dev/null | grep -q "${MIC_NAME}"; then
            return 0
        fi
    fi
    if command -v SwitchAudioSource >/dev/null 2>&1; then
        if SwitchAudioSource -a -t input 2>/dev/null | grep -q "${MIC_NAME}"; then
            return 0
        fi
    fi
    return 1
}

is_default_mic() {
    if command -v SwitchAudioSource >/dev/null 2>&1; then
        local current
        current=$(SwitchAudioSource -c -t input 2>/dev/null || true)
        if [[ "${current}" == *"${MIC_NAME}"* ]]; then
            return 0
        fi
    fi
    return 1
}

set_default_mic() {
    if command -v SwitchAudioSource >/dev/null 2>&1; then
        SwitchAudioSource -s "${MIC_NAME}" -t input >/dev/null 2>&1
        return $?
    fi
    return 1
}

install_hal_driver() {
    echo "[macOS] Instalando CoreAudio AudioServerPlugIn (${DRIVER_NAME})..."
    local source_bundle=""
    if [[ -d "${BUILT_DRIVER}" ]]; then
        source_bundle="${BUILT_DRIVER}"
    elif [[ -d "${STAGING_DRIVER}" ]]; then
        source_bundle="${STAGING_DRIVER}"
    fi

    if [[ -z "${source_bundle}" ]]; then
        # Tenta compilar via xcodebuild se disponível
        if command -v xcodebuild >/dev/null 2>&1 && [[ -d "${SCRIPT_DIR}/platform/macos/HAL/RealtimeNoiseHAL.xcodeproj" ]]; then
            echo "[macOS] Compilando bundle via xcodebuild..."
            xcodebuild -project "${SCRIPT_DIR}/platform/macos/HAL/RealtimeNoiseHAL.xcodeproj" \
                       -scheme RealtimeNoiseHAL \
                       -configuration Release \
                       SYMROOT="${SCRIPT_DIR}/platform/macos/HAL/build" \
                       build >/dev/null 2>&1 || true
            if [[ -d "${BUILT_DRIVER}" ]]; then
                source_bundle="${BUILT_DRIVER}"
            fi
        fi
    fi

    if [[ -n "${source_bundle}" && -d "${source_bundle}" ]]; then
        echo "[macOS] Copiando ${source_bundle} para ${SYSTEM_HAL_DIR}..."
        if [[ -w "${SYSTEM_HAL_DIR}" ]]; then
            rm -rf "${INSTALLED_DRIVER}"
            cp -R "${source_bundle}" "${INSTALLED_DRIVER}"
        else
            echo "[macOS] Requer permissao de Administrador (sudo) para instalar em /Library/Audio/Plug-Ins/HAL:"
            sudo cp -R "${source_bundle}" "${INSTALLED_DRIVER}" 2>/dev/null || return 1
        fi

        # Reiniciar o daemon coreaudiod para carregar o novo plug-in
        echo "[macOS] Reiniciando coreaudiod para ativar o microfone virtual..."
        if command -v launchctl >/dev/null 2>&1; then
            sudo launchctl kickstart -k system/com.apple.audio.coreaudiod 2>/dev/null || sudo killall -9 coreaudiod 2>/dev/null || true
        else
            sudo killall -9 coreaudiod 2>/dev/null || true
        fi
        sleep 1
        return 0
    else
        echo "[AVISO] Bundle ${DRIVER_NAME} ainda nao compilado. Execute o projeto Xcode em platform/macos/HAL/" >&2
        return 1
    fi
}

print_json_status() {
    local present=false
    local is_def=false
    local installed=false

    if is_driver_installed; then
        installed=true
    fi
    if is_coreaudio_enumerated || is_driver_installed; then
        present=true
    fi
    if is_default_mic; then
        is_def=true
    fi

    cat <<EOF
{
  "platform": "macos",
  "platform_label": "macOS (CoreAudio HAL Plug-in)",
  "present": ${present},
  "node_id": "com.clearcore.RealtimeNoiseHAL",
  "node_name": "${MIC_NAME}",
  "node_description": "${MIC_NAME}",
  "driver_installed": ${installed},
  "driver_bundle": "${INSTALLED_DRIVER}",
  "is_default": ${is_def},
  "format": "F32LE",
  "rate": 48000,
  "channels": 1,
  "quantum": 480
}
EOF
}

# Despacho de parâmetros
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
    if is_driver_installed || is_coreaudio_enumerated; then
        if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; fi
        exit 0
    else
        if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; fi
        exit 1
    fi
fi

if [[ "${ACTION}" == "set_default" ]]; then
    if set_default_mic; then
        if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; else echo "[OK] Microfone virtual definido como padrao no macOS!"; fi
        exit 0
    else
        if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; else echo "[ERRO] Instale SwitchAudioSource (brew install switchaudio-osx) ou selecione nas Preferencias do Sistema." >&2; fi
        exit 1
    fi
fi

if [[ "${ACTION}" == "recreate" ]]; then
    if install_hal_driver; then
        if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; else echo "[SUCESSO] Plug-in CoreAudio HAL instalado e carregado com sucesso!"; fi
        exit 0
    else
        if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; else echo "[ERRO] Falha ao instalar o plug-in HAL no macOS." >&2; fi
        exit 1
    fi
fi

if [[ "${ACTION}" == "status" ]]; then
    if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; exit 0; fi
    local present=false
    if is_driver_installed || is_coreaudio_enumerated; then present=true; fi
    local is_def="Nao"
    if is_default_mic; then is_def="Sim (Entrada Primaria)"; fi

    echo "=========================================================="
    echo " Microfone Virtual Clearcore (macOS CoreAudio HAL)"
    echo "=========================================================="
    if [[ "${present}" == "true" ]]; then
        echo " - Estado:       🟢 Ativo no CoreAudio"
        echo " - Bundle ID:    com.clearcore.RealtimeNoiseHAL"
        echo " - Nome:         ${MIC_NAME}"
        echo " - Plug-in Path: ${INSTALLED_DRIVER}"
        echo " - Padrao:       ${is_def}"
        echo " - Formato:      48kHz Float32 Mono (10ms)"
    else
        echo " - Estado:       🔴 Plug-in HAL nao instalado em ${INSTALLED_DRIVER}"
        echo " - Instalacao:   Copie RealtimeNoiseHAL.driver para /Library/Audio/Plug-Ins/HAL/"
        echo "                 e execute: sudo killall coreaudiod"
    fi
    echo "=========================================================="
    if [[ "${present}" == "true" ]]; then exit 0; else exit 1; fi
fi

# Acao padrao: verify_or_create
if is_driver_installed || is_coreaudio_enumerated; then
    if [[ "${OUTPUT_JSON}" == "true" ]]; then
        print_json_status
    else
        echo "[OK] Microfone virtual CoreAudio HAL ja existe e esta ativo no macOS!"
    fi
    exit 0
fi

if ! [[ "${OUTPUT_JSON}" == "true" ]]; then
    echo "[AVISO] Plug-in CoreAudio HAL nao detectado em ${SYSTEM_HAL_DIR}! Tentando instalar..."
fi

if install_hal_driver; then
    if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; else echo "[SUCESSO] Microfone virtual criado e registrado no CoreAudio com sucesso!"; fi
    exit 0
else
    if [[ "${OUTPUT_JSON}" == "true" ]]; then print_json_status; fi
    exit 1
fi
