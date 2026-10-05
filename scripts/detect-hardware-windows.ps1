# ==============================================================================
# Clearcore / Hippocamp - Detector de Hardware e Runtimes de IA (Windows)
# ==============================================================================
# Inspeciona dispositivos fisicos e runtimes no Windows:
#   - NVIDIA Dedicated GPU (TensorRT / CUDA)
#   - Intel OpenVINO NPU (Intel AI Boost / NPU Integrada)
#   - Intel OpenVINO iGPU (Intel Arc / Graphics)
#   - Intel OpenVINO CPU (Intel Core Ultra AVX2 / AMX / VNNI)
#   - AMD Ryzen AI NPU (XDNA / XDNA 2)
#   - AMD Radeon iGPU (RDNA Graphics)
#   - Apple Silicon (CoreML)
#   - CPU Nativo Baseline (Tract Pure-Rust Fail-Safe)
# ==============================================================================

[CmdletBinding()]
param(
    [switch]$Json,
    [switch]$Status
)

$ErrorActionPreference = "SilentlyContinue"

# ------------------------------------------------------------------------------
# 1. Deteccao de Hardware Fisico (WMI / PnP)
# ------------------------------------------------------------------------------
$hasNvidia = $false
$hasIntelNpu = $false
$hasIntelGpu = $false
$hasIntelCpu = $false
$hasAmdNpu = $false
$hasAmdGpu = $false

$nvidiaName = "NVIDIA Dedicated GPU"
$intelGpuName = "Intel Arc / Iris Xe Graphics"
$amdGpuName = "AMD Radeon Graphics"

$videoControllers = Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue
foreach ($vc in $videoControllers) {
    $pnpId = $vc.PNPDeviceID
    $name = $vc.Name
    if ($pnpId -match "VEN_10DE" -or $name -match "NVIDIA") {
        $hasNvidia = $true
        $nvidiaName = $name
    }
    if ($pnpId -match "VEN_8086" -or $name -match "Intel") {
        $hasIntelGpu = $true
        $intelGpuName = $name
    }
    if ($pnpId -match "VEN_1002" -or $name -match "AMD|Radeon") {
        $hasAmdGpu = $true
        $amdGpuName = $name
    }
}

$hasAmdCpu = $false
$proc = Get-CimInstance Win32_Processor -ErrorAction SilentlyContinue | Select-Object -First 1
if ($proc) {
    if ($proc.Name -match "Intel") {
        $hasIntelCpu = $true
        $hasAmdCpu = $false
    } elseif ($proc.Name -match "AMD|Ryzen") {
        $hasAmdCpu = $true
        $hasIntelCpu = $false
        # AMD APUs typically include Radeon integrated graphics
        $hasAmdGpu = $true
    }
}

$pnpDevices = Get-PnpDevice -ErrorAction SilentlyContinue
foreach ($dev in $pnpDevices) {
    $inst = $dev.InstanceId
    $friendly = $dev.FriendlyName
    if ($inst -match "VEN_8086&DEV_7D1D" -or $friendly -match "Intel.*AI Boost|Intel.*NPU") {
        $hasIntelNpu = $true
    }
    if ($inst -match "VEN_1022&DEV_1502|VEN_1022&DEV_17F0" -or $friendly -match "AMD.*NPU|Ryzen AI") {
        $hasAmdNpu = $true
    }
}

# ------------------------------------------------------------------------------
# 2. Deteccao de Runtimes no Windows
# ------------------------------------------------------------------------------
$hasTrtRuntime = $false
$trtPath = ""

$cudaPaths = @(
    "C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA\*\bin\nvinfer*.dll",
    "C:\Program Files\NVIDIA Corporation\TensorRT*\lib\nvinfer*.dll"
)
foreach ($cp in $cudaPaths) {
    $found = Get-ChildItem -Path $cp -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) {
        $hasTrtRuntime = $true
        $trtPath = $found.FullName
        break
    }
}

if (-not $hasTrtRuntime) {
    if (Get-Command "nvinfer.dll" -ErrorAction SilentlyContinue) {
        $hasTrtRuntime = $true
        $trtPath = "PATH:nvinfer.dll"
    }
}

# OpenVINO Runtimes (Only checked if Intel hardware is present; never on AMD)
$hasOpenVinoNpu = $false
$hasOpenVinoGpu = $false
$hasOpenVinoCpu = $false

