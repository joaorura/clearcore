#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Guia de Instalacao da Runtime NVIDIA TensorRT
# ==============================================================================
# Este script orienta o usuario na instalacao da runtime TensorRT para GPUs NVIDIA.
# Apos a instalacao, o Clearcore ativara a aceleracao Tier 1 (GPU Dedicada).
# ==============================================================================

set -euo pipefail

echo "=========================================================="
echo " Clearcore / Hippocamp - Instalador de Runtime TensorRT"
echo "=========================================================="

# 1. Detectar hardware fisico NVIDIA
echo "[1/4] Verificando presenca fisica de GPU NVIDIA..."
HAS_NVIDIA=false
if command -v lspci >/dev/null 2>&1; then
    if lspci -nn 2>/dev/null | grep -qi "10de:"; then
        HAS_NVIDIA=true
        echo "  -> GPU NVIDIA detectada via barramento PCI."
    fi
fi
if [[ -e /dev/nvidia0 || -e /proc/driver/nvidia/version ]]; then
    HAS_NVIDIA=true
fi

if [[ "${HAS_NVIDIA}" != "true" ]]; then
    echo "  [AVISO] Nenhuma GPU NVIDIA fisica foi detectada neste sistema."
    echo "  Se voce possui uma GPU NVIDIA externa (eGPU), conecte-a antes de continuar."
fi

# 2. Verificar se o driver NVIDIA e nvidia-smi estao ativos
echo "[2/4] Verificando driver NVIDIA..."
if command -v nvidia-smi >/dev/null 2>&1; then
    NVIDIA_DRIVER_VER=$(nvidia-smi --query-gpu=driver_version --format=csv,noheader 2>/dev/null | head -n1 || echo "desconhecido")
    GPU_NAME=$(nvidia-smi --query-gpu=name --format=csv,noheader 2>/dev/null | head -n1 || echo "NVIDIA GPU")
    echo "  -> GPU: ${GPU_NAME}"
    echo "  -> Versao do Driver: ${NVIDIA_DRIVER_VER}"
else
    echo "  [ALERTA] O utilitario 'nvidia-smi' nao foi encontrado."
    echo "  Certifique-se de que o driver proprietario NVIDIA esta instalado:"
    echo "    - Fedora: sudo dnf install -y akmod-nvidia xorg-x11-drv-nvidia-cuda"
    echo "    - Ubuntu/Debian: sudo apt-get install -y nvidia-driver-560"
    echo ""
fi

# 3. Verificar se TensorRT ja esta instalado
echo "[3/4] Verificando se libnvinfer.so (TensorRT) ja esta disponivel..."
TRT_LIB=""
for loc in \
    "/opt/tensorrt/lib/libnvinfer.so" \
    "/opt/tensorrt/lib/libnvinfer.so.11" \
    "/usr/local/tensorrt/lib/libnvinfer.so" \
    "/usr/lib64/libnvinfer.so" \
    "/usr/lib64/libnvinfer.so.11" \
    "/usr/lib64/libnvinfer.so.10" \
    "/usr/lib/x86_64-linux-gnu/libnvinfer.so" \
    "/usr/local/cuda/lib64/libnvinfer.so" \
    "${HOME}/.local/share/tensorrt/lib/libnvinfer.so"; do
    if [[ -e "${loc}" ]]; then
        TRT_LIB="${loc}"
        break
    fi
done

if [[ -z "${TRT_LIB}" ]] && command -v ldconfig >/dev/null 2>&1; then
    TRT_LIB=$(ldconfig -p 2>/dev/null | grep -i "libnvinfer\.so" | awk '{print $NF}' | head -n1 || true)
fi

if [[ -n "${TRT_LIB}" ]]; then
    echo "  [OK] TensorRT encontrado em: ${TRT_LIB}"
    echo "  A runtime já está pronta para uso pelo Clearcore!"
    exit 0
fi

echo "  -> TensorRT NAO encontrado no sistema."

# Verificar se ha pacote TAR local baixado em ~/Downloads
LOCAL_TAR=$(find "${HOME}/Downloads" -maxdepth 1 -name "TensorRT-*.tar.*" 2>/dev/null | head -n1 || true)

