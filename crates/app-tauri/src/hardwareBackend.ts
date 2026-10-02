// Decisoes puras (sem React, sem ponte) sobre o seletor de acelerador.
//
// Verdade do produto hoje: o unico motor que processa audio e o Tract na CPU.
// OpenVINO/TensorRT/CoreML/Ryzen AI sao detectados, mas ainda nao processam audio
// ("previa"). A UI so deve permitir selecionar o que o motor realmente executa.

/** Motor que de fato processa audio. */
export const ENGINE_BACKEND_ID = 'cpu_tract';

/** Backends que o processo principal aceita em set_hardware_backend. */
const IMPLEMENTED_BACKEND_IDS: readonly string[] = ['auto', ENGINE_BACKEND_ID];

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
  return IMPLEMENTED_BACKEND_IDS.includes(id);
}

/** Acelerador detectavel mas que ainda nao processa audio. */
export function isPreviewBackend(id: string): boolean {
  return !isBackendImplemented(id);
}

export function isBackendSelectable(backend: BackendLike): boolean {
  if (!isBackendImplemented(backend.id)) return false;
  return backend.id === 'auto' || (backend.hardware_detected && backend.runtime_installed);
}

/**
 * Acelerador que o modo Automatico escolheria se ele ja fosse usado pelo motor.
 * Null quando o Automatico resolve para o proprio Tract (nada a mostrar a parte).
 */
export function getDetectedUnusedAccelerator(
  autoResolved: NamedBackend | null | undefined,
): NamedBackend | null {
  if (!autoResolved || !autoResolved.id || autoResolved.id === ENGINE_BACKEND_ID) return null;
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
 * Interpreta a resposta de set_hardware_backend. Um backend em previa nunca e
 * "aplicado", mesmo que a resposta venha vazia ou com success: o motor nao o usa.
 */
export function interpretSelectionResult(requestedId: string, res: unknown): SelectionOutcome {
  const r = (res && typeof res === 'object' ? res : {}) as SetBackendResponse;
  if (!isBackendImplemented(requestedId)) {
    return { kind: 'rejected', reason: 'not_implemented', activeId: ENGINE_BACKEND_ID };
  }
  if (r.success === false) {
    return {
      kind: 'rejected',
      reason: r.reason || 'unknown',
      activeId: r.active_backend || ENGINE_BACKEND_ID,
    };
  }
  return { kind: 'applied', activeId: r.active_backend || requestedId };
}
