#!/usr/bin/env bash
# ==============================================================================
# Clearcore / Hippocamp - Detector de Aceleradores de Hardware & Runtimes de IA
# ==============================================================================
# Inspeciona dispositivos fisicos e runtimes no host:
#   - NVIDIA Dedicated GPU (TensorRT / CUDA)
#   - Intel OpenVINO NPU (Intel AI Boost / NPU Integrada)
#   - Intel OpenVINO iGPU (Intel Arc / Arrow Lake Graphics)
#   - Intel OpenVINO CPU (Intel Core Ultra AVX2 / AMX / VNNI Otimizado)
#   - AMD Ryzen AI NPU (XDNA / XDNA 2)
#   - AMD Radeon iGPU (RDNA / ROCm / Vulkan)
#   - Apple Silicon (CoreML - Neural Engine / Metal)
#   - CPU Nativo Baseline (Tract Pure-Rust Fail-Safe)
# ==============================================================================

set -euo pipefail

OUTPUT_JSON=false
if [[ "${1:-}" == "--json" ]]; then
    OUTPUT_JSON=true
fi

if [[ "${OUTPUT_JSON}" == "false" ]]; then
    echo "=========================================================="
    echo " Clearcore - Hardware & Runtime Detector"
    echo "=========================================================="
    echo "[1/3] Verificando barramento PCI e dispositivos de aceleracao..."
fi

PCI_INFO=""
if command -v lspci >/dev/null 2>&1; then
    PCI_INFO="$(lspci -nn 2>/dev/null || true)"
fi

HAS_NVIDIA=false
HAS_INTEL_NPU=false
HAS_INTEL_GPU=false
HAS_INTEL_CPU=false
HAS_AMD_NPU=false
HAS_AMD_GPU=false

# 1. NVIDIA GPU
if echo "${PCI_INFO}" | grep -qi "10de:"; then
    HAS_NVIDIA=true
fi
if [[ -e /dev/nvidia0 || -e /proc/driver/nvidia/version ]]; then
    HAS_NVIDIA=true
fi

# 2. Intel NPU
if echo "${PCI_INFO}" | grep -qi "8086:7d1d\|8086:7d1e" || [[ -e /dev/accel/accel0 && $(cat /sys/class/accel/accel0/device/vendor 2>/dev/null || true) == "0x8086" ]]; then
    HAS_INTEL_NPU=true
fi

# 3. Intel iGPU / Arc GPU
if echo "${PCI_INFO}" | grep -i "8086:" | grep -qiE 'vga|display|graphics|\[030'; then
    HAS_INTEL_GPU=true
fi

# 4. CPU Detection (Dynamic Model Name and Vendor)
CPU_MODEL=""
if [[ -f /proc/cpuinfo ]]; then
    CPU_MODEL="$(grep -m1 "model name" /proc/cpuinfo | cut -d: -f2 | sed 's/^[ \t]*//' || true)"
fi
if [[ -z "${CPU_MODEL}" ]] && command -v lscpu >/dev/null 2>&1; then
    CPU_MODEL="$(lscpu | grep -i "Model name:" | cut -d: -f2 | sed 's/^[ \t]*//' || true)"
fi
if [[ -z "${CPU_MODEL}" ]]; then
    CPU_MODEL="Processador Host x86_64"
fi

HAS_AMD_CPU=false
if echo "${CPU_MODEL}" | grep -qiE "amd|ryzen"; then
    HAS_AMD_CPU=true
    HAS_INTEL_CPU=false
elif echo "${CPU_MODEL}" | grep -qi "intel"; then
    HAS_INTEL_CPU=true
    HAS_AMD_CPU=false
fi

# 5. AMD Ryzen AI NPU (XDNA / XDNA 2)
if echo "${PCI_INFO}" | grep -qi "1022:1502\|1022:17f0" || [[ -e /dev/amdxdna || -d /sys/class/accel/amdxdna0 ]]; then
    HAS_AMD_NPU=true
fi

# 6. AMD Radeon iGPU / dGPU (RDNA Graphics)
if echo "${PCI_INFO}" | grep -i "1002:" | grep -qiE 'vga|display|graphics|\[030'; then
    HAS_AMD_GPU=true
