'use strict';
// Autoteste em node puro: node scripts/backend-selection.selftest.cjs
const assert = require('node:assert/strict');
const { resolveBackendSelection, SUPPORTED_BACKENDS } = require('../electron/backend-selection.cjs');

const tests = [];
const t = (name, fn) => tests.push([name, fn]);

t('auto e aceito', () => {
  assert.deepEqual(resolveBackendSelection('auto'), { success: true, active_backend: 'auto' });
});

t('cpu_tract e aceito', () => {
  assert.deepEqual(resolveBackendSelection('cpu_tract'), { success: true, active_backend: 'cpu_tract' });
});

t('openvino backends sao aceitos', () => {
  assert.deepEqual(resolveBackendSelection('openvino_npu'), { success: true, active_backend: 'openvino_npu' });
  assert.deepEqual(resolveBackendSelection('openvino_gpu'), { success: true, active_backend: 'openvino_gpu' });
  assert.deepEqual(resolveBackendSelection('openvino_cpu'), { success: true, active_backend: 'openvino_cpu' });
});

t('outros aceleradores suportados sao aceitos', () => {
  assert.deepEqual(resolveBackendSelection('nvidia_tensorrt'), { success: true, active_backend: 'nvidia_tensorrt' });
  assert.deepEqual(resolveBackendSelection('amd_ryzenai_npu'), { success: true, active_backend: 'amd_ryzenai_npu' });
  assert.deepEqual(resolveBackendSelection('amd_ryzenai_gpu'), { success: true, active_backend: 'amd_ryzenai_gpu' });
  assert.deepEqual(resolveBackendSelection('apple_coreml'), { success: true, active_backend: 'apple_coreml' });
});

t('vazio/indefinido vira auto', () => {
  assert.deepEqual(resolveBackendSelection(undefined), { success: true, active_backend: 'auto' });
  assert.deepEqual(resolveBackendSelection(''), { success: true, active_backend: 'auto' });
});

t('desempacotamento de objeto { backendId: "..." } e aceito', () => {
  assert.deepEqual(resolveBackendSelection({ backendId: 'openvino_npu' }), { success: true, active_backend: 'openvino_npu' });
  assert.deepEqual(resolveBackendSelection({ backendId: 'openvino_gpu' }), { success: true, active_backend: 'openvino_gpu' });
  assert.deepEqual(resolveBackendSelection({ backendId: 'openvino_cpu' }), { success: true, active_backend: 'openvino_cpu' });
  assert.deepEqual(resolveBackendSelection({ backendId: 'cpu_tract' }), { success: true, active_backend: 'cpu_tract' });
});

t('desempacotamento de objeto { backend: "..." } e { id: "..." } e aceito', () => {
  assert.deepEqual(resolveBackendSelection({ backend: 'openvino_npu' }), { success: true, active_backend: 'openvino_npu' });
  assert.deepEqual(resolveBackendSelection({ id: 'nvidia_tensorrt' }), { success: true, active_backend: 'nvidia_tensorrt' });
});

t('mapeamento de backend para daemon Rust IPC (SetBackend)', () => {
  const { mapBackendToDaemon } = require('../electron/backend-selection.cjs');
  assert.equal(mapBackendToDaemon('auto'), 'auto');
  assert.equal(mapBackendToDaemon('cpu_tract'), 'tract');
  assert.equal(mapBackendToDaemon('openvino_npu'), 'openvino-npu');
  assert.equal(mapBackendToDaemon('openvino_gpu'), 'openvino-gpu');
  assert.equal(mapBackendToDaemon('openvino_cpu'), 'openvino-cpu');
  assert.equal(mapBackendToDaemon('nvidia_tensorrt'), 'tensorrt');
  assert.equal(mapBackendToDaemon('amd_ryzenai_npu'), 'ryzenai-npu');
  assert.equal(mapBackendToDaemon('apple_coreml'), 'coreml');
});

t('backend desconhecido em objeto e rejeitado', () => {
  assert.deepEqual(resolveBackendSelection({ backendId: 'qualquer_coisa_invalida' }), {
    success: false,
    reason: 'unknown_backend',
    active_backend: 'auto',
  });
});

let failed = 0;
for (const [name, fn] of tests) {
  try {
    fn();
    console.log(`ok   - ${name}`);
  } catch (e) {
    failed++;
    console.log(`FAIL - ${name}\n${e.message}`);
  }
}
console.log(`${tests.length - failed}/${tests.length} passed`);
process.exit(failed ? 1 : 0);
