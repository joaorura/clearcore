'use strict';
// Autoteste em node puro (fora do glob do vitest): node scripts/backend-selection.selftest.cjs
//
// Causa raiz coberta: set_hardware_backend (main.cjs) so guardava uma variavel e devolvia
// success:true para QUALQUER backend, embora o unico motor real seja o Tract na CPU
// (filter-capi sempre constroi TractBackend; o plugin OpenVINO e um stub passthrough).
const assert = require('node:assert/strict');
const { resolveBackendSelection } = require('../electron/backend-selection.cjs');

const tests = [];
const t = (name, fn) => tests.push([name, fn]);

t('auto e aceito', () => {
  assert.deepEqual(resolveBackendSelection('auto'), { success: true, active_backend: 'auto' });
});

t('cpu_tract e aceito', () => {
  assert.deepEqual(resolveBackendSelection('cpu_tract'), { success: true, active_backend: 'cpu_tract' });
});

t('vazio/indefinido vira auto', () => {
  assert.deepEqual(resolveBackendSelection(undefined), { success: true, active_backend: 'auto' });
  assert.deepEqual(resolveBackendSelection(''), { success: true, active_backend: 'auto' });
});

t('aceleradores que ainda nao processam audio sao rejeitados', () => {
  for (const id of ['nvidia_tensorrt', 'openvino_npu', 'openvino_gpu', 'openvino_cpu', 'amd_ryzenai_npu', 'amd_ryzenai_gpu', 'apple_coreml', 'qualquer_coisa']) {
    assert.deepEqual(resolveBackendSelection(id), {
      success: false,
      reason: 'not_implemented',
      active_backend: 'cpu_tract',
    });
  }
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
