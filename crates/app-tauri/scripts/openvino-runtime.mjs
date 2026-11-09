#!/usr/bin/env node
// OpenVINO runtime embedded in the Clearcore packages (<app>/resources/openvino).
//
// The runtime comes from the official `openvino` wheel on PyPI, pinned (version, URL, size and
// SHA-256) in packaging/openvino/openvino-runtime.lock. Only native libraries are kept: the C
// API the daemon dlopens, the core, the ONNX/IR frontends, the CPU/GPU/NPU plugins, TBB (+ hwloc
// for tbbbind) and the GPU kernel cache, plus the wheel's license files. Plugins are found by
// OpenVINO next to libopenvino (no plugins.xml), and every library carries RPATH $ORIGIN, so the
// daemon only needs the absolute path of libopenvino_c (crates/runtime-openvino/src/ffi.rs).
//
// Usage:
//   node openvino-runtime.mjs fetch    --platform linux-x64|win32-x64 --out <dir>
//   node openvino-runtime.mjs assemble --platform linux-x64|win32-x64 --from <wheel's openvino/ dir>
//                                      --out <dir> [--wheel-sha256 <hex>] [--licenses <dir>]
//   node openvino-runtime.mjs verify   --dir <dir>
// `fetch` downloads the pinned wheel, checks size + SHA-256 before extracting, then assembles.
// `assemble` builds the same layout from an already-extracted wheel (local packaging and tests).
// Every assembled directory has an openvino-runtime.json manifest (per-file SHA-256) that
// package-app.mjs verifies before copying.
import fs from 'fs';
import os from 'os';
import path from 'path';
import { createHash } from 'crypto';
import { execFileSync } from 'child_process';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
export const REPO_ROOT = path.resolve(__dirname, '..', '..', '..');
export const LOCK_FILE = path.join(REPO_ROOT, 'packaging', 'openvino', 'openvino-runtime.lock');
export const MANIFEST = 'openvino-runtime.json';

/** File-name patterns kept from the wheel's openvino/libs, per platform. */
export const KEEP = {
  'linux-x64': [
    /^libopenvino_c\.so\.\d+$/,
    /^libopenvino\.so\.\d+$/,
    /^libopenvino_onnx_frontend\.so\.\d+$/,
    /^libopenvino_ir_frontend\.so\.\d+$/,
    /^libopenvino_intel_(cpu|gpu|npu)_plugin\.so$/,
    /^libtbb\.so\.\d+$/,
    /^libtbbmalloc\.so\.\d+$/,
    /^libtbbbind_2_5\.so\.\d+$/,
    /^libhwloc\.so\.\d+$/,
    /^cache\.json$/,
  ],
  'win32-x64': [
    /^openvino_c\.dll$/i,
    /^openvino\.dll$/i,
    /^openvino_onnx_frontend\.dll$/i,
    /^openvino_ir_frontend\.dll$/i,
    /^openvino_intel_(cpu|gpu|npu)_plugin\.dll$/i,
    /^tbb12\.dll$/i,
    /^tbbmalloc\.dll$/i,
    /^tbbbind_2_5\.dll$/i,
    /^hwloc\.dll$/i,
    /^cache\.json$/i,
  ],
};

/** Files that must be present for the runtime to be usable on each platform. */
export const REQUIRED = {
  'linux-x64': [/^libopenvino_c\.so\.\d+$/, /^libopenvino\.so\.\d+$/, /^libopenvino_onnx_frontend\.so\.\d+$/,
    /^libopenvino_intel_cpu_plugin\.so$/, /^libopenvino_intel_gpu_plugin\.so$/, /^libopenvino_intel_npu_plugin\.so$/,
    /^libtbb\.so\.\d+$/],
  'win32-x64': [/^openvino_c\.dll$/i, /^openvino\.dll$/i, /^openvino_onnx_frontend\.dll$/i,
    /^openvino_intel_cpu_plugin\.dll$/i, /^openvino_intel_gpu_plugin\.dll$/i, /^openvino_intel_npu_plugin\.dll$/i,
    /^tbb12\.dll$/i],
};

/** Lock-file key prefix of each platform. */
const LOCK_PREFIX = { 'linux-x64': 'LINUX_X64', 'win32-x64': 'WINDOWS_X64' };

