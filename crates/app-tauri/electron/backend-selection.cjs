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

/**
 * @param {unknown} backendId
 * @returns {{success: true, active_backend: string} | {success: false, reason: string, active_backend: string}}
 */
function resolveBackendSelection(backendId) {
  const id = String(backendId || 'auto');
  if (!SUPPORTED_BACKENDS.has(id)) {
    return { success: false, reason: 'unknown_backend', active_backend: 'auto' };
  }
  return { success: true, active_backend: id };
}

module.exports = { resolveBackendSelection, ENGINE_BACKEND_ID, SUPPORTED_BACKENDS };
