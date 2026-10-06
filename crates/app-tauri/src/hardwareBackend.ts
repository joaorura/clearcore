// Gerenciamento e regras do seletor de acelerador de hardware & IA.

/** Motor padrão baseline. */
export const ENGINE_BACKEND_ID = 'cpu_tract';

/** Backends suportados pelo ClearCore. */
export const SUPPORTED_BACKEND_IDS: readonly string[] = [
  'auto',
  'nvidia_tensorrt',
  'openvino_npu',
  'openvino_gpu',
  'openvino_cpu',
  'amd_ryzenai_npu',
  'amd_ryzenai_gpu',
  'apple_coreml',
  'cpu_tract',
];

export interface BackendLike {
  id: string;
  hardware_detected: boolean;
  runtime_installed: boolean;
}

export interface NamedBackend {
  id: string;
  name: string;
}

export function isBackendImplemented(id: string): boolean {
  return SUPPORTED_BACKEND_IDS.includes(id);
}

/** Acelerador não suportado ou em estágio de prévia. */
export function isPreviewBackend(id: string): boolean {
  return !isBackendImplemented(id);
}

/**
 * Um acelerador é selecionável se o hardware foi detectado e a runtime está instalada
 * (ou se for o modo automático 'auto').
 */
export function isBackendSelectable(backend: BackendLike): boolean {
  if (!isBackendImplemented(backend.id)) return false;
  return backend.id === 'auto' || (backend.hardware_detected && backend.runtime_installed);
}

/**
 * Identifica se a runtime para o hardware detectado está ausente / pendente de instalação.
 */
export function isRuntimePending(backend: BackendLike): boolean {
  return backend.hardware_detected && !backend.runtime_installed;
}

/**
 * Retorna informações do acelerador resolvido automaticamente quando aplicável.
 */
export function getDetectedUnusedAccelerator(
  autoResolved: NamedBackend | null | undefined,
): NamedBackend | null {
  if (!autoResolved || !autoResolved.id || autoResolved.id === 'auto') return null;
  return { id: autoResolved.id, name: autoResolved.name };
}

export type SelectionOutcome =
  | { kind: 'applied'; activeId: string }
  | { kind: 'rejected'; reason: string; activeId: string };

interface SetBackendResponse {
  success?: boolean;
  reason?: string;
  active_backend?: string;
}

/**
 * Interpreta a resposta de set_hardware_backend.
 */
export function interpretSelectionResult(requestedId: string, res: unknown): SelectionOutcome {
  const r = (res && typeof res === 'object' ? res : {}) as SetBackendResponse;
  if (!isBackendImplemented(requestedId)) {
    return { kind: 'rejected', reason: 'unknown_backend', activeId: 'auto' };
  }
  if (r.success === false) {
    return {
      kind: 'rejected',
      reason: r.reason || 'unknown',
      activeId: r.active_backend || 'auto',
    };
  }
  return { kind: 'applied', activeId: r.active_backend || requestedId };
}

export interface NativeCommandItem {
  id: 'ubuntu' | 'fedora' | 'arch' | 'python' | 'windows';
  label: string;
  command: string;
  note?: string;
}

export interface DiagnosticStepItem {
  title: string;
  command?: string;
  tip?: string;
}

export const NVIDIA_TENSORRT_DOCS_URL =
  'https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html';

export interface PromptContext {
  os?: string;
  deviceInfo?: string;
}

/**
 * Gera um prompt pronto para LLM solicitando passo a passo de instalação da runtime.
 */