elif [[ "${HAS_AMD_CPU}" == "true" ]]; then
    # Most modern AMD APUs include Radeon integrated graphics
    HAS_AMD_GPU=true
fi

if [[ "${OUTPUT_JSON}" == "false" ]]; then
    echo "[2/3] Verificando presenca das runtimes no sistema..."
fi

# ------------------------------------------------------------------------------
# Inspecao de Runtimes: NVIDIA TensorRT
# ------------------------------------------------------------------------------
HAS_TRT_RUNTIME=false
TRT_PATH=""

if ldconfig -p 2>/dev/null | grep -qi "libnvinfer\.so"; then
    HAS_TRT_RUNTIME=true
    TRT_PATH="$(ldconfig -p 2>/dev/null | grep -i 'libnvinfer\.so' | head -n1 | awk '{print $NF}')"
fi

if [[ "${HAS_TRT_RUNTIME}" == "false" ]]; then
    for candidate in \
        /usr/lib64/libnvinfer.so* \
        /usr/lib/x86_64-linux-gnu/libnvinfer.so* \
        /usr/local/cuda/lib64/libnvinfer.so* \
        /opt/cuda/lib64/libnvinfer.so* \
        /usr/local/tensorrt/lib/libnvinfer.so* \
        /home/*/.local/lib/python*/site-packages/tensorrt_libs/libnvinfer.so* \
        /home/*/miniconda3/lib*/python*/site-packages/tensorrt_libs/libnvinfer.so* \
        /home/*/*/.venv/lib*/python*/site-packages/tensorrt_libs/libnvinfer.so* \
        /home/*/*/.venv/lib64/python*/site-packages/tensorrt_libs/libnvinfer.so*; do
        if [[ -e "${candidate}" ]]; then
            HAS_TRT_RUNTIME=true
            TRT_PATH="${candidate}"
            break
        fi
    done
fi

if [[ "${HAS_TRT_RUNTIME}" == "false" ]]; then
    if python3 -c "import tensorrt" 2>/dev/null || \
       /home/joaorura/miniconda3/bin/python3 -c "import tensorrt" 2>/dev/null || \
       /home/joaorura/nvidia-broadcast-linux/.venv/bin/python3 -c "import tensorrt" 2>/dev/null; then
        HAS_TRT_RUNTIME=true
        TRT_PATH="python:tensorrt"
    fi
fi

# ------------------------------------------------------------------------------
# Inspecao de Runtimes: Intel OpenVINO (NPU, iGPU, CPU)
# ------------------------------------------------------------------------------
HAS_OPENVINO_BASE=false
HAS_OPENVINO_NPU=false
HAS_OPENVINO_GPU=false
HAS_OPENVINO_CPU=false

if ldconfig -p 2>/dev/null | grep -q "libopenvino\.so" || [[ -e /lib64/libopenvino.so || -e /usr/lib64/libopenvino.so || -e /lib64/libopenvino.so.2510 ]]; then
    HAS_OPENVINO_BASE=true
fi

# Plugin NPU (Only if Intel NPU hardware is present)
if [[ "${HAS_INTEL_NPU}" == "true" ]]; then
    for p in /usr/lib64/openvino*/libopenvino_intel_npu_plugin.so /lib64/openvino*/libopenvino_intel_npu_plugin.so /usr/lib/x86_64-linux-gnu/openvino*/libopenvino_intel_npu_plugin.so; do
        if [[ -e "${p}" ]]; then
            HAS_OPENVINO_NPU=true
            break
        fi
    done
fi

# Plugin GPU (Only if Intel iGPU/dGPU is present)
if [[ "${HAS_INTEL_GPU}" == "true" ]]; then
    for p in /usr/lib64/openvino*/libopenvino_intel_gpu_plugin.so /lib64/openvino*/libopenvino_intel_gpu_plugin.so /usr/lib/x86_64-linux-gnu/openvino*/libopenvino_intel_gpu_plugin.so; do
        if [[ -e "${p}" ]]; then
            HAS_OPENVINO_GPU=true
            break
        fi
    done
