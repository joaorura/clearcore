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
#   - Apple Silicon (CoreML)
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

# 4. Intel CPU
if grep -qi "intel" /proc/cpuinfo 2>/dev/null; then
    HAS_INTEL_CPU=true
fi

# 5. AMD NPU & GPU
if echo "${PCI_INFO}" | grep -qi "1022:1502" || [[ -e /dev/amdxdna ]]; then
    HAS_AMD_NPU=true
fi
if echo "${PCI_INFO}" | grep -i "1002:" | grep -qiE 'vga|display|graphics|\[030'; then
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

# Plugin NPU
for p in /usr/lib64/openvino*/libopenvino_intel_npu_plugin.so /lib64/openvino*/libopenvino_intel_npu_plugin.so /usr/lib/x86_64-linux-gnu/openvino*/libopenvino_intel_npu_plugin.so; do
    if [[ -e "${p}" ]]; then
        HAS_OPENVINO_NPU=true
        break
    fi
done

# Plugin GPU (iGPU/dGPU)
for p in /usr/lib64/openvino*/libopenvino_intel_gpu_plugin.so /lib64/openvino*/libopenvino_intel_gpu_plugin.so /usr/lib/x86_64-linux-gnu/openvino*/libopenvino_intel_gpu_plugin.so; do
    if [[ -e "${p}" ]]; then
        HAS_OPENVINO_GPU=true
        break
    fi
done

# Plugin CPU
for p in /usr/lib64/openvino*/libopenvino_intel_cpu_plugin.so /lib64/openvino*/libopenvino_intel_cpu_plugin.so /usr/lib/x86_64-linux-gnu/openvino*/libopenvino_intel_cpu_plugin.so; do
    if [[ -e "${p}" ]]; then
        HAS_OPENVINO_CPU=true
        break
    fi
done

# Inspecao rapida de dispositivos disponiveis via OpenVINO se biblioteca ativa
OV_DEVICES=""
if [[ "${HAS_OPENVINO_BASE}" == "true" ]]; then
    OV_DEVICES=$(python3 -c "import openvino as ov; print(','.join(ov.Core().available_devices))" 2>/dev/null || \
                 /home/joaorura/miniconda3/bin/python3 -c "import openvino as ov; print(','.join(ov.Core().available_devices))" 2>/dev/null || \
                 /home/joaorura/.local/share/whisper-ov/.venv/bin/python3 -c "import openvino as ov; print(','.join(ov.Core().available_devices))" 2>/dev/null || true)
    if [[ "${OV_DEVICES}" == *"NPU"* ]]; then
        HAS_OPENVINO_NPU=true
    fi
    if [[ "${OV_DEVICES}" == *"GPU"* ]]; then
        HAS_OPENVINO_GPU=true
    fi
    if [[ "${OV_DEVICES}" == *"CPU"* ]]; then
        HAS_OPENVINO_CPU=true
    fi
fi

# ------------------------------------------------------------------------------
# Inspecao de Runtimes: AMD Ryzen AI (XDNA)
# ------------------------------------------------------------------------------
HAS_RYZENAI_RUNTIME=false
if ldconfig -p 2>/dev/null | grep -q "libxrt_core\.so" || [[ -d /opt/xilinx/xrt ]]; then
    HAS_RYZENAI_RUNTIME=true
fi

# ------------------------------------------------------------------------------
# Emissao de JSON Estruturado para Frontend / IPC
# ------------------------------------------------------------------------------
if [[ "${OUTPUT_JSON}" == "true" ]]; then
    cat <<EOF
{
  "backends": [
    {
      "id": "auto",
      "name": "Automático (Melhor Acelerador)",
      "tier": "Auto",
      "hardware_detected": true,
      "runtime_installed": true,
      "device_info": "Seleção dinâmica inteligente do acelerador de menor latência",
      "runtime_name": "Agendador Automático ClearCore",
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
      "device_info": "Intel(R) AI Boost (NPU Neural dedicada no SoC - ultrabaixo consumo)",
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
      "device_info": "Intel Arrow Lake-P Graphics (GPU integrada de alta largura de banda)",
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
      "runtime_installed": ${HAS_OPENVINO_CPU},
      "device_info": "Intel Core Ultra 7 265H (Aceleração vetorial AVX2 / AMX / VNNI)",
      "runtime_name": "OpenVINO CPU Plugin (libopenvino_intel_cpu_plugin.so)",
      "install_script": "./scripts/install-openvino.sh",
      "install_command": "./scripts/install-openvino.sh",
      "install_instruction": "Otimizações vetoriais avançadas da Intel para CPU com o compilador OpenVINO."
    },
    {
      "id": "amd_ryzenai",
      "name": "AMD Ryzen AI NPU (XDNA)",
      "tier": "Npu",
      "hardware_detected": ${HAS_AMD_NPU},
      "runtime_installed": ${HAS_RYZENAI_RUNTIME},
      "device_info": "AMD Ryzen AI NPU (XDNA / XDNA 2)",
      "runtime_name": "Ryzen AI Software (libxrt_core.so)",
      "install_script": "./scripts/install-ryzenai.sh",
      "install_command": "./scripts/install-ryzenai.sh",
      "install_instruction": "Hardware AMD Ryzen AI NPU não encontrado ou driver XRT ausente."
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
      "device_info": "Processador Host x86_64 / ARM (Execução Baseline Fail-Safe em Rust puro)",
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

echo "  [SEMPRE ATIVO] CPU Tract Baseline (100% Rust Seguro, Fail-Closed)"
echo "----------------------------------------------------------"
echo "Hierarquia de Selecao:"
echo "1. GPU Dedicada (TensorRT 100)"
echo "2. NPU (Intel OpenVINO NPU / AMD Ryzen AI NPU 80)"
echo "3. GPU Integrada (Intel OpenVINO iGPU 70)"
echo "4. CPU (Intel OpenVINO CPU 60 -> Tract Rust Baseline 50)"
echo "=========================================================="
