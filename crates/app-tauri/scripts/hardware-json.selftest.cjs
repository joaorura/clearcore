'use strict';
// Autoteste em node puro (fora do glob do vitest): node scripts/hardware-json.selftest.cjs
//
// Causa raiz coberta: queryHardwareBackends (main.cjs) fazia JSON.parse direto da saida de
// detect-hardware.sh --json. Qualquer texto fora do JSON (aviso do shell, lixo de stderr
// misturado) virava excecao, so um console.warn e o fallback JS mostrava "Runtime Ausente"
// sem explicar nada. parseHardwareJson extrai o objeto e devolve {ok, data, error}.
const assert = require('node:assert/strict');
const { parseHardwareJson } = require('../electron/hardware-json.cjs');

const tests = [];
const t = (name, fn) => tests.push([name, fn]);

const OBJ = { auto_resolved_backend: { id: 'cpu_tract' }, backends: [{ id: 'auto' }] };
const JSON_TEXT = JSON.stringify(OBJ, null, 2);

t('JSON limpo', () => {
  const r = parseHardwareJson(JSON_TEXT);
  assert.equal(r.ok, true);
  assert.deepEqual(r.data, OBJ);
  assert.equal(r.error, null);
});

t('texto ANTES do JSON (aviso do shell)', () => {
  const r = parseHardwareJson(`bash: aviso: algo estranho\n${JSON_TEXT}\n`);
  assert.equal(r.ok, true);
  assert.deepEqual(r.data, OBJ);
});

t('texto DEPOIS do JSON', () => {
  const r = parseHardwareJson(`${JSON_TEXT}\nlixo final sem chaves\n`);
  assert.equal(r.ok, true);
  assert.deepEqual(r.data, OBJ);
});

t('texto antes e depois, com chaves dentro de strings do proprio JSON', () => {
  const withBraces = { device_info: 'CPU {modelo} } estranho', backends: [] };
  const r = parseHardwareJson(`prefixo\n${JSON.stringify(withBraces)}\nsufixo`);
  assert.equal(r.ok, true);
  assert.deepEqual(r.data, withBraces);
});

t('lixo NO MEIO (dois objetos separados por texto) falha com erro claro', () => {
  const r = parseHardwareJson('{"a":1}\nlixo no meio\n{"b":2}');
  assert.equal(r.ok, false);
  assert.equal(r.data, null);
  assert.equal(typeof r.error, 'string');
  assert.match(r.error, /JSON/);
  assert.ok(r.error.length < 200, 'erro deve ser curto');
});

t('lixo dentro do objeto falha com erro claro', () => {
  const r = parseHardwareJson('{"a": 1, lixo, "b": 2}');
  assert.equal(r.ok, false);
  assert.match(r.error, /JSON/);
});

t('saida vazia', () => {
  for (const v of ['', '   \n  ', undefined, null]) {
    const r = parseHardwareJson(v);
    assert.equal(r.ok, false);
    assert.equal(r.data, null);
    assert.match(r.error, /vazia/);
  }
});

t('texto sem nenhum objeto JSON', () => {
  const r = parseHardwareJson('Command not found: lspci');
  assert.equal(r.ok, false);
  assert.match(r.error, /JSON/);
});

t('aceita Buffer', () => {
  const r = parseHardwareJson(Buffer.from(JSON_TEXT, 'utf8'));
  assert.equal(r.ok, true);
  assert.deepEqual(r.data, OBJ);
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
