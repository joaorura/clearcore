#!/usr/bin/env bash
# ==============================================================================
# Clearcore - Diagnostico do OpenVINO (Intel NPU / iGPU / CPU) e guia de drivers
# ==============================================================================
# Os pacotes do Clearcore para Linux x64 (tar.gz, AppImage, RPM, DEB) ja trazem a
# runtime OpenVINO com os plugins CPU, GPU e NPU em <app>/resources/openvino. Nada
# precisa ser instalado para o OpenVINO em si; a NPU e a GPU Intel dependem apenas
# dos DRIVERS do sistema (kernel + user space), que este script verifica e orienta.
#
# Este script NAO instala nada nem usa sudo: ele mostra o que falta e o comando.
#
# Busca da libopenvino_c (mesma ordem do daemon, crates/runtime-openvino/src/ffi.rs):
#   1. OPENVINO_LIB_PATH (arquivo libopenvino_c.so* ou diretorio)
#   2. runtime embutido: <app>/resources/openvino (este script fica em resources/scripts)
#      e /opt/clearcore/resources/openvino
#   3. /usr/lib64, /lib64, /usr/local/lib64, /usr/lib/x86_64-linux-gnu
# Series aceitas: 2025.x e 2026.x.
#
# Uso: install-openvino.sh            (diagnostico + guia)
#      install-openvino.sh --check    (so o diagnostico; sai 0 se a NPU/GPU presentes tem runtime)
# ==============================================================================

set -uo pipefail

CHECK_ONLY=false
[[ "${1:-}" == "--check" ]] && CHECK_ONLY=true

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "=========================================================="
echo " Clearcore - OpenVINO (Intel NPU / iGPU / CPU)"
echo "=========================================================="

# 1. Hardware Intel
HAS_INTEL_NPU=false
HAS_INTEL_GPU=false
HAS_INTEL_CPU=false
if compgen -G "/dev/accel/accel*" >/dev/null; then
    HAS_INTEL_NPU=true
fi
if command -v lspci >/dev/null 2>&1; then
    if lspci -nn 2>/dev/null | grep -qiE "8086:(7d1d|7d1e|643e|ad1d|b03e)"; then
        HAS_INTEL_NPU=true
    fi
    if lspci -nn 2>/dev/null | grep -iE "VGA|Display|3D" | grep -qi "8086:"; then
        HAS_INTEL_GPU=true
    fi
fi
if grep -qi "GenuineIntel" /proc/cpuinfo 2>/dev/null; then
    HAS_INTEL_CPU=true
fi
echo "[1/3] Hardware: NPU Intel=${HAS_INTEL_NPU}  GPU Intel=${HAS_INTEL_GPU}  CPU Intel=${HAS_INTEL_CPU}"

# 2. Runtime OpenVINO
OV_DIRS=()
if [[ -n "${OPENVINO_LIB_PATH:-}" ]]; then
    if [[ -d "${OPENVINO_LIB_PATH}" ]]; then
        OV_DIRS+=("${OPENVINO_LIB_PATH}")
    else
        OV_DIRS+=("$(dirname "${OPENVINO_LIB_PATH}")")
    fi
fi
OV_DIRS+=("${SCRIPT_DIR}/../openvino" "${SCRIPT_DIR}/../resources/openvino"
          /opt/clearcore/resources/openvino /usr/lib64 /lib64 /usr/local/lib64
          /usr/lib/x86_64-linux-gnu)

OV_LIB=""
for d in "${OV_DIRS[@]}"; do
    [[ -d "${d}" ]] || continue
    # Maior sufixo numerico primeiro (libopenvino_c.so.2641 antes de .2600).
    cand="$(find "${d}" -maxdepth 1 -name 'libopenvino_c.so*' \( -type f -o -type l \) 2>/dev/null \
        | sort -V -r | head -n1)"
    if [[ -n "${cand}" && -e "${cand}" ]]; then
        OV_LIB="$(cd "$(dirname "${cand}")" && pwd)/$(basename "${cand}")"
        break
    fi
done

OV_NPU_PLUGIN=false
OV_GPU_PLUGIN=false
if [[ -z "${OV_LIB}" ]]; then
    echo "[2/3] OpenVINO: NAO encontrado (status not_found)."
    echo "      O pacote do Clearcore traz a runtime em resources/openvino; reinstale o pacote"
    echo "      ou aponte OPENVINO_LIB_PATH para uma libopenvino_c.so.* (2025.x ou 2026.x)."
else
    OV_DIR="$(dirname "${OV_LIB}")"
    SUFFIX="${OV_LIB##*.so.}"
    echo "[2/3] OpenVINO: ${OV_LIB}"
    case "${SUFFIX}" in
        25*|26*|2025.*|2026.*) echo "      serie suportada (sufixo ${SUFFIX})" ;;
        "${OV_LIB}") echo "      sem sufixo de versao; o daemon confere a versao ao carregar" ;;
        *) echo "      [AVISO] serie possivelmente incompativel (sufixo ${SUFFIX}); aceitas: 2025.x, 2026.x" ;;
    esac
    # Plugins ao lado da biblioteca (wheel pip, runtime embutido) ou em openvino-<versao>/
    # (pacote do Fedora: /usr/lib64/openvino-2026.0.0/).
    for pd in "${OV_DIR}" "${OV_DIR}"/openvino-*; do
        [[ -e "${pd}/libopenvino_intel_npu_plugin.so" ]] && OV_NPU_PLUGIN=true
        [[ -e "${pd}/libopenvino_intel_gpu_plugin.so" ]] && OV_GPU_PLUGIN=true
    done
    echo "      plugin NPU=${OV_NPU_PLUGIN}  plugin GPU=${OV_GPU_PLUGIN}"
    if [[ "${HAS_INTEL_NPU}" == "true" && "${OV_NPU_PLUGIN}" == "false" ]]; then
        echo "      [AVISO] esta runtime nao tem o plugin NPU (status npu_plugin_missing)."
        echo "      O pacote 'openvino' do Fedora (2026.0) so traz CPU/GPU; o Clearcore usa a"
        echo "      runtime embutida (resources/openvino), que tem o plugin NPU."
    fi