fi

# Plugin CPU (Only if Intel CPU is present - never on AMD)
if [[ "${HAS_INTEL_CPU}" == "true" ]]; then
    for p in /usr/lib64/openvino*/libopenvino_intel_cpu_plugin.so /lib64/openvino*/libopenvino_intel_cpu_plugin.so /usr/lib/x86_64-linux-gnu/openvino*/libopenvino_intel_cpu_plugin.so; do
        if [[ -e "${p}" ]]; then
            HAS_OPENVINO_CPU=true
            break
        fi
    done
fi

# Inspecao de dispositivos via OpenVINO Core (apenas para hardware Intel relevante)
if [[ "${HAS_OPENVINO_BASE}" == "true" ]]; then
    OV_DEVICES=$(python3 -c "import openvino as ov; print(','.join(ov.Core().available_devices))" 2>/dev/null || \
                 /home/joaorura/miniconda3/bin/python3 -c "import openvino as ov; print(','.join(ov.Core().available_devices))" 2>/dev/null || \
                 /home/joaorura/.local/share/whisper-ov/.venv/bin/python3 -c "import openvino as ov; print(','.join(ov.Core().available_devices))" 2>/dev/null || true)
    if [[ "${OV_DEVICES}" == *"NPU"* && "${HAS_INTEL_NPU}" == "true" ]]; then
        HAS_OPENVINO_NPU=true
    fi
    if [[ "${OV_DEVICES}" == *"GPU"* && "${HAS_INTEL_GPU}" == "true" ]]; then
        HAS_OPENVINO_GPU=true
    fi
    if [[ "${OV_DEVICES}" == *"CPU"* && "${HAS_INTEL_CPU}" == "true" ]]; then
        HAS_OPENVINO_CPU=true
    fi
fi

# Garantia de isolamento estrito contra AMD: OpenVINO nunca e associado a hardware AMD
if [[ "${HAS_AMD_CPU}" == "true" || "${HAS_INTEL_CPU}" == "false" ]]; then
    HAS_OPENVINO_CPU=false
fi
if [[ "${HAS_INTEL_GPU}" == "false" ]]; then
    HAS_OPENVINO_GPU=false
fi
if [[ "${HAS_INTEL_NPU}" == "false" ]]; then
    HAS_OPENVINO_NPU=false
fi

# ------------------------------------------------------------------------------
# Inspecao de Runtimes: AMD Ryzen AI NPU (XDNA) & AMD Radeon iGPU
# ------------------------------------------------------------------------------
HAS_AMD_NPU_RUNTIME=false
if ldconfig -p 2>/dev/null | grep -qi "libxrt_core\.so" || [[ -d /opt/xilinx/xrt || -e /usr/lib64/libxrt_core.so ]]; then
    HAS_AMD_NPU_RUNTIME=true
fi

HAS_AMD_GPU_RUNTIME=false
if ldconfig -p 2>/dev/null | grep -qiE "libvulkan_radeon\.so|amdvlk|libMesaOpenCL\.so" || [[ -d /opt/rocm || -e /usr/lib64/libvulkan_radeon.so ]]; then
    HAS_AMD_GPU_RUNTIME=true
fi

# ------------------------------------------------------------------------------
# Resolucao Dinamica do Backend Escolhido pelo AUTO
# ------------------------------------------------------------------------------
AUTO_ID="cpu_tract"
AUTO_NAME="CPU Nativo (Tract Pure-Rust)"

if [[ "${HAS_NVIDIA}" == "true" && "${HAS_TRT_RUNTIME}" == "true" ]]; then
    AUTO_ID="nvidia_tensorrt"
    AUTO_NAME="NVIDIA GPU (TensorRT / CUDA)"
elif [[ "${HAS_INTEL_NPU}" == "true" && "${HAS_OPENVINO_NPU}" == "true" ]]; then
    AUTO_ID="openvino_npu"
    AUTO_NAME="Intel OpenVINO (NPU - AI Boost)"
elif [[ "${HAS_AMD_NPU}" == "true" && "${HAS_AMD_NPU_RUNTIME}" == "true" ]]; then
    AUTO_ID="amd_ryzenai_npu"
    AUTO_NAME="AMD Ryzen AI (NPU - XDNA)"
