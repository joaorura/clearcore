#!/usr/bin/env bash
# scripts/detect-dev-models.sh
# Detecta e configura automaticamente os modelos treinados no M3
# para cadastro de voz (voice-enrollment) e isolamento acústico (pDFNet3 com FiLM).
# Se as variáveis de ambiente já estiverem definidas, respeita os valores existentes.

detect_and_export_dev_models() {
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

    # 2. Modelo de isolamento pDFNet3 com FiLM
    if [[ -z "${CLEARCORE_DEV_PDFNET3_ASSET:-}" || -z "${CLEARCORE_DEV_PDFNET3_SHA256:-}" ]]; then
        local pdfnet3_file="${m3_dir}/pdfnet3-release-asset-v1.tar.gz"
        if [[ -f "${pdfnet3_file}" ]]; then
            local pdfnet3_sha
            pdfnet3_sha="$(sha256sum "${pdfnet3_file}" | cut -d' ' -f1)"
            export CLEARCORE_DEV_PDFNET3_ASSET="${pdfnet3_file}"
            export CLEARCORE_DEV_PDFNET3_SHA256="${pdfnet3_sha}"
            echo "🧪 [Auto-detect] Modelo de isolamento pDFNet3 detectado: ${pdfnet3_file}"
        fi
    fi
}

detect_and_export_dev_models