fi

# 3. Drivers e permissoes
echo "[3/3] Drivers:"
DRIVER_OK=true
if [[ "${HAS_INTEL_NPU}" == "true" ]]; then
    if compgen -G "/usr/lib*/libze_intel_npu.so*" >/dev/null || compgen -G "/usr/lib/x86_64-linux-gnu/libze_intel_npu.so*" >/dev/null; then
        echo "      NPU: driver user space (level-zero NPU) presente"
    else
        echo "      NPU: [FALTA] driver user space da NPU (libze_intel_npu)"
        DRIVER_OK=false
    fi
    for node in /dev/accel/accel*; do
        [[ -e "${node}" ]] || continue
        if [[ -r "${node}" && -w "${node}" ]]; then
            echo "      NPU: ${node} acessivel"
        else
            echo "      NPU: [SEM ACESSO] ${node} (adicione o usuario ao grupo render)"
            DRIVER_OK=false
        fi
    done
fi
if [[ "${HAS_INTEL_GPU}" == "true" ]]; then
    if compgen -G "/usr/lib*/libze_intel_gpu.so*" >/dev/null || compgen -G "/usr/lib*/intel-opencl/libigdrcl.so" >/dev/null \
       || compgen -G "/usr/lib/x86_64-linux-gnu/libze_intel_gpu.so*" >/dev/null; then
        echo "      GPU: compute runtime Intel presente"
    else
        echo "      GPU: [FALTA] compute runtime Intel (level-zero / OpenCL)"
        DRIVER_OK=false
    fi
    if ! compgen -G "/usr/lib*/libOpenCL.so.1" >/dev/null && ! compgen -G "/usr/lib/x86_64-linux-gnu/libOpenCL.so.1" >/dev/null; then
        echo "      GPU: [FALTA] libOpenCL.so.1 (o plugin GPU do OpenVINO depende dela)"
        DRIVER_OK=false
    fi
fi

if [[ -z "${OV_LIB}" ]] || [[ "${HAS_INTEL_NPU}" == "true" && "${OV_NPU_PLUGIN}" == "false" ]]; then
    DRIVER_OK=false
fi

if [[ "${CHECK_ONLY}" == "true" ]]; then
    [[ -n "${OV_LIB}" && "${DRIVER_OK}" == "true" ]] && exit 0
    exit 1
fi

if [[ "${DRIVER_OK}" == "true" && -n "${OV_LIB}" ]]; then
    echo "----------------------------------------------------------"
    echo "Tudo pronto. Escolha 'Intel OpenVINO (NPU)' ou '(iGPU)' nas configuracoes do Clearcore;"
    echo "o GetStatus do daemon mostra active_backend e, se cair para o tract, o motivo"
    echo "(backend_fallback_reason)."
    exit 0
fi

OS_ID="linux"
if [[ -f /etc/os-release ]]; then
    # shellcheck disable=SC1091
    OS_ID="$(. /etc/os-release && echo "${ID:-linux}")"
fi

echo "----------------------------------------------------------"
echo "Como instalar os drivers (${OS_ID}):"
case "${OS_ID}" in
    fedora|rhel|centos|rocky|almalinux)
        echo "  NPU: sudo dnf install -y intel-npu-driver intel-npu-firmware oneapi-level-zero"
        echo "  GPU: sudo dnf install -y intel-compute-runtime intel-level-zero OpenCL-ICD-Loader"
        echo "  (o pacote 'openvino' do Fedora nao e necessario: o Clearcore traz a propria runtime)"
        ;;
    ubuntu|debian|pop|linuxmint)
        echo "  NPU: instale os .deb do driver em https://github.com/intel/linux-npu-driver/releases"
        echo "       (intel-driver-compiler-npu, intel-fw-npu, intel-level-zero-npu) e: sudo apt install -y libze1"
        echo "  GPU: sudo apt install -y intel-opencl-icd libze-intel-gpu1 libze1 ocl-icd-libopencl1"
        ;;
    arch|manjaro|endeavouros)
        echo "  NPU: yay -S intel-npu-driver-bin    (AUR) e sudo pacman -S level-zero-loader"
        echo "  GPU: sudo pacman -S --needed intel-compute-runtime level-zero-loader ocl-icd"
        ;;
    *)
        echo "  NPU: https://github.com/intel/linux-npu-driver/releases"
        echo "  GPU: https://github.com/intel/compute-runtime/releases"
        ;;
esac
echo "  Acesso a /dev/accel e /dev/dri: sudo usermod -aG render \$USER  (e abra uma nova sessao)"
echo "  Kernel: o modulo intel_vpu (NPU) vem no kernel Linux >= 6.8."
echo "=========================================================="
exit 0