export function getBackendLlmPrompt(
  backendId: string,
  context?: PromptContext
): string | null {
  const osStr = context?.os?.trim() || 'Linux/Windows';
  const hwStr =
    context?.deviceInfo && context.deviceInfo.trim().length > 0
      ? `Meu hardware possui: ${context.deviceInfo.trim()}.`
      : 'Meu hardware possui uma GPU NVIDIA.';

  switch (backendId) {
    case 'nvidia_tensorrt':
      return `Preciso instalar o runtime da biblioteca C/C++ da NVIDIA TensorRT (libnvinfer) no meu sistema operacional (${osStr}) para permitir inferência com aceleração por GPU em um aplicativo de áudio. ${hwStr} Por favor, me forneça o passo a passo oficial para instalar o TensorRT nativo com base na documentação oficial: ${NVIDIA_TENSORRT_DOCS_URL}`;
    case 'openvino_npu':
      return `Preciso instalar e configurar os drivers e o runtime da Intel OpenVINO NPU no meu sistema operacional (${osStr}). ${context?.deviceInfo ? `Meu hardware: ${context.deviceInfo}.` : 'Meu processador possui uma Intel NPU (AI Boost).'} Por favor, me forneça o passo a passo com base na documentação oficial: https://docs.openvino.ai/2025/get-started/configurations/configurations-intel-npu.html`;
    case 'openvino_gpu':
      return `Preciso instalar e configurar os drivers de computação da Intel GPU (OpenCL / compute-runtime) e OpenVINO no meu sistema operacional (${osStr}). ${context?.deviceInfo ? `Meu hardware: ${context.deviceInfo}.` : 'Possuo GPU Intel Arc / Iris Xe.'} Por favor, me forneça o passo a passo com base na documentação oficial: https://docs.openvino.ai/2025/get-started/configurations/configurations-intel-gpu.html`;
    case 'openvino_cpu':
      return `Preciso configurar as extensões de aceleração OpenVINO CPU no meu sistema operacional (${osStr}). ${context?.deviceInfo ? `Meu hardware: ${context.deviceInfo}.` : ''} Documentação oficial: https://docs.openvino.ai/`;
    case 'amd_ryzenai_npu':
      return `Preciso instalar os drivers AMD XDNA e o runtime AMD Ryzen AI Software / XRT no meu sistema operacional (${osStr}). ${context?.deviceInfo ? `Meu hardware: ${context.deviceInfo}.` : 'Possuo uma NPU AMD Ryzen AI.'} Documentação oficial: https://ryzenai.docs.amd.com/`;
    case 'amd_ryzenai_gpu':
      return `Preciso configurar a aceleração por GPU AMD Radeon (Vulkan / ROCm / DirectML) no meu sistema operacional (${osStr}). ${context?.deviceInfo ? `Meu hardware: ${context.deviceInfo}.` : 'Possuo GPU AMD Radeon.'} Documentação oficial: https://rocm.docs.amd.com/`;
    default:
      return null;
  }
}

export interface BackendHelpDetails {
  librarySearched: {
    linux: string;
    windows: string;
    description: string;
  };
  officialDocs: {
    title: string;
    url: string;
  };
  llmPrompt?: string;
  nativeCommands: NativeCommandItem[];
  diagnosticGuide: DiagnosticStepItem[];
}

