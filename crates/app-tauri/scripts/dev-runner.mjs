#!/usr/bin/env node
import { spawn } from 'child_process';
import http from 'http';
import path from 'path';
import fs from 'fs';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const appDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appDir, '..', '..');

console.log('=====================================================');
console.log('🚀 Clearcore Desktop - Starting Development Environment');
console.log('=====================================================');
console.log(`Platform: ${process.platform} (${process.arch})`);

// 1. Verify if daemon release or debug binary exists
const isWin = process.platform === 'win32';
const daemonBinName = isWin ? 'realtime-noise-service.exe' : 'realtime-noise-service';
const releaseBin = path.join(repoRoot, 'target', 'release', daemonBinName);
const debugBin = path.join(repoRoot, 'target', 'debug', daemonBinName);

if (fs.existsSync(releaseBin)) {
  console.log(`✓ Backend daemon found at: target/release/${daemonBinName}`);
} else if (fs.existsSync(debugBin)) {
  console.log(`✓ Backend daemon found at: target/debug/${daemonBinName}`);
} else {
  console.warn(`⚠️ Backend daemon ${daemonBinName} not found in target/release or target/debug.`);
  console.warn(`   Run 'cargo build --release -p realtime-noise-service' to build the audio engine.`);
}

// 2. Start Vite dev server with HMR
console.log('⚡ Starting Vite development server on http://127.0.0.1:5173...');
const viteBin = path.join(appDir, 'node_modules', '.bin', isWin ? 'vite.cmd' : 'vite');
const viteProc = spawn(viteBin, ['--host', '127.0.0.1', '--port', '5173'], {
  cwd: appDir,
  stdio: 'inherit',
  shell: isWin,
});

// Function to poll Vite HTTP until it responds
function waitForVite(port = 5173, timeoutMs = 20000) {
  const startTime = Date.now();
  return new Promise((resolve, reject) => {
    const check = () => {
      const req = http.get(`http://127.0.0.1:${port}`, (res) => {
        resolve();
      });
      req.on('error', () => {
        if (Date.now() - startTime > timeoutMs) {
          reject(new Error('Vite dev server timed out starting'));
        } else {
          setTimeout(check, 250);
        }
      });
    };
    check();
  });
}

// 3. Launch Electron when Vite is ready
async function startElectron() {
  try {
    await waitForVite(5173);
    console.log('✓ Vite server ready!');
    console.log('⚡ Launching Electron Desktop App in development mode (--dev)...');

    const electronBin = path.join(appDir, 'node_modules', '.bin', isWin ? 'electron.cmd' : 'electron');
    const electronProc = spawn(electronBin, ['electron/main.cjs', '--dev'], {
      cwd: appDir,
      stdio: 'inherit',
      shell: isWin,
    });

    electronProc.on('exit', (code) => {
      console.log(`\n[Dev] Electron closed (code=${code}). Stopping Vite dev server...`);
      try {
        viteProc.kill('SIGTERM');
      } catch {}
      process.exit(code || 0);
    });

    electronProc.on('error', (err) => {
      console.error('[Dev] Failed to spawn Electron:', err.message);
      try {
        viteProc.kill('SIGTERM');
      } catch {}
      process.exit(1);
    });
  } catch (err) {
    console.error('❌ Error initializing dev environment:', err.message);
    try {
      viteProc.kill('SIGTERM');
    } catch {}
    process.exit(1);
  }
}

process.on('SIGINT', () => {
  console.log('\n[Dev] Stopping dev environment...');
  try {
    viteProc.kill('SIGTERM');
  } catch {}
  process.exit(0);
});

startElectron();
