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
