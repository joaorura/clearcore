#!/usr/bin/env node
// Self-test of openvino-runtime.mjs (no network, no real libraries): lock parsing, file
// selection per platform, assemble -> verify -> bundle, and tamper detection.
import assert from 'assert/strict';
import fs from 'fs';
import os from 'os';
import path from 'path';
import {
  assemble, bundleInto, embedsOpenVino, lockFor, readLock, selectFiles, verify, MANIFEST,
} from './openvino-runtime.mjs';

let passed = 0;
function check(name, fn) {
  fn();
  passed += 1;
  console.log(`ok   - ${name}`);
}

const LINUX_LIBS = [
  'libopenvino_c.so.2641', 'libopenvino.so.2641', 'libopenvino_onnx_frontend.so.2641',
  'libopenvino_ir_frontend.so.2641', 'libopenvino_intel_cpu_plugin.so', 'libopenvino_intel_gpu_plugin.so',
  'libopenvino_intel_npu_plugin.so', 'libtbb.so.12', 'libtbbmalloc.so.2', 'libtbbbind_2_5.so.3',
  'libhwloc.so.15', 'cache.json',
];

check('the pinned lock has complete Linux x64 and Windows x64 entries', () => {
  const lock = readLock();
  for (const platform of ['linux-x64', 'win32-x64']) {
    const entry = lockFor(platform, lock);
    assert.match(entry.sha256, /^[0-9a-f]{64}$/);
    assert.ok(entry.url.startsWith('https://files.pythonhosted.org/'), entry.url);
    assert.ok(entry.url.endsWith(entry.wheel), entry.url);
    assert.ok(entry.wheel.includes(entry.version), entry.wheel);
    assert.ok(entry.size > 10_000_000);
  }
  assert.throws(() => lockFor('linux-arm64', lock));
});

check('only Linux x64 and Windows x64 embed OpenVINO', () => {
  assert.equal(embedsOpenVino('linux', 'x64'), true);
  assert.equal(embedsOpenVino('win32', 'x64'), true);
  assert.equal(embedsOpenVino('linux', 'arm64'), false);
  assert.equal(embedsOpenVino('win32', 'arm64'), false);
  assert.equal(embedsOpenVino('darwin', 'arm64'), false);
});

check('selection keeps the runtime and drops Python bindings and unused frontends', () => {
  const names = [...LINUX_LIBS, 'libopenvino_pytorch_frontend.so.2641', 'libopenvino_tensorflow_frontend.so.2641',
    'libopenvino_auto_plugin.so', 'tbb.pc', '_pyopenvino.cpython-313-x86_64-linux-gnu.so'];
  assert.deepEqual(selectFiles(names, 'linux-x64'), [...LINUX_LIBS].sort());
  assert.throws(() => selectFiles(LINUX_LIBS.filter((n) => !n.includes('npu')), 'linux-x64'), /npu/);
  const win = ['openvino_c.dll', 'openvino.dll', 'openvino_onnx_frontend.dll', 'openvino_intel_cpu_plugin.dll',
    'openvino_intel_gpu_plugin.dll', 'openvino_intel_npu_plugin.dll', 'tbb12.dll', 'openvino_pytorch_frontend.dll'];
  assert.deepEqual(selectFiles(win, 'win32-x64'), win.filter((n) => !n.includes('pytorch')).sort());
});

check('assemble -> verify -> bundle, and a tampered file is refused', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'ov-selftest-'));
  try {
    const libs = path.join(root, 'wheel', 'openvino', 'libs');
    const licenses = path.join(root, 'wheel', 'openvino-2026.4.1.dist-info', 'licenses');
    fs.mkdirSync(libs, { recursive: true });
    fs.mkdirSync(licenses, { recursive: true });
    for (const name of [...LINUX_LIBS, 'libopenvino_paddle_frontend.so.2641']) {
      fs.writeFileSync(path.join(libs, name), `fake ${name}`);
    }
    fs.writeFileSync(path.join(licenses, 'LICENSE'), 'Apache-2.0');
    const out = path.join(root, 'runtime');
    const manifest = assemble({ from: path.join(root, 'wheel'), out, platform: 'linux-x64', wheelSha256: 'ab'.repeat(32) });
    assert.equal(Object.keys(manifest.files).length, LINUX_LIBS.length);
    assert.ok(!fs.existsSync(path.join(out, 'libopenvino_paddle_frontend.so.2641')));
    assert.ok(fs.existsSync(path.join(out, 'licenses', 'LICENSE')));
    assert.ok(fs.existsSync(path.join(out, MANIFEST)));
    verify(out);
    const dest = path.join(root, 'app', 'resources', 'openvino');
    const { bytes } = bundleInto(out, dest);
    assert.ok(bytes > 0);
    verify(dest);
    fs.appendFileSync(path.join(out, 'libopenvino_intel_npu_plugin.so'), 'x');
    assert.throws(() => verify(out), /size|SHA-256/);
    assert.throws(() => bundleInto(out, path.join(root, 'other')), /size|SHA-256/);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
});

console.log(`\n${passed} ok`);
