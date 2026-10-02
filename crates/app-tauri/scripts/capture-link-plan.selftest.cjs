'use strict';
// Autoteste em node puro (fora do glob do vitest): node scripts/capture-link-plan.selftest.cjs
//
// Causa raiz coberta: setSystemInputDevice (main.cjs) so ligava portas terminadas em
// _FL/_1/_FR/_2. Um microfone MONO (ex.: headset Bluetooth) expoe uma unica porta
// "...:capture_MONO", que nunca casava; nenhum pw-link era feito e so saia silencio.
const assert = require('node:assert/strict');
const { planCaptureLinks } = require('../electron/capture-link-plan.cjs');

const CAP = 'realtime-noise-capture';
const IN_STEREO = [`${CAP}:input_FL`, `${CAP}:input_FR`];
const IN_MONO = [`${CAP}:input_MONO`];

const ALSA = 'alsa_input.pci-0000_00_1f.3-platform-sof_sdw.HiFi__Mic__source';
const BT = 'bluez_input.38:FB:A0:38:F7:EB'; // node.name de Bluetooth contem ':'

const tests = [];
const t = (name, fn) => tests.push([name, fn]);

t('estereo ALSA com entradas FL/FR: FL->FL e FR->FR (comportamento anterior preservado)', () => {
  const out = [`${ALSA}:capture_FL`, `${ALSA}:capture_FR`, 'realtime-noise-source:capture_MONO'];
  assert.deepEqual(planCaptureLinks({ nodeName: ALSA, outPorts: out, inPorts: IN_STEREO }), [
    { src: `${ALSA}:capture_FL`, dst: `${CAP}:input_FL` },
    { src: `${ALSA}:capture_FR`, dst: `${CAP}:input_FR` },
  ]);
});

t('estereo com entrada unica input_MONO: so o canal esquerdo (igual ao fallback anterior)', () => {
  const out = [`${ALSA}:capture_FL`, `${ALSA}:capture_FR`];
  assert.deepEqual(planCaptureLinks({ nodeName: ALSA, outPorts: out, inPorts: IN_MONO }), [
    { src: `${ALSA}:capture_FL`, dst: `${CAP}:input_MONO` },
  ]);
});

t('estereo com portas numeradas _1/_2', () => {
  const out = ['v4l2_input.x:capture_1', 'v4l2_input.x:capture_2'];
  assert.deepEqual(planCaptureLinks({ nodeName: 'v4l2_input.x', outPorts: out, inPorts: IN_STEREO }), [
    { src: 'v4l2_input.x:capture_1', dst: `${CAP}:input_FL` },
    { src: 'v4l2_input.x:capture_2', dst: `${CAP}:input_FR` },
  ]);
});

t('MONO (Bluetooth) com entradas FL/FR: a unica porta alimenta as duas entradas', () => {
  const out = [`${BT}:capture_MONO`, `${ALSA}:capture_FL`];
  assert.deepEqual(planCaptureLinks({ nodeName: BT, outPorts: out, inPorts: IN_STEREO }), [
    { src: `${BT}:capture_MONO`, dst: `${CAP}:input_FL` },
    { src: `${BT}:capture_MONO`, dst: `${CAP}:input_FR` },
  ]);
});

t('MONO com entrada unica input_MONO', () => {
  assert.deepEqual(
    planCaptureLinks({ nodeName: BT, outPorts: [`${BT}:capture_MONO`], inPorts: IN_MONO }),
    [{ src: `${BT}:capture_MONO`, dst: `${CAP}:input_MONO` }],
  );
});

t('fonte com uma unica porta FL e tratada como mono', () => {
  assert.deepEqual(
    planCaptureLinks({ nodeName: ALSA, outPorts: [`${ALSA}:capture_FL`], inPorts: IN_STEREO }),
    [
      { src: `${ALSA}:capture_FL`, dst: `${CAP}:input_FL` },
      { src: `${ALSA}:capture_FL`, dst: `${CAP}:input_FR` },
    ],
  );
});

t('prefixo de nome nao confunde nos: "alsa_input.a" nao casa "alsa_input.ab"', () => {
  const out = ['alsa_input.ab:capture_FL', 'alsa_input.ab:capture_FR'];
  assert.deepEqual(planCaptureLinks({ nodeName: 'alsa_input.a', outPorts: out, inPorts: IN_STEREO }), []);
});

t('nunca liga portas do proprio realtime-noise nem sem entradas', () => {
  assert.deepEqual(
    planCaptureLinks({
      nodeName: 'realtime-noise-source',
      outPorts: ['realtime-noise-source:capture_MONO'],
      inPorts: IN_STEREO,
    }),
    [],
  );
  assert.deepEqual(planCaptureLinks({ nodeName: BT, outPorts: [`${BT}:capture_MONO`], inPorts: [] }), []);
  assert.deepEqual(planCaptureLinks({ nodeName: '', outPorts: [`${BT}:capture_MONO`], inPorts: IN_STEREO }), []);
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