if ($hasIntelCpu -or $hasIntelGpu -or $hasIntelNpu) {
    $ovPaths = @(
        "C:\Program Files (x86)\Intel\openvino*\runtime\bin\intel64\openvino.dll",
        "C:\openvino\runtime\bin\intel64\openvino.dll"
    )
    foreach ($op in $ovPaths) {
        if (Test-Path $op) {
            if ($hasIntelCpu) { $hasOpenVinoCpu = $true }
            if ($hasIntelGpu) { $hasOpenVinoGpu = $true }
            break
        }
    }

    $npuDrv = Get-Service "intel-npu-driver" -ErrorAction SilentlyContinue
    if ($hasIntelNpu -and ($npuDrv -or $hasOpenVinoCpu)) {
        $hasOpenVinoNpu = $true
    }
}

# AMD Runtimes
$hasAmdNpuRuntime = $false
if (Test-Path "C:\Program Files\RyzenAI\*\bin\xrt_core.dll" -or (Get-Service "amdxdna" -ErrorAction SilentlyContinue)) {
    $hasAmdNpuRuntime = $true
}

$hasAmdGpuRuntime = $false
if (Test-Path "$env:SystemRoot\System32\DirectML.dll" -or (Test-Path "$env:SystemRoot\System32\vulkan-1.dll")) {
    $hasAmdGpuRuntime = $true
}

# ------------------------------------------------------------------------------
# 3. Resolucao Automatica (AUTO Selection)
# ------------------------------------------------------------------------------
$autoId = "cpu_tract"
$autoName = "CPU Nativo (Tract Pure-Rust)"

if ($hasNvidia -and $hasTrtRuntime) {
    $autoId = "nvidia_tensorrt"
    $autoName = "NVIDIA GPU (TensorRT / CUDA)"
} elseif ($hasIntelNpu -and $hasOpenVinoNpu) {
    $autoId = "openvino_npu"
    $autoName = "Intel OpenVINO (NPU - AI Boost)"
} elseif ($hasAmdNpu -and $hasAmdNpuRuntime) {
    $autoId = "amd_ryzenai_npu"
    $autoName = "AMD Ryzen AI (NPU - XDNA)"
} elseif ($hasIntelGpu -and $hasOpenVinoGpu) {
    $autoId = "openvino_gpu"
    $autoName = "Intel OpenVINO (iGPU - Intel Graphics)"
} elseif ($hasAmdGpu -and $hasAmdGpuRuntime) {
    $autoId = "amd_ryzenai_gpu"
    $autoName = "AMD Radeon (iGPU - RDNA Graphics)"
} elseif ($hasIntelCpu -and $hasOpenVinoCpu) {
    $autoId = "openvino_cpu"
    $autoName = "Intel OpenVINO (CPU - Otimizado)"
} else {
    $autoId = "cpu_tract"
    $autoName = "CPU Nativo (Tract Pure-Rust)"
}