export function getBackendHelpDetails(backendId: string): BackendHelpDetails | null {
  switch (backendId) {
    case 'nvidia_tensorrt':
      return {
        librarySearched: {
          linux: 'libnvinfer.so.10 / libnvinfer.so',
          windows: 'nvinfer.dll / nvinfer_10.dll',
          description: 'Buscada em /usr/lib64, /usr/lib/x86_64-linux-gnu, LD_LIBRARY_PATH e módulos Python tensorrt_libs.',
        },
        officialDocs: {
          title: 'NVIDIA TensorRT Installation Guide',
          url: NVIDIA_TENSORRT_DOCS_URL,
        },
        llmPrompt: getBackendLlmPrompt('nvidia_tensorrt') ?? undefined,
        nativeCommands: [
          {
            id: 'ubuntu',
            label: 'Ubuntu / Debian (APT)',
            command: 'sudo apt update && sudo apt install -y libnvinfer10 libnvonnxparsers10',
            note: 'Instalação nativa via repositório oficial da NVIDIA ou consulte o guia oficial: https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html',
          },
          {
            id: 'fedora',
            label: 'Fedora / RHEL (DNF)',
            command: 'sudo dnf install -y tensorrt || pip install tensorrt',
            note: 'Consulte o guia oficial da NVIDIA para instruções de instalação nativa: https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html',
          },
          {
            id: 'arch',
            label: 'Arch Linux (Pacman)',
            command: 'sudo pacman -S --needed tensorrt',
            note: 'Disponível no repositório extra ou consulte o guia oficial da NVIDIA: https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html',
          },
          {
            id: 'python',
            label: 'Python (pip / Universal)',
            command: 'pip install tensorrt',
            note: 'Instala o pacote pré-compilado oficial NVIDIA no ambiente Python.',
          },
          {
            id: 'windows',
            label: 'Windows',
            command: 'pip install tensorrt',
            note: 'Baixe o zip/instalador oficial no NVIDIA Developer ou consulte: https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html',
          },
        ],
        diagnosticGuide: [
          {
            title: 'Verifique se o driver NVIDIA e GPU estão operacionais:',
            command: 'nvidia-smi',
            tip: 'Deve exibir sua placa física NVIDIA e a versão do driver CUDA instalada.',
          },
          {
            title: 'Verifique se libnvinfer está no cache dinâmico de bibliotecas:',
            command: 'ldconfig -p | grep libnvinfer',
            tip: 'Deve retornar libnvinfer.so.10 ou libnvinfer.so no sistema.',
          },
          {
            title: 'Valide a inicialização no Python:',
            command: 'python3 -c "import tensorrt as trt; print(trt.__version__)"',
            tip: 'Deve imprimir a versão oficial do TensorRT (ex: 10.x.x).',
          },
        ],
      };

    case 'openvino_npu':
      return {
        librarySearched: {
          linux: 'libopenvino_intel_npu_plugin.so (e libopenvino.so)',
          windows: 'openvino_intel_npu_plugin.dll (e openvino.dll)',
          description: 'Buscada em /usr/lib64/openvino*, /opt/intel/openvino*, LD_LIBRARY_PATH e pacotes Python.',
        },
        officialDocs: {
          title: 'Intel OpenVINO - NPU Configuration Guide',
          url: 'https://docs.openvino.ai/2025/get-started/configurations/configurations-intel-npu.html',
        },
        nativeCommands: [
          {
            id: 'ubuntu',
            label: 'Ubuntu / Debian (APT)',
            command: 'sudo apt update && sudo apt install -y intel-npu-driver openvino || pip install openvino',
            note: 'Instala o driver do kernel intel-npu e o runtime neural OpenVINO.',
          },
          {
            id: 'fedora',
            label: 'Fedora / RHEL (DNF)',
            command: 'sudo dnf install -y intel-npu-driver openvino || pip install openvino',
            note: 'Disponível nativamente no Fedora ou via pip.',
          },
          {
            id: 'arch',
            label: 'Arch Linux (Pacman)',
            command: 'sudo pacman -S --needed intel-npu-driver-bin openvino || pip install openvino',
            note: 'Driver disponível no AUR e openvino no repositório extra.',
          },
          {
            id: 'python',
            label: 'Python (pip / Universal)',
            command: 'pip install openvino',
            note: 'Pacote oficial OpenVINO com backend NPU habilitado.',
          },
          {
            id: 'windows',
            label: 'Windows',
            command: 'pip install openvino',
            note: 'Instale o Intel NPU Driver via Windows Update ou Intel DSA.',
          },
        ],
        diagnosticGuide: [
          {
            title: 'Verifique se o dispositivo da NPU está exposto no barramento de aceleração:',
            command: 'ls -la /dev/accel/accel*',
            tip: 'Deve listar /dev/accel/accel0 com vendor Intel (0x8086).',
          },
          {
            title: 'Permita acesso do seu usuário sem permissões de root:',
            command: 'sudo usermod -aG render $USER',
            tip: 'Concede acesso ao grupo de aceleração gráfica/neural.',
          },
          {
            title: 'Verifique se o OpenVINO reconhece a NPU no sistema:',
            command: 'python3 -c "import openvino as ov; print(ov.Core().available_devices)"',
            tip: 'Deve incluir "NPU" na lista de dispositivos disponíveis.',
          },
        ],
      };

    case 'openvino_gpu':
      return {
        librarySearched: {
          linux: 'libopenvino_intel_gpu_plugin.so',
          windows: 'openvino_intel_gpu_plugin.dll',
          description: 'Plugin de computação paralela para GPU integrada Intel Arc e Iris Xe.',
        },
        officialDocs: {
          title: 'Intel OpenVINO - GPU Configuration Guide',
          url: 'https://docs.openvino.ai/2025/get-started/configurations/configurations-intel-gpu.html',
        },
        nativeCommands: [
          {
            id: 'ubuntu',
            label: 'Ubuntu / Debian (APT)',
            command: 'sudo apt update && sudo apt install -y intel-opencl-icd openvino || pip install openvino',
          },
          {
            id: 'fedora',
            label: 'Fedora / RHEL (DNF)',
            command: 'sudo dnf install -y intel-compute-runtime openvino || pip install openvino',
          },
          {
            id: 'arch',
            label: 'Arch Linux (Pacman)',
            command: 'sudo pacman -S --needed intel-compute-runtime openvino || pip install openvino',
          },
          {
            id: 'python',
            label: 'Python (pip / Universal)',
            command: 'pip install openvino',
          },
          {
            id: 'windows',
            label: 'Windows',
            command: 'pip install openvino',
          },
        ],
        diagnosticGuide: [
          {
            title: 'Verifique a presença dos drivers OpenCL da Intel:',
            command: 'clinfo | grep -i "Intel"',
          },
          {
            title: 'Verifique os dispositivos detectados pelo OpenVINO:',
            command: 'python3 -c "import openvino as ov; print(ov.Core().available_devices)"',
            tip: 'Deve listar "GPU".',
          },
        ],
      };

    case 'openvino_cpu':
      return {
        librarySearched: {
          linux: 'libopenvino_intel_cpu_plugin.so',
          windows: 'openvino_intel_cpu_plugin.dll',
          description: 'Plugin vetorial de alta performance para CPUs Intel com instruções AVX2 / AMX / VNNI.',
        },
        officialDocs: {
          title: 'Intel OpenVINO Documentation',
          url: 'https://docs.openvino.ai/',
        },
        nativeCommands: [
          {
            id: 'ubuntu',
            label: 'Ubuntu / Debian (APT)',
            command: 'sudo apt update && sudo apt install -y openvino || pip install openvino',
          },
          {
            id: 'fedora',
            label: 'Fedora / RHEL (DNF)',
            command: 'sudo dnf install -y openvino || pip install openvino',
          },
          {
            id: 'arch',
            label: 'Arch Linux (Pacman)',
            command: 'sudo pacman -S --needed openvino || pip install openvino',
          },
          {
            id: 'python',
            label: 'Python (pip / Universal)',
            command: 'pip install openvino',
          },
          {
            id: 'windows',
            label: 'Windows',
            command: 'pip install openvino',
          },
        ],
        diagnosticGuide: [
          {
            title: 'Verifique extensões vetoriais da CPU:',
            command: 'lscpu | grep -iE "avx2|amx|vnni"',
          },
          {
            title: 'Verifique se o backend CPU está ativo no OpenVINO:',
            command: 'python3 -c "import openvino as ov; print(ov.Core().available_devices)"',
            tip: 'Deve listar "CPU".',
          },
        ],
      };

    case 'amd_ryzenai_npu':
      return {
        librarySearched: {
          linux: 'libxrt_core.so',
          windows: 'xrt_core.dll',
          description: 'Biblioteca do runtime AMD XRT / Ryzen AI Software para NPU XDNA.',
        },
        officialDocs: {
          title: 'AMD Ryzen AI Software Documentation',
          url: 'https://ryzenai.docs.amd.com/',
        },
        nativeCommands: [
          {
            id: 'ubuntu',
            label: 'Ubuntu / Debian',
            command: 'sudo apt install -y amdxdna-driver xrt || pip install ryzenai',
          },
          {
            id: 'fedora',
            label: 'Fedora / RHEL',
            command: 'sudo dnf install -y amdxdna-driver xrt || pip install ryzenai',
          },
          {
            id: 'arch',
            label: 'Arch Linux',
            command: 'sudo pacman -S --needed amdxdna-driver-bin xrt || pip install ryzenai',
          },
          {
            id: 'python',
            label: 'Python / Universal',
            command: 'pip install ryzenai',
          },
          {
            id: 'windows',
            label: 'Windows',
            command: 'pip install ryzenai',
            note: 'Instale o driver NPU AMD e o instalador Ryzen AI Software oficial.',
          },
        ],
        diagnosticGuide: [
          {
            title: 'Verifique o driver de kernel amdxdna:',
            command: 'ls -la /dev/amdxdna* 2>/dev/null || lsmod | grep amdxdna',
          },
          {
            title: 'Verifique os utilitários do runtime XRT:',
            command: 'xbutil status 2>/dev/null || xbutil examine',
          },
        ],
      };

    case 'amd_ryzenai_gpu':
      return {
        librarySearched: {
          linux: 'libvulkan_radeon.so / ROCm',
          windows: 'DirectML.dll / vulkan-1.dll',
          description: 'Drivers gráficos e de computação Vulkan RADV, ROCm e DirectML para Radeon.',
        },
        officialDocs: {
          title: 'AMD ROCm & Radeon Documentation',
          url: 'https://rocm.docs.amd.com/',
        },
        nativeCommands: [
          {
            id: 'ubuntu',
            label: 'Ubuntu / Debian',
            command: 'sudo apt install -y mesa-vulkan-drivers rocm-opencl-runtime',
          },
          {
            id: 'fedora',
            label: 'Fedora / RHEL',
            command: 'sudo dnf install -y mesa-vulkan-drivers rocm-opencl',
          },
          {
            id: 'arch',
            label: 'Arch Linux',
            command: 'sudo pacman -S --needed vulkan-radeon opencl-mesa',
          },
          {
            id: 'python',
            label: 'Universal / Vulkan',
            command: 'sudo apt install -y mesa-vulkan-drivers || sudo dnf install -y mesa-vulkan-drivers',
          },
          {
            id: 'windows',
            label: 'Windows',
            command: 'winget install AMD.Adrenalin',
          },
        ],
        diagnosticGuide: [
          {
            title: 'Verifique o suporte e extensões Vulkan da GPU Radeon:',
            command: 'vulkaninfo --summary',
          },
        ],
      };

    default:
      return null;
  }
}

