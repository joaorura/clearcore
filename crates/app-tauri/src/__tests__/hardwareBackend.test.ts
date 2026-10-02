import { describe, it, expect } from 'vitest';
import {
  ENGINE_BACKEND_ID,
  getDetectedUnusedAccelerator,
  interpretSelectionResult,
  isBackendImplemented,
  isBackendSelectable,
  isPreviewBackend,
} from '../hardwareBackend';
import { ptBR } from '../i18n/locales/pt-BR';
import { enUS } from '../i18n/locales/en-US';

const backend = (id: string, hardware_detected = true, runtime_installed = true) => ({
  id,
  hardware_detected,
  runtime_installed,
});

describe('hardwareBackend: o que o motor realmente executa', () => {
  it('o motor real e unico: CPU (Tract)', () => {
    expect(ENGINE_BACKEND_ID).toBe('cpu_tract');
  });

  it('somente auto e cpu_tract estao implementados', () => {
    expect(isBackendImplemented('auto')).toBe(true);
    expect(isBackendImplemented('cpu_tract')).toBe(true);
    for (const id of ['nvidia_tensorrt', 'openvino_npu', 'openvino_gpu', 'openvino_cpu', 'amd_ryzenai_npu', 'amd_ryzenai_gpu', 'apple_coreml']) {
      expect(isBackendImplemented(id)).toBe(false);
      expect(isPreviewBackend(id)).toBe(true);
    }
    expect(isPreviewBackend('auto')).toBe(false);
    expect(isPreviewBackend('cpu_tract')).toBe(false);
  });

  it('aceleradores em previa nunca sao selecionaveis, mesmo com hardware e runtime prontos', () => {
    expect(isBackendSelectable(backend('openvino_npu', true, true))).toBe(false);
    expect(isBackendSelectable(backend('nvidia_tensorrt', true, true))).toBe(false);
  });

  it('auto e cpu_tract sao selecionaveis', () => {
    expect(isBackendSelectable(backend('auto', true, true))).toBe(true);
    expect(isBackendSelectable(backend('cpu_tract', true, true))).toBe(true);
  });
});

describe('hardwareBackend: acelerador detectado separado do mecanismo ativo', () => {
  it('devolve o acelerador detectado quando o auto resolveu para algo diferente de Tract', () => {
    expect(getDetectedUnusedAccelerator({ id: 'openvino_npu', name: 'Intel OpenVINO (NPU - AI Boost)' })).toEqual({
      id: 'openvino_npu',
      name: 'Intel OpenVINO (NPU - AI Boost)',
    });
  });

  it('devolve null quando o auto resolveu para a propria CPU (Tract) ou nao ha dado', () => {
    expect(getDetectedUnusedAccelerator({ id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' })).toBeNull();
    expect(getDetectedUnusedAccelerator(null)).toBeNull();
    expect(getDetectedUnusedAccelerator(undefined)).toBeNull();
  });
});

describe('hardwareBackend: interpretacao da resposta de set_hardware_backend', () => {
  it('rejeicao not_implemented do processo principal nao muda o mecanismo ativo', () => {
    expect(
      interpretSelectionResult('openvino_npu', { success: false, reason: 'not_implemented', active_backend: 'cpu_tract' }),
    ).toEqual({ kind: 'rejected', reason: 'not_implemented', activeId: 'cpu_tract' });
  });

  it('backend em previa e rejeitado mesmo que a resposta venha vazia ou com success', () => {
    expect(interpretSelectionResult('openvino_npu', {})).toMatchObject({ kind: 'rejected', reason: 'not_implemented' });
    expect(interpretSelectionResult('openvino_npu', { success: true, active_backend: 'openvino_npu' })).toMatchObject({
      kind: 'rejected',
      reason: 'not_implemented',
      activeId: 'cpu_tract',
    });
  });

  it('auto e cpu_tract sao aplicados', () => {
    expect(interpretSelectionResult('cpu_tract', { success: true, active_backend: 'cpu_tract' })).toEqual({
      kind: 'applied',
      activeId: 'cpu_tract',
    });
    expect(interpretSelectionResult('auto', { success: true, active_backend: 'auto' })).toEqual({
      kind: 'applied',
      activeId: 'auto',
    });
    // Sem ponte (ex.: navegador puro) a resposta vem vazia: aplica o id pedido.
    expect(interpretSelectionResult('auto', {})).toEqual({ kind: 'applied', activeId: 'auto' });
  });
});

describe('hardwareBackend: textos nao prometem o que o motor nao faz', () => {
  it('previa e mecanismo ativo reais nos dois idiomas', () => {
    expect(ptBR.hardwareBackend.previewBadge).toBe('Prévia — ainda não processa áudio');
    expect(ptBR.hardwareBackend.activeEngineName).toBe('CPU (Tract)');
    expect(ptBR.hardwareBackend.detectedUnusedLabel).toBe('Detectado (ainda não utilizado):');
    expect(enUS.hardwareBackend.previewBadge).toMatch(/^Preview/);
    expect(enUS.hardwareBackend.activeEngineName).toBe('CPU (Tract)');
  });

  it('descricao nao diz que o acelerador executa o DeepFilterNet3', () => {
    expect(ptBR.hardwareBackend.description).not.toMatch(/Selecione qual acelerador/i);
    expect(enUS.hardwareBackend.description).not.toMatch(/Select which accelerator/i);
    expect(ptBR.hardwareBackend.description).toMatch(/CPU/);
    expect(enUS.hardwareBackend.description).toMatch(/CPU/);
  });

  it('nao existe mais mensagem de sucesso falso na troca de acelerador', () => {
    expect('switchSuccess' in ptBR.hardwareBackend).toBe(false);
    expect('switchSuccess' in enUS.hardwareBackend).toBe(false);
  });
});
