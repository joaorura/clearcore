'use strict';
// Extrai o objeto JSON da saida de `detect-hardware.sh --json`. Logica pura, sem
// Electron nem processos, para ser testada em node puro
// (scripts/hardware-json.selftest.cjs).
//
// A saida deveria ser so JSON, mas qualquer aviso do shell antes ou depois quebrava o
// JSON.parse direto. Aqui o texto vai do primeiro "{" ao ultimo "}"; se mesmo assim nao
// for um JSON valido (ex.: lixo no meio), devolve um erro curto e claro em vez de lancar.

/**
 * @param {string | Buffer | null | undefined} text
 * @returns {{ok: true, data: object, error: null} | {ok: false, data: null, error: string}}
 */
function parseHardwareJson(text) {
  const raw = text == null ? '' : String(text);
  if (raw.trim() === '') {
    return { ok: false, data: null, error: 'saída vazia do detector de hardware' };
  }
  const start = raw.indexOf('{');
  const end = raw.lastIndexOf('}');
  if (start === -1 || end === -1 || end < start) {
    return { ok: false, data: null, error: 'nenhum objeto JSON na saída do detector de hardware' };
  }
  try {
    const data = JSON.parse(raw.slice(start, end + 1));
    if (data === null || typeof data !== 'object' || Array.isArray(data)) {
      return { ok: false, data: null, error: 'JSON do detector de hardware não é um objeto' };
    }
    return { ok: true, data, error: null };
  } catch (err) {
    const msg = String(err && err.message ? err.message : err).slice(0, 120);
    return { ok: false, data: null, error: `JSON inválido na saída do detector de hardware: ${msg}` };
  }
}

module.exports = { parseHardwareJson };
