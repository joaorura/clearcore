#!/usr/bin/env node

import { execSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appTauriDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appTauriDir, '../..');

function getTargetTriple() {
  if (process.env.TARGET) {
    return process.env.TARGET;
  }
  try {
    const output = execSync('rustc -vV', { encoding: 'utf8' });
    for (const line of output.split('\n')) {
      if (line.startsWith('host: ')) {
        return line.substring(6).trim();
      }
    }
  } catch (err) {
    console.warn('Failed to query rustc for target triple, defaulting to x86_64-unknown-linux-gnu', err);
  }
  return 'x86_64-unknown-linux-gnu';
}

function main() {
  const targetTriple = getTargetTriple();
  console.log(`[prepare-sidecar] Target triple: ${targetTriple}`);

  console.log('[prepare-sidecar] Building realtime-noise-service in release mode...');
  try {
    execSync('cargo build -p realtime-noise-service --release', {
      cwd: repoRoot,
      stdio: 'inherit',
    });
  } catch (err) {
    console.error('[prepare-sidecar] cargo build failed:', err);
    // Don't throw immediately if binary already exists in target
  }

  const isWindows = targetTriple.includes('windows') || process.platform === 'win32';
  const binaryName = isWindows ? 'realtime-noise-service.exe' : 'realtime-noise-service';
  const targetBinaryName = isWindows
    ? `realtime-noise-service-${targetTriple}.exe`
    : `realtime-noise-service-${targetTriple}`;

  const srcBinary = path.join(repoRoot, 'target', 'release', binaryName);
  const destDir = path.join(appTauriDir, 'src-tauri', 'binaries');
  const destBinary = path.join(destDir, targetBinaryName);

  if (!fs.existsSync(destDir)) {
    fs.mkdirSync(destDir, { recursive: true });
  }

  if (fs.existsSync(srcBinary)) {
    fs.copyFileSync(srcBinary, destBinary);
    if (!isWindows) {
      fs.chmodSync(destBinary, 0o755);
    }
    console.log(`[prepare-sidecar] Successfully copied ${srcBinary} -> ${destBinary}`);
  } else {
    console.warn(`[prepare-sidecar] Source binary ${srcBinary} does not exist yet. Ensure release build runs before packaging.`);
  }
}

main();
