'use strict';
// Gerencia a seleção de backends de hardware aceitos pelo processo principal.
// Testável em node puro (scripts/backend-selection.selftest.cjs).

const ENGINE_BACKEND_ID = 'cpu_tract';

const SUPPORTED_BACKENDS = new Set([
  'auto',
  'nvidia_tensorrt',
  'openvino_npu',
  'openvino_gpu',
  'openvino_cpu',
  'amd_ryzenai_npu',
  'amd_ryzenai_gpu',
  'apple_coreml',
  'cpu_tract',
]);

const BACKEND_MAP_TO_DAEMON = {
  'auto': 'auto',
  'cpu_tract': 'tract',
  'openvino_npu': 'openvino-npu',
  'openvino_gpu': 'openvino-gpu',
  'openvino_cpu': 'openvino-cpu',
  'nvidia_tensorrt': 'tensorrt',
  'amd_ryzenai_npu': 'ryzenai-npu',
  'amd_ryzenai_gpu': 'ryzenai-gpu',
  'apple_coreml': 'coreml',
};

/**
 * Normaliza qualquer formato de argumento (string ou objeto como { backendId: '...' } ou { backend: '...' })
 * para o id em string canônica.
 * @param {unknown} input
 * @returns {string}
 */
function extractBackendId(input) {
  if (input === null || input === undefined) {
    return 'auto';
  }
  if (typeof input === 'string') {
    return input.trim() || 'auto';
  }
  if (typeof input === 'object') {
    const obj = /** @type {Record<string, unknown>} */ (input);
    const candidate = obj.backendId ?? obj.backend ?? obj.id;
    if (typeof candidate === 'string') {
      return candidate.trim() || 'auto';
    }
  }
  return String(input).trim() || 'auto';
}

/**
 * Mapeia o id de backend do Electron/Frontend para o identificador aceito pelo Daemon Rust IPC (SetBackend).
 * @param {string} backendId
 * @returns {string}
 */
function mapBackendToDaemon(backendId) {
  return BACKEND_MAP_TO_DAEMON[backendId] || backendId.replace(/_/g, '-');
}

/**
 * @param {unknown} backendId
 * @returns {{success: true, active_backend: string} | {success: false, reason: string, active_backend: string}}
 */
function resolveBackendSelection(backendId) {
  const id = extractBackendId(backendId);
  if (!SUPPORTED_BACKENDS.has(id)) {
    return { success: false, reason: 'unknown_backend', active_backend: 'auto' };
  }
  return { success: true, active_backend: id };
}

module.exports = {
  resolveBackendSelection,
  extractBackendId,
  mapBackendToDaemon,
  ENGINE_BACKEND_ID,
  SUPPORTED_BACKENDS,
  BACKEND_MAP_TO_DAEMON,
};