# 4. Instrucoes detalhadas por distribuicao
echo "[4/4] Guia de instalacao para seu sistema operacional:"
echo "----------------------------------------------------------"
echo "  [PORTAL OFICIAL DE DOWNLOAD]"
echo "  📥 https://developer.nvidia.com/tensorrt/download"
echo ""
echo "  [GUIA OFICIAL DE INSTALACAO (REFERENCIA)]"
echo "  📖 https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html"
echo "----------------------------------------------------------"

if [[ -n "${LOCAL_TAR}" ]]; then
    echo ""
    echo "  🌟 [PACOTE LOCAL ENCONTRADO EM SEUS DOWNLOADS]:"
    echo "     Arquivo: ${LOCAL_TAR}"
    echo ""
    echo "  💡 METODO RECOMENDADO / COMPROVADO (Instalacao direta no sistema):"
    echo "     1. Crie a pasta de destino e extraia o pacote:"
    if [[ "${LOCAL_TAR}" == *.zst ]]; then
        echo "        sudo mkdir -p /opt/tensorrt"
        echo "        sudo tar --zstd -xf '${LOCAL_TAR}' --strip-components=1 -C /opt/tensorrt"
    else
        echo "        sudo mkdir -p /opt/tensorrt"
        echo "        sudo tar -xzf '${LOCAL_TAR}' --strip-components=1 -C /opt/tensorrt"
    fi
    echo "     2. Registre as bibliotecas no carregador dinamico do sistema (ldconfig):"
    echo "        echo '/opt/tensorrt/lib' | sudo tee /etc/ld.so.conf.d/tensorrt.conf"
    echo "        sudo ldconfig"
    echo "     3. Pronto! O Clearcore detectara imediatamente o TensorRT."
    echo ""
fi

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
        echo "Metodo 1 (Comprovado - Pacote TAR Local):"
        echo "  1. Baixe o pacote TAR para sua versao do CUDA (ex: CUDA 13.x / Linux x86_64):"
        echo "     https://developer.nvidia.com/tensorrt/download"
        echo "  2. Extraia em /opt/tensorrt:"
        echo "     sudo mkdir -p /opt/tensorrt"
        echo "     sudo tar --zstd -xf ~/Downloads/TensorRT-*.tar.zst --strip-components=1 -C /opt/tensorrt"
        echo "  3. Configure o ldconfig:"
        echo "     echo '/opt/tensorrt/lib' | sudo tee /etc/ld.so.conf.d/tensorrt.conf"
        echo "     sudo ldconfig"
        echo ""
        echo "Metodo 2 (Via DNF se repositorio NVIDIA estiver configurado):"
        echo "  sudo dnf install -y tensorrt || pip install tensorrt"
        ;;
    ubuntu|debian|pop)
        echo "Instrucoes para Ubuntu / Debian / Pop!_OS:"
        echo ""
        echo "  1. Baixe no portal oficial: https://developer.nvidia.com/tensorrt/download"
        echo "  2. Ou adicione o repositorio NVIDIA oficial (referencia):"
        echo "     wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2404/x86_64/cuda-keyring_1.1-1_all.deb"
        echo "     sudo dpkg -i cuda-keyring_1.1-1_all.deb"
        echo "     sudo apt-get update && sudo apt-get install -y libnvinfer11 libnvonnxparsers11"
        echo "     sudo ldconfig"
        ;;
        echo "  3. Atualize o cache de bibliotecas:"
        echo "     sudo ldconfig"
        ;;
    arch|manjaro)
        echo "Instrucoes para Arch Linux / Manjaro:"
        echo ""
        echo "  1. Instale via repositório ou AUR:"
        echo "     yay -S tensorrt"
        echo "     sudo ldconfig"
        ;;
    *)
        echo "Instrucoes genericas Linux:"
        echo "  1. Acesse https://developer.nvidia.com/tensorrt"
        echo "  2. Baixe o tarball Linux x86_64 correspondente"
        echo "  3. Extraia as bibliotecas libnvinfer*.so em /usr/local/lib64 ou /usr/lib64 e execute 'sudo ldconfig'"
        ;;
esac

echo "----------------------------------------------------------"
echo "Nota: O Clearcore utiliza estritamente TensorRT para placas NVIDIA."
echo "Enquanto o TensorRT nao estiver instalado, o Clearcore continuara"
echo "automaticamente usando a NPU (Intel/AMD), GPU Integrada ou CPU Tract."
echo "=========================================================="
