'use strict';
// Planeja os `pw-link` que ligam o microfone fisico escolhido ao stream de captura
// do helper (realtime-noise-capture). Logica pura, sem Electron nem processos, para
// poder ser testada em node puro (scripts/capture-link-plan.selftest.cjs).
//
// Estereo: FL(_1) -> entrada esquerda e FR(_2) -> entrada direita (como antes).
// Mono (ex.: headset Bluetooth, unica porta "capture_MONO"): a unica porta alimenta
// TODAS as entradas do stream.

const CAPTURE_NODE = 'realtime-noise-capture';

function channelOf(portSuffix) {
  const m = /_([^_]+)$/.exec(portSuffix);
  return m ? m[1] : portSuffix;
}

/**
 * @param {{nodeName: string, outPorts: string[], inPorts: string[]}} args
 *   outPorts: saida de `pw-link -o`; inPorts: saida de `pw-link -i` (portas de entrada).
 * @returns {{src: string, dst: string}[]}
 */
function planCaptureLinks({ nodeName, outPorts, inPorts }) {
  if (!nodeName) return [];
  const prefix = `${nodeName}:`;
  const srcPorts = outPorts
    .map((p) => p.trim())
    .filter((p) => p.startsWith(prefix) && !p.includes('realtime-noise'));
  const inputs = [
    ...new Set(
      inPorts.map((p) => p.trim()).filter((p) => p.startsWith(`${CAPTURE_NODE}:input_`)),
    ),
  ];
  if (srcPorts.length === 0 || inputs.length === 0) return [];

  const links = [];
  if (srcPorts.length === 1) {
    for (const dst of inputs) links.push({ src: srcPorts[0], dst });
    return links;
  }

  const find = (suffix) => inputs.find((p) => p === `${CAPTURE_NODE}:${suffix}`);
  const inLeft = find('input_FL') || find('input_MONO') || inputs[0];
  const inRight = find('input_FR');
  for (const src of srcPorts) {
    const ch = channelOf(src.slice(prefix.length));
    if (ch === 'FL' || ch === '1') links.push({ src, dst: inLeft });
    else if ((ch === 'FR' || ch === '2') && inRight) links.push({ src, dst: inRight });
  }
  return links;
}

module.exports = { planCaptureLinks, CAPTURE_NODE };