elif [[ "${HAS_INTEL_GPU}" == "true" && "${HAS_OPENVINO_GPU}" == "true" ]]; then
    AUTO_ID="openvino_gpu"
    AUTO_NAME="Intel OpenVINO (iGPU - Intel Graphics)"
elif [[ "${HAS_AMD_GPU}" == "true" && "${HAS_AMD_GPU_RUNTIME}" == "true" ]]; then
    AUTO_ID="amd_ryzenai_gpu"
    AUTO_NAME="AMD Radeon (iGPU - RDNA Graphics)"
elif [[ "${HAS_INTEL_CPU}" == "true" && "${HAS_OPENVINO_CPU}" == "true" ]]; then
    AUTO_ID="openvino_cpu"
    AUTO_NAME="Intel OpenVINO (CPU - Otimizado)"
else
    AUTO_ID="cpu_tract"
    AUTO_NAME="CPU Nativo (Tract Pure-Rust)"
fi

# ------------------------------------------------------------------------------
# Emissao de JSON Estruturado para Frontend / IPC
# ------------------------------------------------------------------------------
if [[ "${OUTPUT_JSON}" == "true" ]]; then
    cat <<EOF
{
  "auto_resolved_backend": {
    "id": "${AUTO_ID}",
    "name": "${AUTO_NAME}"
  },
  "backends": [
    {
      "id": "auto",
      "name": "Automático (Melhor Acelerador)",
      "tier": "Auto",
      "hardware_detected": true,
      "runtime_installed": true,
      "device_info": "Seleção dinâmica inteligente do acelerador de menor latência",
      "runtime_name": "Agendador Automático ClearCore",
      "auto_resolved_id": "${AUTO_ID}",
      "auto_resolved_name": "${AUTO_NAME}",
      "install_script": "",
      "install_command": "",
      "install_instruction": ""
    },
    {
      "id": "nvidia_tensorrt",
      "name": "NVIDIA GPU (TensorRT / CUDA)",
      "tier": "DedicatedGpu",
      "hardware_detected": ${HAS_NVIDIA},
      "runtime_installed": ${HAS_TRT_RUNTIME},
      "device_info": "NVIDIA RTX PRO 1000 Blackwell Generation Laptop GPU",
      "runtime_name": "NVIDIA TensorRT (libnvinfer.so)",
      "install_script": "./scripts/install-tensorrt.sh",
      "install_command": "./scripts/install-tensorrt.sh",
      "install_instruction": "A GPU física NVIDIA foi detectada. Execute o script para compilar e registrar os módulos CUDA/TensorRT para aceleração máxima."
    },
    {
      "id": "openvino_npu",
      "name": "Intel OpenVINO (NPU - AI Boost)",
      "tier": "Npu",
      "hardware_detected": ${HAS_INTEL_NPU},
      "runtime_installed": ${HAS_OPENVINO_NPU},
      "device_info": $(if [[ "${HAS_INTEL_NPU}" == "true" ]]; then echo "\"Intel(R) AI Boost (NPU Neural dedicada no SoC - ultrabaixo consumo)\""; else echo "\"NPU Intel AI Boost não encontrada neste sistema.\""; fi),
      "runtime_name": "OpenVINO NPU Plugin (libopenvino_intel_npu_plugin.so)",
      "install_script": "./scripts/install-openvino.sh",
      "install_command": "./scripts/install-openvino.sh",
      "install_instruction": "A NPU Intel foi detectada no processador Core Ultra. Instale o OpenVINO e o driver intel-npu-driver para acelerar a rede neural sem impacto na bateria."
    },
    {
      "id": "openvino_gpu",
      "name": "Intel OpenVINO (iGPU - Intel Graphics)",
      "tier": "IntegratedGpu",
      "hardware_detected": ${HAS_INTEL_GPU},
      "runtime_installed": ${HAS_OPENVINO_GPU},
      "device_info": $(if [[ "${HAS_INTEL_GPU}" == "true" ]]; then echo "\"GPU Integrada Intel Arc / Graphics\""; else echo "\"GPU Integrada Intel não encontrada neste sistema.\""; fi),
      "runtime_name": "OpenVINO GPU Plugin (libopenvino_intel_gpu_plugin.so)",
      "install_script": "./scripts/install-openvino.sh",
      "install_command": "./scripts/install-openvino.sh",
      "install_instruction": "A GPU integrada Intel foi detectada. Instale o compute-runtime OpenCL/oneAPI e o OpenVINO para processar em GPU paralela."
    },
    {
      "id": "openvino_cpu",
      "name": "Intel OpenVINO (CPU - Otimizado)",
      "tier": "Cpu",
      "hardware_detected": ${HAS_INTEL_CPU},
      "runtime_installed": $(if [[ "${HAS_INTEL_CPU}" == "true" ]]; then echo "${HAS_OPENVINO_CPU}"; else echo "false"; fi),
      "device_info": $(if [[ "${HAS_INTEL_CPU}" == "true" ]]; then echo "\"${CPU_MODEL} (Aceleração vetorial AVX2 / AMX / VNNI)\""; else echo "\"Incompatível: Processador AMD detectado (${CPU_MODEL}). OpenVINO é exclusivo para hardware Intel.\""; fi),
      "runtime_name": "OpenVINO CPU Plugin (libopenvino_intel_cpu_plugin.so)",
      "install_script": "./scripts/install-openvino.sh",
      "install_command": "./scripts/install-openvino.sh",
      "install_instruction": "Otimizações vetoriais avançadas da Intel para CPU com o compilador OpenVINO (exclusivo para processadores Intel)."
    },
    {
      "id": "amd_ryzenai_npu",
      "name": "AMD Ryzen AI (NPU - XDNA)",
      "tier": "Npu",
      "hardware_detected": ${HAS_AMD_NPU},
      "runtime_installed": ${HAS_AMD_NPU_RUNTIME},
      "device_info": "AMD Ryzen AI NPU (XDNA / XDNA 2 dedicada no processador)",
      "runtime_name": "Ryzen AI Software (libxrt_core.so)",
      "install_script": "./scripts/install-ryzenai.sh",
      "install_command": "./scripts/install-ryzenai.sh",
      "install_instruction": "A NPU AMD Ryzen AI requer o driver amdxdna e o pacote Ryzen AI Software / XRT para processamento neural."
    },
    {
      "id": "amd_ryzenai_gpu",
      "name": "AMD Radeon (iGPU - RDNA Graphics)",
      "tier": "IntegratedGpu",
      "hardware_detected": ${HAS_AMD_GPU},
      "runtime_installed": ${HAS_AMD_GPU_RUNTIME},
      "device_info": $(if [[ "${HAS_AMD_GPU}" == "true" || "${HAS_AMD_CPU}" == "true" ]]; then echo "\"${CPU_MODEL} (Gráficos AMD Radeon)\""; else echo "\"GPU AMD Radeon não detectada.\""; fi),
      "runtime_name": "AMD ROCm / Vulkan / DirectML",
      "install_script": "./scripts/install-ryzenai.sh",
      "install_command": "./scripts/install-ryzenai.sh",
      "install_instruction": "Instale os drivers gráficos AMD e a runtime Vulkan/ROCm para acelerar na GPU integrada Radeon."
    },
    {
      "id": "apple_coreml",
      "name": "Apple Silicon (CoreML)",
      "tier": "Npu",
      "hardware_detected": false,
      "runtime_installed": false,
      "device_info": "Exclusivo para computadores Apple Mac com chip Apple Silicon (M1/M2/M3/M4)",
      "runtime_name": "Apple Neural Engine (CoreML)",
      "install_script": "",
      "install_command": "",
      "install_instruction": "Requer sistema operacional macOS com processador Apple Silicon."
    },
    {
      "id": "cpu_tract",
      "name": "CPU Nativo (Tract Pure-Rust)",
      "tier": "Cpu",
      "hardware_detected": true,
      "runtime_installed": true,
      "device_info": "${CPU_MODEL} (Execução Baseline Fail-Safe em Rust puro)",
      "runtime_name": "Tract (Embarcado, Zero Dependências)",
      "install_script": "",
      "install_command": "",
      "install_instruction": "Mecanismo baseline sempre disponível com 100% de segurança em Rust (#![forbid(unsafe_code)])."
    }
  ]
}
EOF
    exit 0
