'use strict';
// Decide se set_hardware_backend aceita o backend pedido. Logica pura, sem Electron,
// testavel em node puro (scripts/backend-selection.selftest.cjs).
//
// Hoje o unico motor que processa audio e o Tract na CPU. Os demais aceleradores
// (OpenVINO, TensorRT, CoreML, Ryzen AI) sao detectados mas ainda sao previa/stub, entao
// escolhe-los nao pode devolver success:true como se algo tivesse mudado.

const ENGINE_BACKEND_ID = 'cpu_tract';
const SELECTABLE_BACKENDS = new Set(['auto', ENGINE_BACKEND_ID]);

/**
 * @param {unknown} backendId
 * @returns {{success: true, active_backend: string} | {success: false, reason: 'not_implemented', active_backend: string}}
 */
function resolveBackendSelection(backendId) {
  const id = String(backendId || 'auto');
  if (!SELECTABLE_BACKENDS.has(id)) {
    return { success: false, reason: 'not_implemented', active_backend: ENGINE_BACKEND_ID };
  }
  return { success: true, active_backend: id };
}

module.exports = { resolveBackendSelection, ENGINE_BACKEND_ID };
