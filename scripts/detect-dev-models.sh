#!/usr/bin/env bash
# scripts/detect-dev-models.sh
# Detecta e configura automaticamente o modelo treinado no M3
# para cadastro de voz (voice-enrollment).
# Se as variáveis de ambiente já estiverem definidas, respeita os valores existentes.

detect_and_export_dev_models() {
    local current_script_dir
    current_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local repo_root
    repo_root="$(cd "${current_script_dir}/.." && pwd)"

    # Se o modelo oficial nativo já existe no repositório, nenhuma ação externa é necessária
    if [[ -f "${repo_root}/models/enrollment/enrollment.onnx" ]]; then
        return 0
    fi

    local candidate_dirs=()

    if [[ -n "${CLEARCORE_TRAIN_M3_DIR:-}" ]]; then
        candidate_dirs+=("${CLEARCORE_TRAIN_M3_DIR}")
    fi

    local current_script_dir
    current_script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    local repo_root
    repo_root="$(cd "${current_script_dir}/.." && pwd)"

    candidate_dirs+=(
        "${repo_root}/../../../projects/clearcore-train/runs/m3"
        "${HOME}/orca/projects/clearcore-train/runs/m3"
    )

    local m3_dir=""
    for d in "${candidate_dirs[@]}"; do
        if [[ -d "${d}" ]]; then
            m3_dir="$(cd "${d}" && pwd)"
            break
        fi
    done

    if [[ -z "${m3_dir}" ]]; then
        return 0
    fi

    # 1. Modelo de cadastro de voz (voice-enrollment)
    if [[ -z "${CLEARCORE_DEV_ENROLLMENT_ASSET:-}" || -z "${CLEARCORE_DEV_ENROLLMENT_SHA256:-}" ]]; then
        local enroll_file="${m3_dir}/voice-enrollment-asset-v1.tar.gz"
        if [[ -f "${enroll_file}" ]]; then
            local enroll_sha
            enroll_sha="$(sha256sum "${enroll_file}" | cut -d' ' -f1)"
            export CLEARCORE_DEV_ENROLLMENT_ASSET="${enroll_file}"
            export CLEARCORE_DEV_ENROLLMENT_SHA256="${enroll_sha}"
            echo "🧪 [Auto-detect] Modelo de cadastro de voz detectado: ${enroll_file}"
        fi
    fi
}

detect_and_export_dev_models