# ------------------------------------------------------------------------------
# 4. Saida JSON ou Relatorio Formatado
# ------------------------------------------------------------------------------
if ($Json -or (-not $Status)) {
    $payload = @{
        auto_resolved_backend = @{
            id = $autoId
            name = $autoName
        }
        backends = @(
            @{
                id = "auto"
                name = "Automático (Melhor Acelerador)"
                tier = "Auto"
                hardware_detected = $true
                runtime_installed = $true
                device_info = "Seleção dinâmica inteligente do acelerador de menor latência"
                runtime_name = "Agendador Automático ClearCore"
                auto_resolved_id = $autoId
                auto_resolved_name = $autoName
                install_script = ""
                install_command = ""
                install_instruction = ""
            },
            @{
                id = "nvidia_tensorrt"
                name = "NVIDIA GPU (TensorRT / CUDA)"
                tier = "DedicatedGpu"
                hardware_detected = [bool]$hasNvidia
                runtime_installed = [bool]$hasTrtRuntime
                device_info = $nvidiaName
                runtime_name = "NVIDIA TensorRT (nvinfer.dll)"
                install_script = ""
                install_command = "pip install tensorrt"
                install_instruction = "GPU NVIDIA detectada. Instale o NVIDIA CUDA Toolkit e o pacote TensorRT ('pip install tensorrt' ou pelo instalador da NVIDIA). Guia oficial: https://docs.nvidia.com/deeplearning/tensorrt/install-guide/index.html"
            },
            @{
                id = "openvino_npu"
                name = "Intel OpenVINO (NPU - AI Boost)"
                tier = "Npu"
                hardware_detected = [bool]$hasIntelNpu
                runtime_installed = [bool]$hasOpenVinoNpu
                device_info = "Intel(R) AI Boost (NPU Neural dedicada no SoC)"
                runtime_name = "OpenVINO NPU Plugin (openvino_intel_npu_plugin.dll)"
                install_script = ""
                install_command = "pip install openvino"
                install_instruction = "NPU Intel AI Boost detectada. Instale o driver Intel NPU (via Windows Update ou Intel DSA) e o pacote 'pip install openvino'. Documentação: https://docs.openvino.ai/"
            },
            @{
                id = "openvino_gpu"
                name = "Intel OpenVINO (iGPU - Intel Graphics)"
                tier = "IntegratedGpu"
                hardware_detected = [bool]$hasIntelGpu
                runtime_installed = [bool]$hasOpenVinoGpu
                device_info = $intelGpuName
                runtime_name = "OpenVINO GPU Plugin (openvino_intel_gpu_plugin.dll)"
                install_script = ""
                install_command = "pip install openvino"
                install_instruction = "GPU integrada Intel Arc / Iris Xe detectada. Mantenha os drivers gráficos atualizados e instale 'pip install openvino'. Documentação: https://docs.openvino.ai/"
            },
            @{
                id = "openvino_cpu"
                name = "Intel OpenVINO (CPU - Otimizado)"
                tier = "Cpu"
                hardware_detected = [bool]$hasIntelCpu
                runtime_installed = [bool]($hasIntelCpu -and $hasOpenVinoCpu)
                device_info = if ($hasIntelCpu) { if ($proc) { $proc.Name } else { "Processador Intel Host" } } else { "Incompatível: Processador AMD detectado ($($proc.Name)). OpenVINO requer processador Intel." }
                runtime_name = "OpenVINO CPU Plugin (openvino_intel_cpu_plugin.dll)"
                install_script = ""
                install_command = "pip install openvino"
                install_instruction = "Processador Intel detectado. Instale 'pip install openvino' para habilitar aceleração vetorial Intel AVX2/AMX na CPU. Documentação: https://docs.openvino.ai/"
            },
            @{
                id = "amd_ryzenai_npu"
                name = "AMD Ryzen AI (NPU - XDNA)"
                tier = "Npu"
                hardware_detected = [bool]$hasAmdNpu
                runtime_installed = [bool]$hasAmdNpuRuntime
                device_info = "AMD Ryzen AI NPU (XDNA / XDNA 2 dedicada)"
                runtime_name = "Ryzen AI Software (xrt_core.dll)"
                install_script = ""
                install_command = "pip install ryzenai"
                install_instruction = "NPU AMD Ryzen AI detectada. Instale o driver AMD IPU/NPU e o instalador Ryzen AI Software. Documentação: https://ryzenai.docs.amd.com/"
            },
            @{
                id = "amd_ryzenai_gpu"
                name = "AMD Radeon (iGPU - RDNA Graphics)"
                tier = "IntegratedGpu"
                hardware_detected = [bool]$hasAmdGpu
                runtime_installed = [bool]$hasAmdGpuRuntime
                device_info = $amdGpuName
                runtime_name = "DirectML / Vulkan (DirectML.dll)"
                install_script = ""
                install_command = "winget install AMD.Adrenalin"
                install_instruction = "GPU AMD Radeon detectada. Instale os drivers oficiais AMD Adrenalin Edition para aceleração via DirectML e Vulkan. Documentação: https://www.amd.com/support"
            },
            @{
                id = "apple_coreml"
                name = "Apple Silicon (CoreML)"
                tier = "Npu"
                hardware_detected = $false
                runtime_installed = $false
                device_info = "Exclusivo para computadores Apple Mac com chip Apple Silicon"
                runtime_name = "Apple Neural Engine (CoreML)"
                install_script = ""
                install_command = ""
                install_instruction = "Requer computador Mac rodando macOS."
            },
            @{
                id = "cpu_tract"
                name = "CPU Nativo (Tract Pure-Rust)"
                tier = "Cpu"
                hardware_detected = $true
                runtime_installed = $true
                device_info = "Processador Host CPU (Execução Baseline Fail-Safe em Rust puro)"
                runtime_name = "Tract (Embarcado, Zero Dependências)"
                install_script = ""
                install_command = ""
                install_instruction = "Mecanismo baseline sempre disponível com 100% de segurança em Rust."
            }
        )
    }
    $payload | ConvertTo-Json -Depth 5
    exit 0
}

Write-Host "ClearCore - Detector de Hardware (Windows)"
Write-Host "Selecao AUTO Atual: $autoName ($autoId)"