/** Platforms that embed OpenVINO (ARM64 and macOS stay on tract). */
export function embedsOpenVino(platform, arch) {
  return Object.hasOwn(KEEP, `${platform}-${arch}`);
}

export function readLock(file = LOCK_FILE) {
  const lock = {};
  for (const line of fs.readFileSync(file, 'utf8').split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq > 0) lock[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
  }
  return lock;
}

export function lockFor(platform, lock = readLock()) {
  const prefix = LOCK_PREFIX[platform];
  if (!prefix) throw new Error(`OpenVINO is not embedded for ${platform}`);
  const entry = {
    version: lock.OPENVINO_VERSION,
    build: lock.OPENVINO_BUILD,
    wheel: lock[`${prefix}_WHEEL`],
    url: lock[`${prefix}_URL`],
    size: Number(lock[`${prefix}_SIZE`]),
    sha256: (lock[`${prefix}_SHA256`] || '').toLowerCase(),
  };
  if (!entry.version || !entry.url || !/^[0-9a-f]{64}$/.test(entry.sha256) || !entry.size) {
    throw new Error(`incomplete OpenVINO lock entry for ${platform}: ${JSON.stringify(entry)}`);
  }
  return entry;
}

export function sha256File(file) {
  return createHash('sha256').update(fs.readFileSync(file)).digest('hex');
}

/** Selects the files to keep from a directory listing (pure, unit-tested). */
export function selectFiles(names, platform) {
  const keep = KEEP[platform];
  if (!keep) throw new Error(`unknown OpenVINO platform ${platform}`);
  const selected = names.filter((name) => keep.some((re) => re.test(name))).sort();
  const missing = REQUIRED[platform].filter((re) => !selected.some((name) => re.test(name)));
  if (missing.length) {
    throw new Error(`OpenVINO runtime incomplete for ${platform}: missing ${missing.map(String).join(', ')}`);
  }
  return selected;
}

/** Directory (searched recursively) that holds the C API library of `platform`. */
function findLibsDir(root, platform) {
  const target = platform === 'win32-x64' ? /^openvino_c\.dll$/i : /^libopenvino_c\.so\.\d+$/;
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) stack.push(full);
      else if (target.test(entry.name)) return dir;
    }
  }
  throw new Error(`no OpenVINO C API library under ${root}`);
}

function findLicensesDir(root) {
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop();
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (!entry.isDirectory()) continue;
      if (entry.name === 'licenses' && /openvino-[^/\\]*\.dist-info$/.test(dir)) return full;
      stack.push(full);
    }
  }
  return null;
}

/**
 * Builds <out> from an extracted wheel (or any directory containing the wheel's libs): copies the
 * kept libraries (dereferencing symlinks), the license files, and writes the manifest.
 */
export function assemble({ from, out, platform, wheelSha256 = null, licenses = null, version = null }) {
  const libsDir = findLibsDir(from, platform);
  const files = selectFiles(fs.readdirSync(libsDir), platform);
  fs.rmSync(out, { recursive: true, force: true });
  fs.mkdirSync(out, { recursive: true });
  const manifestFiles = {};
  for (const name of files) {
    const dest = path.join(out, name);
    fs.copyFileSync(fs.realpathSync(path.join(libsDir, name)), dest);
    if (!name.endsWith('.json')) fs.chmodSync(dest, 0o755);
    manifestFiles[name] = { sha256: sha256File(dest), size: fs.statSync(dest).size };
  }
  const licensesDir = licenses || findLicensesDir(from);
  if (licensesDir && fs.existsSync(licensesDir)) {
    fs.cpSync(licensesDir, path.join(out, 'licenses'), { recursive: true });
  }
  const manifest = {
    product: 'openvino',
    version: version || readLock().OPENVINO_VERSION,
    platform,
    source: wheelSha256 ? 'pypi-wheel' : 'local-directory',
    wheel_sha256: wheelSha256,
    files: manifestFiles,
  };
  fs.writeFileSync(path.join(out, MANIFEST), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

/** Checks every manifest entry (presence, size, SHA-256). Returns the manifest. */
export function verify(dir) {
  const manifestPath = path.join(dir, MANIFEST);
  if (!fs.existsSync(manifestPath)) throw new Error(`${manifestPath} missing`);
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  selectFiles(Object.keys(manifest.files), manifest.platform);
  for (const [name, { sha256, size }] of Object.entries(manifest.files)) {
    const file = path.join(dir, name);
    if (!fs.existsSync(file)) throw new Error(`${file} missing`);
    if (fs.statSync(file).size !== size) throw new Error(`${file}: size differs from the manifest`);
    if (sha256File(file) !== sha256) throw new Error(`${file}: SHA-256 differs from the manifest`);
  }
  return manifest;
}

/** Copies a verified runtime into <dest> (resources/openvino). Returns the total bytes. */
export function bundleInto(src, dest) {
  const manifest = verify(src);
  fs.rmSync(dest, { recursive: true, force: true });
  fs.cpSync(src, dest, { recursive: true, dereference: true });
  let bytes = 0;
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const full = path.join(dir, entry.name);
      if (entry.isDirectory()) walk(full);
      else bytes += fs.statSync(full).size;
    }
  };
  walk(dest);
  return { manifest, bytes };
}

