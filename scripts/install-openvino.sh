#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Guia de Instalacao da Runtime Intel OpenVINO & NPU
# ==============================================================================
# Este script orienta o usuario na instalacao da runtime OpenVINO para:
# - Intel NPU (Tier 2: Core Ultra 100/200 series, Arrow Lake, Lunar Lake, Meteor Lake)
# - Intel Arc / iGPU (Tier 3: Xe / Arc Graphics)
# - Intel CPU (Tier 4: Otimizacao AMX / AVX-VNNI)
# ==============================================================================

set -euo pipefail

echo "=========================================================="
echo " Clearcore / Hippocamp - Instalador OpenVINO & Intel NPU"
echo "=========================================================="

# 1. Detectar hardware Intel
echo "[1/4] Verificando presenca fisica de hardware Intel..."
HAS_INTEL_NPU=false
HAS_INTEL_GPU=false
HAS_INTEL_CPU=false

if command -v lspci >/dev/null 2>&1; then
    if lspci -nn 2>/dev/null | grep -qi "8086:7d1d\|8086:7d1e\|8086:643e"; then
        HAS_INTEL_NPU=true
        echo "  -> Intel NPU detectada via barramento PCI (8086:7d1d/7d1e)."
    fi
    if lspci -nn 2>/dev/null | grep -qi "8086:.*\[030"; then
        HAS_INTEL_GPU=true
        echo "  -> Intel Arc / iGPU detectada via barramento PCI."
    fi
fi

if [[ -e /dev/accel/accel0 || -d /sys/class/accel ]]; then
    HAS_INTEL_NPU=true
    echo "  -> Driver de kernel de aceleracao NPU (/dev/accel) ativo."
fi

if grep -qi "GenuineIntel" /proc/cpuinfo 2>/dev/null; then
    HAS_INTEL_CPU=true
    echo "  -> Processador Intel detectado."
fi

# 2. Verificar se OpenVINO ja esta instalado
echo "[2/4] Verificando instalacao atual do OpenVINO..."
OPENVINO_LIB=""
for loc in \
    "/lib64/libopenvino.so.2510" \
    "/usr/lib64/libopenvino.so" \
    "/lib64/libopenvino.so" \
    "/usr/lib/x86_64-linux-gnu/libopenvino.so" \
    "/opt/intel/openvino/runtime/lib/intel64/libopenvino.so"; do
    if [[ -e "${loc}" ]]; then
        OPENVINO_LIB="${loc}"
        break
    fi
done

if [[ -z "${OPENVINO_LIB}" ]] && command -v ldconfig >/dev/null 2>&1; then
    OPENVINO_LIB=$(ldconfig -p 2>/dev/null | grep "libopenvino\.so" | awk '{print $NF}' | head -n1 || true)
fi

if [[ -n "${OPENVINO_LIB}" ]]; then
    echo "  [OK] OpenVINO encontrado em: ${OPENVINO_LIB}"
    echo "  Status do driver Intel NPU (Level Zero):"
    if [[ -e /dev/accel/accel0 ]]; then
        echo "    -> Dispositivo /dev/accel/accel0 pronto para inferencia NPU!"
    else
        echo "    -> [AVISO] /dev/accel/accel0 nao encontrado. Para NPU, certifique-se"
        echo "       de que o driver 'intel-driver-compiler-npu' ou modulo 'intel_vpu' esta carregado."
    fi
    exit 0
fi

echo "  -> OpenVINO NAO encontrado no sistema."

# 3. Instrucoes detalhadas por distribuicao
echo "[3/4] Guia de instalacao para seu sistema operacional:"
echo "----------------------------------------------------------"

OS_ID="linux"
if [[ -f /etc/os-release ]]; then
    # shellcheck disable=SC1091
    source /etc/os-release
    OS_ID="${ID:-linux}"
fi

case "${OS_ID}" in
    fedora|rhel|centos)
        echo "Instrucoes para Fedora / RHEL / CentOS:"
        echo ""
        echo "  1. Instalar bibliotecas OpenVINO e Level Zero NPU via dnf:"
        echo "     sudo dnf install -y openvino oneapi-level-zero"
        echo "     # Se disponivel no repositorio da distro:"
        echo "     sudo dnf install -y intel-driver-compiler-npu intel-level-zero-npu intel-compute-runtime"
        echo ""
        echo "  2. Repositorio Oficial Intel OpenVINO (opcional, para versao mais recente):"
        echo "     sudo tee /etc/yum.repos.d/intel-openvino.repo << 'EOF'"
        echo "[intel-openvino]"
        echo "name=Intel OpenVINO Repository"
        echo "baseurl=https://yum.repos.intel.com/openvino/2025"
        echo "enabled=1"
        echo "gpgcheck=1"
        echo "gpgkey=https://yum.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB"
        echo "EOF"
        echo "     sudo dnf install -y openvino"
        ;;
    ubuntu|debian|pop)
        echo "Instrucoes para Ubuntu / Debian / Pop!_OS:"
        echo ""
        echo "  1. Instale os pacotes oficiais OpenVINO da Intel:"
        echo "     wget https://apt.repos.intel.com/intel-gpg-keys/GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB"
        echo "     sudo gpg --dearmor --output /usr/share/keyrings/oneapi-archive-keyring.gpg GPG-PUB-KEY-INTEL-SW-PRODUCTS.PUB"
        echo "     echo 'deb [signed-by=/usr/share/keyrings/oneapi-archive-keyring.gpg] https://apt.repos.intel.com/openvino/2025 ubuntu24 main' | sudo tee /etc/apt/sources.list.d/intel-openvino-2025.list"
        echo "     sudo apt-get update"
        echo "     sudo apt-get install -y openvino intel-level-zero-npu intel-opencl-icd"
        ;;
    arch|manjaro)
        echo "Instrucoes para Arch Linux / Manjaro:"
        echo ""
        echo "  yay -S openvino intel-npu-driver-bin intel-compute-runtime"
        ;;
    *)
        echo "Instrucoes genericas Linux:"
        echo "  Acesse https://software.intel.com/content/www/us/en/develop/tools/openvino-toolkit/download.html"
        echo "  e instale o OpenVINO Runtime Archive."
        ;;
esac

echo "----------------------------------------------------------"
echo "[4/4] Permissoes para NPU (/dev/accel):"
echo "  Certifique-se de que seu usuario tem acesso ao grupo de aceleradores:"
echo "    sudo usermod -aG render \$USER"
echo "=========================================================="
