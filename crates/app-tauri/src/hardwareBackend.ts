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
