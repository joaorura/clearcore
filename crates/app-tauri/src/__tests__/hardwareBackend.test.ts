import { describe, it, expect } from 'vitest';
import {
  ENGINE_BACKEND_ID,
  SUPPORTED_BACKEND_IDS,
  getDetectedUnusedAccelerator,
  interpretSelectionResult,
  isBackendImplemented,
  isBackendSelectable,
  isRuntimePending,
  isPreviewBackend,
  getBackendHelpDetails,
  getBackendLlmPrompt,
  NVIDIA_TENSORRT_DOCS_URL,
  NVIDIA_TENSORRT_DOWNLOAD_URL,
} from '../hardwareBackend';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';

const backend = (id: string, hardware_detected = true, runtime_installed = true) => ({
  id,
  hardware_detected,
  runtime_installed,
});

describe('hardwareBackend: backends suportados e disponibilidade', () => {
  it('o motor padrão baseline é CPU (Tract)', () => {
    expect(ENGINE_BACKEND_ID).toBe('cpu_tract');
  });

  it('todos os aceleradores oficiais são reconhecidos e implementados', () => {
    for (const id of SUPPORTED_BACKEND_IDS) {
      expect(isBackendImplemented(id)).toBe(true);
      expect(isPreviewBackend(id)).toBe(false);
    }
    expect(isBackendImplemented('desconhecido_xyz')).toBe(false);
    expect(isPreviewBackend('desconhecido_xyz')).toBe(true);
  });

  it('aceleradores com hardware detectado e runtime instalada são selecionáveis e disponíveis', () => {
    expect(isBackendSelectable(backend('openvino_npu', true, true))).toBe(true);
    expect(isBackendSelectable(backend('openvino_gpu', true, true))).toBe(true);
    expect(isBackendSelectable(backend('openvino_cpu', true, true))).toBe(true);
    expect(isBackendSelectable(backend('nvidia_tensorrt', true, true))).toBe(true);
    expect(isBackendSelectable(backend('cpu_tract', true, true))).toBe(true);
    expect(isBackendSelectable(backend('auto', true, true))).toBe(true);
  });

  it('aceleradores sem runtime instalada não são selecionáveis diretamente e constam como pendentes', () => {
    const missingTrt = backend('nvidia_tensorrt', true, false);
    expect(isBackendSelectable(missingTrt)).toBe(false);
    expect(isRuntimePending(missingTrt)).toBe(true);

    const missingOv = backend('openvino_npu', true, false);
    expect(isBackendSelectable(missingOv)).toBe(false);
    expect(isRuntimePending(missingOv)).toBe(true);
  });

  it('aceleradores sem hardware físico presente não são selecionáveis nem pendentes', () => {
    const noHw = backend('amd_ryzenai_npu', false, false);
    expect(isBackendSelectable(noHw)).toBe(false);
    expect(isRuntimePending(noHw)).toBe(false);
  });
});

