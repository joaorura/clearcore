#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Guia de Instalacao do AMD Ryzen AI Software (NPU & iGPU)
# ==============================================================================
# Este script orienta o usuario na instalacao do driver e runtime Ryzen AI:
# - AMD Ryzen AI NPU (Tier 2: XDNA / XDNA 2, Ryzen 7040/8040/AI 300 series)
# - AMD Radeon iGPU (Tier 3: RDNA 3 / 3.5 Graphics)
# ==============================================================================

set -euo pipefail

echo "=========================================================="
echo " Clearcore / Hippocamp - Instalador AMD Ryzen AI"
echo "=========================================================="

# 1. Detectar hardware fisico AMD
echo "[1/4] Verificando presenca fisica de hardware AMD Ryzen AI..."
HAS_AMD_NPU=false
HAS_AMD_IGPU=false

if command -v lspci >/dev/null 2>&1; then
    if lspci -nn 2>/dev/null | grep -qi "1022:1502"; then
        HAS_AMD_NPU=true
        echo "  -> AMD NPU detectada via barramento PCI (1022:1502)."
    fi
    if lspci -nn 2>/dev/null | grep -qi "1002:.*\[030"; then
        HAS_AMD_IGPU=true
        echo "  -> AMD Radeon GPU/iGPU detectada via barramento PCI."
    fi
fi

if [[ -e /dev/amdxdna ]]; then
    HAS_AMD_NPU=true
    echo "  -> Driver de dispositivo /dev/amdxdna ativo!"
fi

if [[ "${HAS_AMD_NPU}" != "true" && "${HAS_AMD_IGPU}" != "true" ]]; then
    echo "  [AVISO] Nenhuma NPU ou GPU AMD detectada neste sistema."
fi

# 2. Verificar se o driver XDNA e XRT estao instalados
echo "[2/4] Verificando instalacao do runtime XRT / Ryzen AI..."
XRT_LIB=""
for loc in \
    "/opt/xilinx/xrt/lib/libxrt_core.so" \
    "/usr/lib64/libxrt_core.so" \
    "/usr/lib/x86_64-linux-gnu/libxrt_core.so"; do
    if [[ -e "${loc}" ]]; then
        XRT_LIB="${loc}"
        break
    fi
done

if [[ -z "${XRT_LIB}" ]] && command -v ldconfig >/dev/null 2>&1; then
    XRT_LIB=$(ldconfig -p 2>/dev/null | grep "libxrt_core\.so" | awk '{print $NF}' | head -n1 || true)
fi

if [[ -n "${XRT_LIB}" ]]; then
    echo "  [OK] XRT (Xilinx Runtime) encontrado em: ${XRT_LIB}"
    if [[ -e /dev/amdxdna ]]; then
        echo "  -> Driver /dev/amdxdna pronto para inferencia NPU no Clearcore!"
    else
        echo "  -> [AVISO] Runtime presente, mas /dev/amdxdna ausente."
        echo "     Instale o modulo do kernel: https://github.com/amd/xdna-driver"
    fi
    exit 0
fi

echo "  -> Ryzen AI / XRT NAO encontrado no sistema."

# 3. Instrucoes para instalacao no Linux
echo "[3/4] Guia de instalacao para Linux (Ubuntu / Fedora / Arch):"
echo "----------------------------------------------------------"
echo "O suporte a AMD Ryzen AI no Linux requer 2 componentes:"
echo "1. Driver de Kernel XDNA (/dev/amdxdna)"
echo "2. Pacote XRT (Xilinx Runtime) + Vitis-AI ONNX Execution Provider"
echo ""
echo "Passo 1: Instalar o driver de kernel XDNA:"
echo "  git clone https://github.com/amd/xdna-driver.git"
echo "  cd xdna-driver"
echo "  # Compilar e carregar modulo dkms:"
echo "  sudo ./build.sh -d"
echo "  sudo modprobe amdxdna"
echo ""
echo "Passo 2: Instalar o runtime XRT:"
echo "  Baixe o pacote oficial XRT para sua distribuicao em:"
echo "  https://github.com/Xilinx/XRT/releases"
echo "  - Fedora: sudo dnf install -y ./xrt_*.rpm"
echo "  - Ubuntu: sudo apt install -y ./xrt_*.deb"
echo ""
echo "Passo 3: Habilitar permissoes de usuario:"
echo "  sudo usermod -aG render \$USER"
echo ""
echo "Passo 4: Verificacao:"
echo "  /opt/xilinx/xrt/bin/xbutil examine"
echo "----------------------------------------------------------"
echo "[4/4] Informacoes para Windows:"
echo "No Windows 11, o driver AMD NPU e atualizado via Windows Update."
echo "Para instalar a suite completa, baixe o instalador oficial:"
echo "https://www.amd.com/en/developer/resources/ryzen-ai-software.html"
echo "=========================================================="
