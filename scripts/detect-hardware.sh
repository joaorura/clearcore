OUTPUT_JSON=false
if [[ "${1:-}" == "--json" ]]; then
    OUTPUT_JSON=true
fi

if [[ "${OUTPUT_JSON}" == "false" ]]; then
    echo "=========================================================="
    echo " Clearcore / Hippocamp - Hardware & Runtime Detector"
    echo "=========================================================="
    echo "[1/3] Verificando barramento PCI e dispositivos de aceleracao..."
fi

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

if [[ "${OUTPUT_JSON}" == "false" ]]; then
    echo "[2/3] Verificando presenca das runtimes no sistema..."
fi

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
      "device_info": "Seleção dinâmica por prioridade de hardware e disponibilidade",
      "runtime_name": "Agendador Automático",
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
      "device_info": "GPU Dedicada NVIDIA (Blackwell / Ada / Ampere)",
      "runtime_name": "NVIDIA TensorRT (libnvinfer.so)",
      "install_script": "./scripts/install-tensorrt.sh",
      "install_command": "./scripts/install-tensorrt.sh",
      "install_instruction": "A GPU física NVIDIA foi detectada, mas a biblioteca de aceleração TensorRT não foi encontrada no sistema. Execute o script de instalação para compilar e registrar os módulos CUDA/TensorRT de altíssimo desempenho."
    },
    {
      "id": "intel_openvino",
      "name": "Intel NPU / Arc GPU (OpenVINO)",
      "tier": "Npu",
      "hardware_detected": ${HAS_INTEL_NPU},
      "runtime_installed": ${HAS_OPENVINO_RUNTIME},
      "device_info": "Intel NPU (Core Ultra 200H / Arrow Lake) / Arc GPU",
      "runtime_name": "Intel OpenVINO (libopenvino.so)",
      "install_script": "./scripts/install-openvino.sh",
      "install_command": "./scripts/install-openvino.sh",
      "install_instruction": "A NPU Intel foi detectada no processador Core Ultra. Instale o runtime OpenVINO para processar a rede neural na NPU dedicada com zero impacto na CPU e bateria."
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
      "device_info": "Disponível exclusivamente em computadores Apple Mac com chip Apple Silicon (M1/M2/M3/M4)",
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
      "device_info": "Processador Host x86_64 / ARM (Execução Nativa Segura)",
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