function extractZip(zip, dest) {
  fs.mkdirSync(dest, { recursive: true });
  if (process.platform === 'win32') {
    execFileSync('tar', ['-xf', zip, '-C', dest], { stdio: 'inherit' });
    return;
  }
  try {
    execFileSync('unzip', ['-q', zip, '-d', dest], { stdio: 'inherit' });
  } catch {
    execFileSync('python3', ['-I', '-m', 'zipfile', '-e', zip, dest], { stdio: 'inherit' });
  }
}

export async function fetchRuntime({ platform, out }) {
  const entry = lockFor(platform);
  const work = fs.mkdtempSync(path.join(os.tmpdir(), 'clearcore-openvino-'));
  try {
    const wheel = path.join(work, entry.wheel);
    console.log(`Downloading ${entry.url}`);
    const response = await fetch(entry.url);
    if (!response.ok) throw new Error(`download failed: HTTP ${response.status}`);
    fs.writeFileSync(wheel, Buffer.from(await response.arrayBuffer()));
    const size = fs.statSync(wheel).size;
    const sha256 = sha256File(wheel);
    if (size !== entry.size || sha256 !== entry.sha256) {
      throw new Error(`${entry.wheel}: got ${size} bytes / SHA-256 ${sha256}, expected ${entry.size} / ${entry.sha256}`);
    }
    console.log(`Verified ${entry.wheel} (SHA-256 ${sha256})`);
    const extracted = path.join(work, 'extracted');
    extractZip(wheel, extracted);
    return assemble({ from: extracted, out, platform, wheelSha256: sha256, version: entry.version });
  } finally {
    fs.rmSync(work, { recursive: true, force: true });
  }
}

function parseArgs(argv) {
  const args = { _: [] };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i].startsWith('--')) {
      args[argv[i].slice(2)] = argv[i + 1];
      i += 1;
    } else {
      args._.push(argv[i]);
    }
  }
  return args;
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const command = args._[0];
  if (command === 'fetch') {
    const manifest = await fetchRuntime({ platform: args.platform, out: path.resolve(args.out) });
    console.log(`OpenVINO ${manifest.version} (${manifest.platform}): ${Object.keys(manifest.files).length} files in ${args.out}`);
  } else if (command === 'assemble') {
    const manifest = assemble({
      from: path.resolve(args.from),
      out: path.resolve(args.out),
      platform: args.platform,
      wheelSha256: args['wheel-sha256'] || null,
      licenses: args.licenses ? path.resolve(args.licenses) : null,
    });
    console.log(`Assembled ${Object.keys(manifest.files).length} files into ${args.out}`);
  } else if (command === 'verify') {
    const manifest = verify(path.resolve(args.dir));
    console.log(`OK: ${Object.keys(manifest.files).length} files match ${MANIFEST}`);
  } else {
    console.error('usage: openvino-runtime.mjs fetch|assemble|verify ... (see header)');
    process.exit(2);
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(`❌ ${error.message}`);
    process.exit(1);
  });
}