describe('hardwareBackend: resolução do acelerador automático', () => {
  it('devolve o acelerador selecionado pelo modo Auto', () => {
    expect(getDetectedUnusedAccelerator({ id: 'openvino_npu', name: 'Intel OpenVINO (NPU - AI Boost)' })).toEqual({
      id: 'openvino_npu',
      name: 'Intel OpenVINO (NPU - AI Boost)',
    });
    expect(getDetectedUnusedAccelerator({ id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' })).toEqual({
      id: 'cpu_tract',
      name: 'CPU Nativo (Tract Pure-Rust)',
    });
  });

  it('devolve null quando não há dado ou id é auto', () => {
    expect(getDetectedUnusedAccelerator(null)).toBeNull();
    expect(getDetectedUnusedAccelerator(undefined)).toBeNull();
    expect(getDetectedUnusedAccelerator({ id: 'auto', name: 'Automático' })).toBeNull();
  });
});

describe('hardwareBackend: interpretação da resposta de set_hardware_backend', () => {
  it('aplica a seleção quando o processo principal aceita o backend', () => {
    expect(interpretSelectionResult('openvino_npu', { success: true, active_backend: 'openvino_npu' })).toEqual({
      kind: 'applied',
      activeId: 'openvino_npu',
    });
    expect(interpretSelectionResult('cpu_tract', { success: true, active_backend: 'cpu_tract' })).toEqual({
      kind: 'applied',
      activeId: 'cpu_tract',
    });
    expect(interpretSelectionResult('auto', { success: true, active_backend: 'auto' })).toEqual({
      kind: 'applied',
      activeId: 'auto',
    });
  });

  it('trata resposta vazia da ponte em ambiente web aplicando o id solicitado', () => {
    expect(interpretSelectionResult('openvino_npu', {})).toEqual({
      kind: 'applied',
      activeId: 'openvino_npu',
    });
  });

  it('backend inválido ou não suportado é rejeitado', () => {
    expect(interpretSelectionResult('desconhecido', {})).toEqual({
      kind: 'rejected',
      reason: 'unknown_backend',
      activeId: 'auto',
    });
  });
});

describe('hardwareBackend: consistência de textos e badges de status nos idiomas', () => {
  it('badges de status corretos para runtime pronta (Disponível), ativa e ausente (Pendente)', () => {
    expect(ptBR.hardwareBackend.runtimeReadyBadge).toBe('Disponível');
    expect(ptBR.hardwareBackend.statusActiveBadge).toBe('Ativo');
    expect(ptBR.hardwareBackend.runtimeMissingBadge).toBe('Instalação Pendente');

    expect(enUS.hardwareBackend.runtimeReadyBadge).toBe('Available');
    expect(enUS.hardwareBackend.statusActiveBadge).toBe('Active');
    expect(enUS.hardwareBackend.runtimeMissingBadge).toBe('Installation Pending');
  });

  it('mensagens de sucesso e orientação de troca de acelerador estão presentes', () => {
    expect(ptBR.hardwareBackend.switchSuccess).toContain('{name}');
    expect(enUS.hardwareBackend.switchSuccess).toContain('{name}');
    expect(ptBR.hardwareBackend.cannotSelectMissing).toBeDefined();
    expect(enUS.hardwareBackend.cannotSelectMissing).toBeDefined();
  });
});

describe('hardwareBackend: instruções de instalação, biblioteca procurada e links oficiais', () => {
  it('NVIDIA TensorRT provê biblioteca procurada, comandos nativos oficiais, link de doc e diagnóstico', () => {
    const trt = getBackendHelpDetails('nvidia_tensorrt');
    expect(trt).not.toBeNull();
    expect(trt!.librarySearched.linux).toContain('libnvinfer.so');
    expect(trt!.librarySearched.windows).toContain('nvinfer.dll');
    expect(trt!.officialDocs.url).toBe(NVIDIA_TENSORRT_DOCS_URL);
    expect(trt!.officialDocs.url).toBe(
      'https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html'
    );
    expect(trt!.officialDocs.title).toContain('Referência');
    expect(trt!.officialDownload).toBeDefined();
    expect(trt!.officialDownload!.url).toBe(NVIDIA_TENSORRT_DOWNLOAD_URL);
    expect(trt!.officialDownload!.url).toBe('https://developer.nvidia.com/tensorrt/download');
    expect(trt!.provenMethodNote).toBeDefined();
    expect(trt!.provenMethodNote).toContain('ldconfig');
    expect(trt!.provenMethodNote).toContain('TAR');

    expect(trt!.llmPrompt).toBeDefined();
    expect(trt!.llmPrompt).toContain(NVIDIA_TENSORRT_DOCS_URL);
    expect(trt!.llmPrompt).toContain(NVIDIA_TENSORRT_DOWNLOAD_URL);
    expect(trt!.llmPrompt).toContain('ldconfig');
    expect(trt!.llmPrompt).toContain('libnvinfer');

    const cmdMap = new Map(trt!.nativeCommands.map((c) => [c.id, c.command]));
    expect(cmdMap.get('ubuntu')).toContain('apt install -y libnvinfer11');
    expect(cmdMap.get('fedora')).toContain('dnf install -y tensorrt');
    expect(cmdMap.get('arch')).toContain('pacman -S --needed tensorrt');
    expect(cmdMap.get('python')).toBe('pip install tensorrt');
    expect(cmdMap.get('windows')).toBe('pip install tensorrt');

    const diagCmds = trt!.diagnosticGuide.map((d) => d.command);
    expect(diagCmds).toContain('nvidia-smi');
    expect(diagCmds).toContain('ldconfig -p | grep libnvinfer');
  });

  it('getBackendLlmPrompt gera prompt estruturado com download oficial e método comprovado', () => {
    const defaultPrompt = getBackendLlmPrompt('nvidia_tensorrt');
    expect(defaultPrompt).not.toBeNull();
    expect(defaultPrompt).toContain('libnvinfer');
    expect(defaultPrompt).toContain(NVIDIA_TENSORRT_DOCS_URL);
    expect(defaultPrompt).toContain(NVIDIA_TENSORRT_DOWNLOAD_URL);
    expect(defaultPrompt).toContain('ldconfig');
    expect(defaultPrompt).toContain('Linux/Windows');
    expect(defaultPrompt).toContain('Meu hardware possui uma GPU NVIDIA.');

    const customPrompt = getBackendLlmPrompt('nvidia_tensorrt', {
      os: 'Linux (Ubuntu 24.04)',
      deviceInfo: 'NVIDIA GeForce RTX 4080',
    });
    expect(customPrompt).toContain('Linux (Ubuntu 24.04)');
    expect(customPrompt).toContain('NVIDIA GeForce RTX 4080');
    expect(customPrompt).toContain(NVIDIA_TENSORRT_DOCS_URL);
    expect(customPrompt).toContain(NVIDIA_TENSORRT_DOWNLOAD_URL);

    // Valida internacionalização das novas chaves de download e método comprovado
    expect(ptBR.hardwareBackend.officialDownloadTitle).toContain('Download NVIDIA');
    expect(ptBR.hardwareBackend.openDownloadLink).toBeDefined();
    expect(ptBR.hardwareBackend.provenMethodTitle).toContain('Método Recomendado');
    expect(ptBR.hardwareBackend.provenMethodDesc).toContain('ldconfig');
    expect(enUS.hardwareBackend.officialDownloadTitle).toContain('Download Portal');
    expect(enUS.hardwareBackend.openDownloadLink).toBeDefined();
    expect(enUS.hardwareBackend.provenMethodTitle).toContain('Recommended');
    expect(enUS.hardwareBackend.provenMethodDesc).toContain('ldconfig');

    // Valida internacionalização das chaves do prompt de LLM
    expect(ptBR.hardwareBackend.llmPromptTitle).toContain('💡 Precisa de ajuda com a instalação?');
    expect(ptBR.hardwareBackend.copyLlmPromptBtn).toBe('📋 Copiar Prompt para LLM');
    expect(ptBR.hardwareBackend.llmPromptCopiedBtn).toBe('✓ Copiado!');
    expect(enUS.hardwareBackend.llmPromptTitle).toContain('💡 Need help');
    expect(enUS.hardwareBackend.copyLlmPromptBtn).toBe('📋 Copy Prompt for LLM');
    expect(enUS.hardwareBackend.llmPromptCopiedBtn).toBe('✓ Copied!');
  });

  it('Intel OpenVINO NPU provê biblioteca, comandos nativos, verificação intel-npu-driver e docs', () => {
    const ovNpu = getBackendHelpDetails('openvino_npu');
    expect(ovNpu).not.toBeNull();
    expect(ovNpu!.librarySearched.linux).toContain('libopenvino_intel_npu_plugin.so');
    expect(ovNpu!.officialDocs.url).toContain('configurations-intel-npu.html');

    const cmdMap = new Map(ovNpu!.nativeCommands.map((c) => [c.id, c.command]));
    expect(cmdMap.get('ubuntu')).toContain('intel-npu-driver');
    expect(cmdMap.get('fedora')).toContain('intel-npu-driver');
    expect(cmdMap.get('arch')).toContain('intel-npu-driver');
    expect(cmdMap.get('python')).toBe('pip install openvino');

    const diagCmds = ovNpu!.diagnosticGuide.map((d) => d.command);
    expect(diagCmds).toContain('ls -la /dev/accel/accel*');
    expect(diagCmds).toContain('sudo usermod -aG render $USER');
  });

  it('Intel OpenVINO GPU e CPU possuem instruções e plugins específicos', () => {
    const ovGpu = getBackendHelpDetails('openvino_gpu');
    expect(ovGpu).not.toBeNull();
    expect(ovGpu!.librarySearched.linux).toContain('libopenvino_intel_gpu_plugin.so');

    const ovCpu = getBackendHelpDetails('openvino_cpu');
    expect(ovCpu).not.toBeNull();
    expect(ovCpu!.librarySearched.linux).toContain('libopenvino_intel_cpu_plugin.so');
  });

  it('AMD Ryzen AI NPU e GPU possuem comandos claros e links oficiais', () => {
    const amdNpu = getBackendHelpDetails('amd_ryzenai_npu');
    expect(amdNpu).not.toBeNull();
    expect(amdNpu!.librarySearched.linux).toContain('libxrt_core.so');
    expect(amdNpu!.officialDocs.url).toBe('https://ryzenai.docs.amd.com/');

    const amdGpu = getBackendHelpDetails('amd_ryzenai_gpu');
    expect(amdGpu).not.toBeNull();
    expect(amdGpu!.officialDocs.url).toBe('https://rocm.docs.amd.com/');
  });

  it('backends baseline ou desconhecidos retornam null', () => {
    expect(getBackendHelpDetails('cpu_tract')).toBeNull();
    expect(getBackendHelpDetails('auto')).toBeNull();
    expect(getBackendHelpDetails('inexistente')).toBeNull();
  });
});