fi

echo "[3/3] Relatorio de Auditoria de Hardware & Runtimes:"
echo "----------------------------------------------------------"
echo "  [SELECAO AUTO ATUAL] ${AUTO_NAME} (${AUTO_ID})"
echo "----------------------------------------------------------"

# NVIDIA
if [[ "${HAS_NVIDIA}" == "true" ]]; then
    echo "  [DETECTADO] GPU Dedicada NVIDIA (Blackwell/Ada/Ampere)"
    if [[ "${HAS_TRT_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (TensorRT ativo: ${TRT_PATH})"
    else
        echo "              Status Runtime: AUSENTE (TensorRT nao encontrado)"
        echo "              -> ACAO: Execute './scripts/install-tensorrt.sh' para ativar TensorRT!"
    fi
else
    echo "  [NAO DETECTADO] GPU Dedicada NVIDIA"
fi

# Intel OpenVINO NPU
if [[ "${HAS_INTEL_NPU}" == "true" ]]; then
    echo "  [DETECTADO] Intel NPU (Core Ultra 200H / Arrow Lake - AI Boost)"
    if [[ "${HAS_OPENVINO_NPU}" == "true" ]]; then
        echo "              Status Runtime: Instalada (OpenVINO NPU Plugin ativo)"
    else
        echo "              Status Runtime: AUSENTE (Plugin NPU nao encontrado)"
        echo "              -> ACAO: Execute './scripts/install-openvino.sh' para ativar NPU Intel!"
    fi
fi

# Intel OpenVINO iGPU
if [[ "${HAS_INTEL_GPU}" == "true" ]]; then
    echo "  [DETECTADO] Intel iGPU (Arrow Lake-P Graphics)"
    if [[ "${HAS_OPENVINO_GPU}" == "true" ]]; then
        echo "              Status Runtime: Instalada (OpenVINO GPU Plugin ativo)"
    fi
fi

# Intel OpenVINO CPU
if [[ "${HAS_INTEL_CPU}" == "true" ]]; then
    echo "  [DETECTADO] Intel CPU (Core Ultra 7 265H - AVX2 / AMX / VNNI)"
    if [[ "${HAS_OPENVINO_CPU}" == "true" ]]; then
        echo "              Status Runtime: Instalada (OpenVINO CPU Plugin ativo)"
    fi
fi

# AMD Ryzen AI NPU
if [[ "${HAS_AMD_NPU}" == "true" ]]; then
    echo "  [DETECTADO] AMD Ryzen AI NPU (XDNA / XDNA 2)"
    if [[ "${HAS_AMD_NPU_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (Ryzen AI Software ativo)"
    else
        echo "              Status Runtime: AUSENTE (XRT / Vitis-AI nao encontrado)"
        echo "              -> ACAO: Execute './scripts/install-ryzenai.sh' para ativar Ryzen AI!"
    fi
fi

# AMD Radeon iGPU
if [[ "${HAS_AMD_GPU}" == "true" ]]; then
    echo "  [DETECTADO] AMD Radeon iGPU (RDNA Graphics)"
    if [[ "${HAS_AMD_GPU_RUNTIME}" == "true" ]]; then
        echo "              Status Runtime: Instalada (Vulkan / ROCm ativo)"
    fi
fi

echo "  [SEMPRE ATIVO] CPU Tract Baseline (100% Rust Seguro, Fail-Closed)"
echo "----------------------------------------------------------"
echo "Hierarquia de Selecao:"
echo "1. GPU Dedicada (TensorRT 100)"
echo "2. NPU (Intel OpenVINO NPU / AMD Ryzen AI NPU 80)"
echo "3. GPU Integrada (Intel OpenVINO iGPU / AMD Radeon iGPU 70)"
echo "4. CPU (Intel OpenVINO CPU 60 -> Tract Rust Baseline 50)"
echo "=========================================================="
