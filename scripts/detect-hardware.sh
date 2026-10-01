#!/usr/bin/env bash
set -euo pipefail

echo "=========================================================="
echo " Clearcore / Hippocamp - Hardware & Runtime Detector"
echo "=========================================================="

echo "[1/3] Verificando barramento PCI e dispositivos de aceleracao..."
PCI_INFO=""
if command -v lspci >/dev/null 2>&1; then
    PCI_INFO="$(lspci -nn 2>/dev/null || true)"
fi

HAS_NVIDIA=false
HAS_INTEL_NPU=false
HAS_AMD_NPU=false
HAS_INTEL_GPU=false
HAS_AMD_GPU=false

if echo "${PCI_INFO}" | grep -qi "10de:"; then
    HAS_NVIDIA=true
fi
if echo "${PCI_INFO}" | grep -qi "8086:7d1d\|8086:7d1e" || [[ -e /dev/accel/accel0 && $(cat /sys/class/accel/accel0/device/vendor 2>/dev/null || true) == "0x8086" ]]; then
    HAS_INTEL_NPU=true
fi
if echo "${PCI_INFO}" | grep -qi "1022:1502" || [[ -e /dev/amdxdna ]]; then
    HAS_AMD_NPU=true
fi
if echo "${PCI_INFO}" | grep -qi "8086:.*\[030"; then
    HAS_INTEL_GPU=true
fi
if echo "${PCI_INFO}" | grep -qi "1002:.*\[030"; then
    HAS_AMD_GPU=true
fi

echo "[2/3] Verificando presenca das runtimes no sistema..."
HAS_TRT_RUNTIME=false
HAS_OPENVINO_RUNTIME=false
HAS_RYZENAI_RUNTIME=false

if ldconfig -p 2>/dev/null | grep -q "libnvinfer\.so"; then
    HAS_TRT_RUNTIME=true
fi
if ldconfig -p 2>/dev/null | grep -q "libopenvino\.so" || [[ -e /lib64/libopenvino.so || -e /usr/lib64/libopenvino.so || -e /lib64/libopenvino.so.2510 ]]; then
    HAS_OPENVINO_RUNTIME=true
fi
if ldconfig -p 2>/dev/null | grep -q "libxrt_core\.so" || [[ -d /opt/xilinx/xrt ]]; then
    HAS_RYZENAI_RUNTIME=true
fi

echo "[3/3] Relatorio de Auditoria de Hardware & Runtimes:"
echo "----------------------------------------------------------"

# NVIDIA
if [[ "${HAS_NVIDIA}" == "true" ]]; then
    echo "  [DETECTADO] GPU Dedicada NVIDIA (Blackwell/Ada/Ampere)"
    if [[ "${HAS_TRT_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (TensorRT ativo)"
    else
        echo "              Status Runtime: AUSENTE (TensorRT nao encontrado)"
        echo "              -> ACAO: Execute './scripts/install-tensorrt.sh' para ativar TensorRT!"
    fi
else
    echo "  [NAO DETECTADO] GPU Dedicada NVIDIA"
fi

# Intel NPU
if [[ "${HAS_INTEL_NPU}" == "true" ]]; then
    echo "  [DETECTADO] Intel NPU (Core Ultra 200H / Arrow Lake)"
    if [[ "${HAS_OPENVINO_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (OpenVINO ativo)"
    else
        echo "              Status Runtime: AUSENTE (OpenVINO nao encontrado)"
        echo "              -> ACAO: Execute './scripts/install-openvino.sh' para ativar NPU Intel!"
    fi
fi

# AMD NPU
if [[ "${HAS_AMD_NPU}" == "true" ]]; then
    echo "  [DETECTADO] AMD Ryzen AI NPU (XDNA / XDNA 2)"
    if [[ "${HAS_RYZENAI_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (Ryzen AI Software ativo)"
    else
        echo "              Status Runtime: AUSENTE (XRT / Vitis-AI nao encontrado)"
        echo "              -> ACAO: Execute './scripts/install-ryzenai.sh' para ativar Ryzen AI!"
    fi
fi

# Intel Arc / iGPU
if [[ "${HAS_INTEL_GPU}" == "true" ]]; then
    echo "  [DETECTADO] Intel Arc / iGPU"
    if [[ "${HAS_OPENVINO_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (OpenVINO GPU ativo)"
    fi
fi

echo "  [SEMPRE ATIVO] CPU Tract Baseline (100% Rust Seguro, Fail-Closed)"
echo "----------------------------------------------------------"
echo "Hierarquia de Selecao:"
echo "1. GPU Dedicada (TensorRT 100 -> DirectML 90 -> Vulkan 85)"
echo "2. NPU (Intel NPU / AMD Ryzen AI NPU 80)"
echo "3. GPU Integrada (Intel Arc / AMD Radeon iGPU 70)"
echo "4. CPU (Intel OpenVINO AMX/VNNI 60 -> Tract 50)"
echo "=========================================================="
