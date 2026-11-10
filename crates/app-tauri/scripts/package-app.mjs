#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';
import { createHash } from 'crypto';
import { bundleInto, embedsOpenVino, MANIFEST as OPENVINO_MANIFEST } from './openvino-runtime.mjs';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const appDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appDir, '..', '..');
const platform = process.env.TARGET_PLATFORM || process.platform;
const arch = process.env.TARGET_ARCH || process.arch;
const rustTriple = process.env.RUST_TARGET || (platform === 'win32' && arch === 'arm64' ? 'aarch64-pc-windows-msvc' : null);
const targetRelease = rustTriple && fs.existsSync(path.resolve(repoRoot, 'target', rustTriple, 'release'))
  ? path.resolve(repoRoot, 'target', rustTriple, 'release')
  : path.resolve(repoRoot, 'target', 'release');
const electronDist = path.resolve(appDir, 'node_modules', 'electron', 'dist');
const releaseDir = path.resolve(repoRoot, 'release');

const bundleName = `Clearcore-${platform}-${arch}`;
const bundleDir = path.join(releaseDir, bundleName);

console.log('=====================================================');
console.log(`📦 Packaging Clearcore Desktop Application for ${platform}-${arch}`);
console.log('=====================================================');

// 1. Ensure Frontend is built
console.log('🔨 Compiling frontend React bundle (npm run build)...');
execSync('npm run build', { cwd: appDir, stdio: 'inherit' });

// 2. Ensure release directory exists and is clean
if (fs.existsSync(bundleDir)) {
  console.log(`🧹 Cleaning previous bundle at ${bundleDir}...`);
  fs.rmSync(bundleDir, { recursive: true, force: true });
}
fs.mkdirSync(bundleDir, { recursive: true });

// 3. Verify Electron distribution
if (!fs.existsSync(electronDist)) {
  console.log(`⚡ Electron dist directory not found at ${electronDist}, attempting to download Electron binaries...`);
  const electronInstallJs = path.resolve(appDir, 'node_modules', 'electron', 'install.js');
  if (fs.existsSync(electronInstallJs)) {
    try {
      execSync(`node "${electronInstallJs}"`, { cwd: appDir, stdio: 'inherit' });
    } catch (e) {
      console.warn('⚠️ electron install.js execution failed:', e.message);
    }
  }
}

if (!fs.existsSync(electronDist)) {
  console.error(`❌ Electron dist directory not found at ${electronDist}`);
  console.error('Please run "npm install" inside crates/app-tauri first.');
  process.exit(1);
}

// 4. Copy Electron distribution files
console.log(`📋 Copying Electron runtime binaries into ${bundleName}...`);
fs.cpSync(electronDist, bundleDir, { recursive: true });

// 5. Rename main executable & handle macOS .app structure
const isWin = platform === 'win32';
const isMac = platform === 'darwin';

let resourcesDir = path.join(bundleDir, 'resources');

if (isMac) {
  // macOS Electron distribution contains Electron.app
  const oldApp = path.join(bundleDir, 'Electron.app');
  const newApp = path.join(bundleDir, 'Clearcore.app');
  if (fs.existsSync(oldApp)) {
    fs.renameSync(oldApp, newApp);
  }
  const appTarget = fs.existsSync(newApp) ? newApp : bundleDir;
  const oldExe = path.join(appTarget, 'Contents', 'MacOS', 'Electron');
  const newExe = path.join(appTarget, 'Contents', 'MacOS', 'Clearcore');
  if (fs.existsSync(oldExe)) {
    fs.renameSync(oldExe, newExe);
    fs.chmodSync(newExe, 0o755);
    console.log(`✓ Renamed application binary to Clearcore in Clearcore.app`);
  }
  resourcesDir = path.join(appTarget, 'Contents', 'Resources');
} else {
  const oldExe = path.join(bundleDir, isWin ? 'electron.exe' : 'electron');
  const newExe = path.join(bundleDir, isWin ? 'Clearcore.exe' : 'clearcore');

  if (fs.existsSync(oldExe)) {
    fs.renameSync(oldExe, newExe);
    if (!isWin) {
      fs.chmodSync(newExe, 0o755);
    }
    console.log(`✓ Renamed application binary to ${path.basename(newExe)}`);
  } else {
    console.warn(`⚠️ Executable ${oldExe} not found to rename.`);
  }
}

// 6. Assemble resources/app
const defaultAsar = path.join(resourcesDir, 'default_app.asar');
if (fs.existsSync(defaultAsar)) {
  fs.rmSync(defaultAsar, { force: true });
}
const appTargetDir = path.join(resourcesDir, 'app');
fs.mkdirSync(appTargetDir, { recursive: true });

console.log('📦 Bundling application code (HTML/JS/Assets) into resources/app...');
// Clean package.json for runtime
const pkg = JSON.parse(fs.readFileSync(path.join(appDir, 'package.json'), 'utf8'));
const runtimePkg = {
  name: 'clearcore',
  productName: 'Clearcore',
  version: pkg.version || '0.1.0',
  description: 'Clearcore Realtime AI Noise Suppression Virtual Microphone',
  main: 'electron/main.cjs',
  author: 'Clearcore Team',
};
fs.writeFileSync(path.join(appTargetDir, 'package.json'), JSON.stringify(runtimePkg, null, 2), 'utf8');

// Copy electron runtime scripts
fs.cpSync(path.join(appDir, 'electron'), path.join(appTargetDir, 'electron'), { recursive: true });
// Copy built dist
fs.cpSync(path.join(appDir, 'dist'), path.join(appTargetDir, 'dist'), { recursive: true });
// Copy assets
fs.cpSync(path.join(appDir, 'assets'), path.join(appTargetDir, 'assets'), { recursive: true });

// 7. Bundle Companion Binaries into resources/bin
const binTargetDir = path.join(resourcesDir, 'bin');
fs.mkdirSync(binTargetDir, { recursive: true });

console.log('⚙️ Bundling companion daemon and audio helpers into resources/bin...');
const daemonBinaryName = isWin ? 'realtime-noise-service.exe' : 'realtime-noise-service';
const serviceBin = path.join(targetRelease, daemonBinaryName);
if (fs.existsSync(serviceBin)) {
  const destService = path.join(binTargetDir, daemonBinaryName);
  fs.copyFileSync(serviceBin, destService);
  if (!isWin) fs.chmodSync(destService, 0o755);
  console.log(`✓ Bundled ${daemonBinaryName}`);
} else {
  console.warn(`⚠️ Daemon binary not found at ${serviceBin}. Make sure cargo build --release was run.`);
}

if (platform === 'linux') {
  const helperCandidates = [
    path.join(repoRoot, 'platform', 'linux', 'helper', 'build', 'pipewire_helper'),
    path.join(targetRelease, 'pipewire_helper'),
  ];
  for (const h of helperCandidates) {
    if (fs.existsSync(h)) {
      const destHelper = path.join(binTargetDir, 'pipewire_helper');
      fs.copyFileSync(h, destHelper);
      fs.chmodSync(destHelper, 0o755);
      console.log('✓ Bundled pipewire_helper');
      break;
    }
  }

  const libCandidates = [
    path.join(repoRoot, 'platform', 'linux', 'helper', 'lib', 'libclearcore_filter.so'),
    path.join(targetRelease, 'libclearcore_filter.so'),
  ];
  for (const l of libCandidates) {
    if (fs.existsSync(l)) {
      const destLib = path.join(binTargetDir, 'libclearcore_filter.so');
      fs.copyFileSync(l, destLib);
      fs.chmodSync(destLib, 0o755);
      // Also copy next to root executable for automatic ld loading
      fs.copyFileSync(l, path.join(bundleDir, 'libclearcore_filter.so'));
      console.log('✓ Bundled libclearcore_filter.so neural library');
      break;
    }
  }

}

// 7b. Bundle the OpenVINO runtime (Linux x64 and Windows x64) into resources/openvino.
// Built by `node scripts/openvino-runtime.mjs fetch` from the wheel pinned (URL, size, SHA-256) in
// packaging/openvino/openvino-runtime.lock; every file is re-checked against the manifest the
// fetch wrote. The daemon finds it relative to its own executable (resources/bin -> resources/
// openvino) and dlopens libopenvino_c by absolute path; plugins resolve next to libopenvino.
// ARM64 and macOS are not embedded (they run the pDFNet3 Pro on tract).
if (embedsOpenVino(platform, arch)) {
  const openvinoSrc = process.env.CLEARCORE_OPENVINO_RUNTIME_DIR
    || path.join(repoRoot, 'third_party', 'openvino-runtime', `${platform}-${arch}`);
  if (fs.existsSync(path.join(openvinoSrc, OPENVINO_MANIFEST))) {
    try {
      const { manifest, bytes } = bundleInto(openvinoSrc, path.join(resourcesDir, 'openvino'));
      console.log(`✓ Bundled OpenVINO ${manifest.version} runtime (${Object.keys(manifest.files).length} files, ${(bytes / 1048576).toFixed(1)} MiB) into resources/openvino`);
    } catch (err) {
      console.error(`❌ OpenVINO runtime at ${openvinoSrc} failed verification: ${err.message}`);
      process.exit(1);
    }
  } else if (process.env.CLEARCORE_REQUIRE_OPENVINO === '1') {
    console.error(`❌ OpenVINO runtime not found at ${openvinoSrc} (run scripts/openvino-runtime.mjs fetch)`);
    process.exit(1);
  } else {
    console.warn(`⚠️ OpenVINO runtime not found at ${openvinoSrc}; the package will run the pDFNet3 Pro on tract only.`);
  }
}

// 8. Bundle Neural Models (All Platforms)
// The pDFNet3 Pro v3 full (speaker-embedding conditioning) is the ONLY isolation model shipped:
// ONE stateful manifest export run by the daemon (tract/OpenVINO) and by the virtual-mic filter
// (ONNX Runtime/OpenVINO). Bundles:
// - models/pdfnet3/pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz into both resources/models/pdfnet3/
//   and bundle root models/pdfnet3/. Mandatory: the package build fails if the archive is missing or
//   its SHA-256 is not the one pinned in crates/model/src/pdfnet3_bundle.rs (the runtime refuses it
//   anyway, with no fallback).
// - models/enrollment/enrollment.onnx (enrollment v3: ECAPA speaker embedding [192] for the Pro v3)
//   likewise, SHA-256 pinned in the same file.
// - governance/model-assets/ (provenance and license records of the DeepFilterNet3 weights the
//   pDFNet3 Pro derives from) into bundle root and resources.
// The base DeepFilterNet3 (models/stateful/, its TensorRT engines and vendor/approved/) is NOT
// shipped: it stays in the repository for tests and development only.
console.log('🧠 Bundling the pDFNet3 Pro isolation model and the enrollment model...');

// Mirrors PDFNET3_PRO_V3_FULL_SHA256 and ENROLLMENT_V3_SHA256 in crates/model/src/pdfnet3_bundle.rs.
const PDFNET3_BUNDLE = {
  'pdfnet3-pro-v3-full-stateful-asset-v1.tar.gz': 'c1bee5593870bc326913e805803dbcfb19632236f0f42163635a5cc502d256ee',
};
const ENROLLMENT_V3_SHA256 = '82a292b95121dd7d7e82a2f4515497512693d499f06bafa0d3d627bed34ad3c1';
const pdfnet3Src = path.join(repoRoot, 'models', 'pdfnet3');
for (const [file, expected] of Object.entries(PDFNET3_BUNDLE)) {
  const source = path.join(pdfnet3Src, file);
  if (!fs.existsSync(source)) {
    console.error(`❌ Missing pDFNet3 Pro archive: ${source}`);
    process.exit(1);
  }
  const actual = createHash('sha256').update(fs.readFileSync(source)).digest('hex');
  if (actual !== expected) {
    console.error(`❌ pDFNet3 Pro archive ${file} has SHA-256 ${actual}, expected ${expected}`);
    process.exit(1);
  }
}
for (const dest of [path.join(resourcesDir, 'models', 'pdfnet3'), path.join(bundleDir, 'models', 'pdfnet3')]) {
  fs.mkdirSync(dest, { recursive: true });
  for (const file of [...Object.keys(PDFNET3_BUNDLE), 'README.md']) {
    const source = path.join(pdfnet3Src, file);
    if (fs.existsSync(source)) fs.copyFileSync(source, path.join(dest, file));
  }
}
console.log('✓ Bundled models/pdfnet3 (pDFNet3 Pro v3 full, SHA-256 verified) into resources/models/pdfnet3 and bundle root');

const modelsEnrollmentSrc = path.join(repoRoot, 'models', 'enrollment');
const enrollmentModel = path.join(modelsEnrollmentSrc, 'enrollment.onnx');
if (!fs.existsSync(enrollmentModel)) {
  console.error(`❌ models/enrollment/enrollment.onnx not found at ${modelsEnrollmentSrc}`);
  process.exit(1);
}
const enrollmentSha = createHash('sha256').update(fs.readFileSync(enrollmentModel)).digest('hex');
if (enrollmentSha !== ENROLLMENT_V3_SHA256) {
  console.error(`❌ enrollment.onnx has SHA-256 ${enrollmentSha}, expected ${ENROLLMENT_V3_SHA256} (enrollment v3)`);
  process.exit(1);
}
for (const dest of [path.join(resourcesDir, 'models', 'enrollment'), path.join(bundleDir, 'models', 'enrollment')]) {
  fs.mkdirSync(dest, { recursive: true });
  for (const file of ['enrollment.onnx', 'README.md']) {
    const source = path.join(modelsEnrollmentSrc, file);
    if (fs.existsSync(source)) fs.copyFileSync(source, path.join(dest, file));
  }
}
console.log('✓ Bundled models/enrollment (enrollment v3, SHA-256 verified) into resources/models/enrollment and bundle root');

const governanceDir = path.join(repoRoot, 'governance', 'model-assets');
if (fs.existsSync(governanceDir)) {
  const destBundleGov = path.join(bundleDir, 'governance', 'model-assets');
  const destResourcesGov = path.join(resourcesDir, 'governance', 'model-assets');

  fs.mkdirSync(destBundleGov, { recursive: true });
  fs.mkdirSync(destResourcesGov, { recursive: true });

  fs.cpSync(governanceDir, destBundleGov, { recursive: true });
  fs.cpSync(governanceDir, destResourcesGov, { recursive: true });
  console.log('✓ Bundled governance model trust policy and manifests');
}

// 9. Bundle Cross-Platform Virtual Mic Scripts into resources/scripts
const scriptsTargetDir = path.join(resourcesDir, 'scripts');
fs.mkdirSync(scriptsTargetDir, { recursive: true });

console.log('📜 Bundling virtual mic scripts into resources/scripts...');
const repoScriptsDir = path.join(repoRoot, 'scripts');
const scriptFiles = [
  'check-virtual-mic.sh',
  'check-virtual-mic-windows.ps1',
  'check-virtual-mic-macos.sh',
  'setup-autostart.sh',
  'setup-autostart.bat',
  'setup-autostart.ps1',
  'detect-hardware.sh',
  'detect-hardware-windows.ps1',
  'install-openvino.sh',
  'install-ryzenai.sh',
  'install-tensorrt.sh',
  'uninstall-linux.sh',
  'uninstall-macos.sh',
  'uninstall-windows.bat',
  'uninstall-windows.ps1',
  'sign-windows-binaries.ps1',
];

for (const sf of scriptFiles) {
  const src = path.join(repoScriptsDir, sf);
  if (fs.existsSync(src)) {
    const dest = path.join(scriptsTargetDir, sf);
    fs.copyFileSync(src, dest);
    if (sf.endsWith('.sh')) {
      fs.chmodSync(dest, 0o755);
    }
  }
}

// 10. Bundle Platform Driver Assets
const driverTargetDir = path.join(resourcesDir, 'driver');
fs.mkdirSync(driverTargetDir, { recursive: true });

if (isWin) {
  const driverDir = path.join(repoRoot, 'platform', 'windows', 'driver');
  const infSrc = path.join(driverDir, 'RealtimeNoise.inf');
  if (fs.existsSync(infSrc)) {
    fs.copyFileSync(infSrc, path.join(driverTargetDir, 'RealtimeNoise.inf'));
    console.log('✓ Bundled RealtimeNoise.inf driver specification');
  }

  // Bundle compiled kernel driver binaries if present
  const sysCandidates = [
    path.join(driverDir, 'RealtimeNoise.sys'),
    path.join(driverDir, 'x64', 'Release', 'RealtimeNoise.sys'),
    path.join(driverDir, 'x64', 'Debug', 'RealtimeNoise.sys'),
  ];
  for (const s of sysCandidates) {
    if (fs.existsSync(s)) {
      fs.copyFileSync(s, path.join(driverTargetDir, 'RealtimeNoise.sys'));
      console.log('✓ Bundled RealtimeNoise.sys kernel driver');
      break;
    }
  }

  const catCandidates = [
    path.join(driverDir, 'RealtimeNoise.cat'),
    path.join(driverDir, 'x64', 'Release', 'RealtimeNoise.cat'),
    path.join(driverDir, 'x64', 'Debug', 'RealtimeNoise.cat'),
  ];
  for (const c of catCandidates) {
    if (fs.existsSync(c)) {
      fs.copyFileSync(c, path.join(driverTargetDir, 'RealtimeNoise.cat'));
      console.log('✓ Bundled RealtimeNoise.cat catalog file');
      break;
    }
  }

  const readmeContent = `ClearCore Windows WaveRT Virtual Microphone Driver
===================================================
Status: Beta Driver Specification

To install and use this virtual microphone driver on Windows:
1. The driver RealtimeNoise.sys is built using Visual Studio and the Windows Driver Kit (WDK 10.0).
2. During the Beta testing phase, kernel drivers signed with a local/test certificate require
   Windows Test-Signing mode enabled:
     bcdedit /set testsigning on
   (run Command Prompt / PowerShell as Administrator, then reboot if prompted).
3. To sign the driver and user-mode binaries with a test certificate, use:
     powershell -File resources/scripts/sign-windows-binaries.ps1
4. To register/install the driver:
     pnputil /add-driver RealtimeNoise.inf /install
`;
  fs.writeFileSync(path.join(driverTargetDir, 'README-DRIVER.txt'), readmeContent, 'utf8');
} else if (isMac) {
  const halDriverCandidates = [
    path.join(repoRoot, 'platform', 'macos', 'HAL', 'RealtimeNoiseHAL.driver'),
    path.join(repoRoot, 'platform', 'macos', 'HAL', 'build', 'Build', 'Products', 'Release', 'RealtimeNoiseHAL.driver'),
    path.join(repoRoot, 'platform', 'macos', 'HAL', 'build', 'Release', 'RealtimeNoiseHAL.driver'),
  ];
  for (const h of halDriverCandidates) {
    if (fs.existsSync(h)) {
      fs.cpSync(h, path.join(driverTargetDir, 'RealtimeNoiseHAL.driver'), { recursive: true });
      console.log('✓ Bundled RealtimeNoiseHAL.driver');
      break;
    }
  }
}

// 11. Platform-specific standalone integration files
if (platform === 'linux') {
  console.log('🐧 Creating Linux desktop integration and one-click installer...');
  
  // clearcore.desktop
  const desktopEntry = `[Desktop Entry]
Type=Application
Name=Clearcore
GenericName=Noise Suppression Virtual Microphone
Comment=Realtime AI Noise Suppression Virtual Microphone (DeepFilterNet3)
Exec=clearcore
Icon=clearcore
Terminal=false
Categories=AudioVideo;Audio;
Keywords=audio;microphone;noise;filter;clearcore;pipewire;
StartupWMClass=clearcore
`;
  fs.writeFileSync(path.join(bundleDir, 'clearcore.desktop'), desktopEntry, 'utf8');

  // install.sh
  // Source of truth is a real, lintable file (also exercised by scripts/linux-install.test.sh)
  const installSh = fs.readFileSync(path.join(__dirname, 'linux-install.sh'), 'utf8');
  const installPath = path.join(bundleDir, 'install.sh');
  fs.writeFileSync(installPath, installSh, 'utf8');
  fs.chmodSync(installPath, 0o755);

  // Comprehensive uninstall.sh
  const uninstallSh = `#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "\${SCRIPT_DIR}/resources/scripts/uninstall-linux.sh" ]]; then
    exec "\${SCRIPT_DIR}/resources/scripts/uninstall-linux.sh" "$@"
fi

echo "=== Clearcore Uninstaller ==="

# 1. Stop all running processes
echo "Stopping Clearcore processes..."
pkill -x "clearcore" >/dev/null 2>&1 || true
pkill -x "pipewire_helper" >/dev/null 2>&1 || true
pkill -x "realtime-noise-service" >/dev/null 2>&1 || true
pkill -f "^pw-loopback.*realtime-noise" >/dev/null 2>&1 || true

# 2. Disable and remove user systemd units
if command -v systemctl >/dev/null 2>&1; then
    systemctl --user stop realtime-noise-helper.service >/dev/null 2>&1 || true
    systemctl --user stop realtime-noise.service >/dev/null 2>&1 || true
    systemctl --user disable realtime-noise-helper.service >/dev/null 2>&1 || true
    systemctl --user disable realtime-noise.service >/dev/null 2>&1 || true
    rm -f "\${HOME}/.config/systemd/user/realtime-noise-helper.service" 2>/dev/null || true
    rm -f "\${HOME}/.config/systemd/user/realtime-noise.service" 2>/dev/null || true
    systemctl --user daemon-reload >/dev/null 2>&1 || true
fi

# 3. Remove autostart desktop entries
rm -f "\${HOME}/.config/autostart/clearcore.desktop" 2>/dev/null || true
rm -f "\${HOME}/.config/autostart/realtime-noise.desktop" 2>/dev/null || true

# 4. Remove PipeWire nodes (source, capture, sink)
if command -v pw-cli >/dev/null 2>&1; then
    NODE_IDS=$(pw-cli list-objects Node 2>/dev/null | awk '
        $1 == "id" { cur_id = $2; sub(/,/, "", cur_id) }
        $0 ~ "node.name = \\"realtime-noise" || $0 ~ "node.description = \\\".*Realtime Noise" {
            if (cur_id != "") { print cur_id }
        }
    ' | sort -u)
    for nid in \${NODE_IDS}; do
        pw-cli destroy "\${nid}" >/dev/null 2>&1 || true
    done
fi

# 5. Clean runtime locks, sockets, and shared memory
RUNTIME_DIR="\${XDG_RUNTIME_DIR:-/tmp}"
rm -f "\${RUNTIME_DIR}/clearcore_state" 2>/dev/null || true
rm -f "\${RUNTIME_DIR}/hippocamp_pipewire_helper.lock" 2>/dev/null || true
rm -f "\${RUNTIME_DIR}/realtime-noise.sock" 2>/dev/null || true
rm -f /tmp/hippocamp_pipewire_helper*.lock 2>/dev/null || true
rm -f /tmp/clearcore*.lock 2>/dev/null || true
rm -f /tmp/clearcore_state* 2>/dev/null || true
rm -f /tmp/realtime-noise*.sock 2>/dev/null || true
rm -f /run/user/*/hippocamp_pipewire_helper*.lock 2>/dev/null || true
rm -f /run/user/*/clearcore_state* 2>/dev/null || true
rm -f /run/user/*/realtime-noise*.sock 2>/dev/null || true
rm -f /tmp/realtime-noise-helper.log 2>/dev/null || true
rm -f /tmp/realtime-noise-service.log 2>/dev/null || true
rm -f /tmp/clearcore-*.log 2>/dev/null || true
rm -rf "\${HOME}/.local/state/clearcore" 2>/dev/null || true
rm -rf "\${HOME}/.config/clearcore" 2>/dev/null || true

# 6. Remove installed files
if [[ $EUID -eq 0 ]]; then
    rm -rf "/opt/clearcore" 2>/dev/null || true
    rm -f "/usr/local/bin/clearcore" 2>/dev/null || true
    rm -f "/usr/share/applications/clearcore.desktop" 2>/dev/null || true
    find "/usr/share/icons/hicolor" -path '*/apps/clearcore.png' -delete 2>/dev/null || true
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "/usr/share/applications" 2>/dev/null || true
    fi
else
    rm -rf "\${HOME}/.local/share/clearcore" 2>/dev/null || true
    rm -f "\${HOME}/.local/bin/clearcore" 2>/dev/null || true
    rm -f "\${HOME}/.local/share/applications/clearcore.desktop" 2>/dev/null || true
    find "\${HOME}/.local/share/icons/hicolor" -path '*/apps/clearcore.png' -delete 2>/dev/null || true
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database "\${HOME}/.local/share/applications" 2>/dev/null || true
    fi
fi

echo "✅ Clearcore uninstalled successfully from Linux."
`;
  const uninstallPath = path.join(bundleDir, 'uninstall.sh');
  fs.writeFileSync(uninstallPath, uninstallSh, 'utf8');
  fs.chmodSync(uninstallPath, 0o755);

  // AppRun script and icon for AppImage packaging compatibility
  const appRunContent = `#!/usr/bin/env bash
HERE="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
export PATH="\${HERE}:\${HERE}/resources/bin:\${PATH}"
export LD_LIBRARY_PATH="\${HERE}:\${HERE}/resources/bin:\${LD_LIBRARY_PATH}"
exec "\${HERE}/clearcore" "$@"
`;
  const appRunPath = path.join(bundleDir, 'AppRun');
  fs.writeFileSync(appRunPath, appRunContent, 'utf8');
  fs.chmodSync(appRunPath, 0o755);

  const iconSrc = path.join(appDir, 'assets', 'icon.png');
  if (fs.existsSync(iconSrc)) {
    fs.copyFileSync(iconSrc, path.join(bundleDir, 'clearcore.png'));
  }

  // Tarball archive for distribution
  const tarName = `${bundleName}.tar.gz`;
  const tarPath = path.join(releaseDir, tarName);
  console.log(`📦 Creating distribution archive: ${tarName}...`);
  try {
    execSync(`tar -czf "${tarPath}" -C "${releaseDir}" "${bundleName}"`, { stdio: 'inherit' });
    console.log(`✓ Distribution archive created at release/${tarName}`);
  } catch (err) {
    console.warn('Could not create tar.gz archive:', err.message);
  }

  // If appimagetool is available, build .AppImage
  const appImageName = `${bundleName}.AppImage`;
  const appImagePath = path.join(releaseDir, appImageName);
  try {
    execSync('appimagetool --version', { stdio: 'ignore' });
    console.log(`📦 Creating AppImage archive: ${appImageName}...`);
    execSync(`ARCH=x86_64 appimagetool --appimage-extract-and-run "${bundleDir}" "${appImagePath}"`, {
      stdio: 'inherit',
      env: { ...process.env, ARCH: 'x86_64' },
    });
    console.log(`✓ AppImage created at release/${appImageName}`);
  } catch {
    // appimagetool not in PATH or not installed locally
  }
} else if (platform === 'darwin') {
  console.log('🍎 Creating macOS application bundle integration and installer...');

  const installSh = `#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
APP_DIR="\${SCRIPT_DIR}/Clearcore.app"
DEST_APP="/Applications/Clearcore.app"
USER_APP="\${HOME}/Applications/Clearcore.app"

echo "=== Clearcore macOS Application Installer ==="

if [[ -d "\${APP_DIR}" ]]; then
    if [[ -w "/Applications" ]]; then
        echo "Installing Clearcore.app to /Applications..."
        rm -rf "\${DEST_APP}"
        cp -R "\${APP_DIR}" "\${DEST_APP}"
    else
        echo "Installing Clearcore.app to \${USER_APP}..."
        mkdir -p "\${HOME}/Applications"
        rm -rf "\${USER_APP}"
        cp -R "\${APP_DIR}" "\${USER_APP}"
    fi
fi

# Run virtual microphone check and HAL installation
echo "Configuring CoreAudio HAL virtual microphone..."
"\${SCRIPT_DIR}/Clearcore.app/Contents/Resources/scripts/check-virtual-mic-macos.sh" --recreate || true

echo ""
echo "✅ Clearcore installed successfully!"
echo "You can launch Clearcore from Launchpad or Applications."
`;
  const installPath = path.join(bundleDir, 'install.sh');
  fs.writeFileSync(installPath, installSh, 'utf8');
  const uninstallSh = `#!/usr/bin/env bash
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "\${SCRIPT_DIR}/Clearcore.app/Contents/Resources/scripts/uninstall-macos.sh" ]]; then
    exec "\${SCRIPT_DIR}/Clearcore.app/Contents/Resources/scripts/uninstall-macos.sh" "$@"
fi

echo "Stopping Clearcore processes..."
killall "Clearcore" >/dev/null 2>&1 || true
killall "realtime-noise-service" >/dev/null 2>&1 || true
pkill -f "Clearcore.app" >/dev/null 2>&1 || true

CURRENT_UID=$(id -u)
PLIST_USER="\${HOME}/Library/LaunchAgents/com.clearcore.realtime-noise.plist"
if [[ -f "\${PLIST_USER}" ]]; then
    launchctl bootout "gui/\${CURRENT_UID}" "\${PLIST_USER}" >/dev/null 2>&1 || true
    launchctl unload "\${PLIST_USER}" >/dev/null 2>&1 || true
    rm -f "\${PLIST_USER}"
fi

DRIVER_NAME="RealtimeNoiseHAL.driver"
rm -rf "\${HOME}/Library/Audio/Plug-Ins/HAL/\${DRIVER_NAME}" 2>/dev/null || true
rm -rf "\${HOME}/Library/Audio/Plug-Ins/HAL/\${DRIVER_NAME}.disabled" 2>/dev/null || true
sudo rm -rf "/Library/Audio/Plug-Ins/HAL/\${DRIVER_NAME}" 2>/dev/null || true
sudo rm -rf "/Library/Audio/Plug-Ins/HAL/\${DRIVER_NAME}.disabled" 2>/dev/null || true

if command -v launchctl >/dev/null 2>&1; then
    sudo launchctl kickstart -k system/com.apple.audio.coreaudiod 2>/dev/null || true
fi
killall coreaudiod >/dev/null 2>&1 || sudo killall coreaudiod >/dev/null 2>&1 || true

rm -rf "/Applications/Clearcore.app" 2>/dev/null || sudo rm -rf "/Applications/Clearcore.app" 2>/dev/null || true
rm -rf "\${HOME}/Applications/Clearcore.app" 2>/dev/null || true
rm -rf "\${HOME}/Library/Application Support/Clearcore" 2>/dev/null || true
rm -rf "\${HOME}/Library/Preferences/com.clearcore.*" 2>/dev/null || true

echo "✅ Clearcore uninstalled successfully from macOS."
`;
  const uninstallPath = path.join(bundleDir, 'uninstall.sh');
  fs.writeFileSync(uninstallPath, uninstallSh, 'utf8');
  fs.chmodSync(uninstallPath, 0o755);

  const tarName = `${bundleName}.tar.gz`;
  const tarPath = path.join(releaseDir, tarName);
  console.log(`📦 Creating distribution archive: ${tarName}...`);
  try {
    execSync(`tar -czf "${tarPath}" -C "${releaseDir}" "${bundleName}"`, { stdio: 'inherit' });
    console.log(`✓ Distribution archive created at release/${tarName}`);
  } catch (err) {
    console.warn('Could not create tar.gz archive:', err.message);
  }

  // Create DMG on macOS
  const dmgName = `${bundleName}.dmg`;
  const dmgPath = path.join(releaseDir, dmgName);
  console.log(`📦 Creating distribution DMG: ${dmgName}...`);
  try {
    execSync(`hdiutil create -volname "Clearcore" -srcfolder "${bundleDir}" -ov -format UDZO "${dmgPath}"`, { stdio: 'inherit' });
    console.log(`✓ Distribution DMG created at release/${dmgName}`);
  } catch (err) {
    console.warn('Could not create DMG archive:', err.message);
  }
} else if (platform === 'win32') {
  console.log('🪟 Creating Windows uninstallers and helper scripts...');
  const uninstallBat = `@echo off
setlocal
set SCRIPT_DIR=%~dp0
if exist "%SCRIPT_DIR%resources\\scripts\\uninstall-windows.ps1" (
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%resources\\scripts\\uninstall-windows.ps1"
) else if exist "%SCRIPT_DIR%uninstall.ps1" (
    powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%SCRIPT_DIR%uninstall.ps1"
)
pause
`;
  fs.writeFileSync(path.join(bundleDir, 'uninstall.bat'), uninstallBat, 'utf8');

  const uninstallPs1 = `# Clearcore Windows Uninstaller
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$PsScript = Join-Path $ScriptDir "resources\\scripts\\uninstall-windows.ps1"
if (Test-Path $PsScript) {
    & $PsScript
} else {
    Write-Host "Encerrando Clearcore..." -ForegroundColor Yellow
    Stop-Process -Name "Clearcore" -Force -ErrorAction SilentlyContinue
    Stop-Process -Name "realtime-noise-service" -Force -ErrorAction SilentlyContinue
}
`;
  fs.writeFileSync(path.join(bundleDir, 'uninstall.ps1'), uninstallPs1, 'utf8');

  // Test code signing for Windows binaries prior to zip compression
  if (isWin) {
    const signScript = path.join(repoRoot, 'scripts', 'sign-windows-binaries.ps1');
    if (fs.existsSync(signScript)) {
      console.log('🔏 Applying test code signing to Windows executables...');
      try {
        execSync(`powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${signScript}"`, { stdio: 'inherit' });
      } catch (signErr) {
        console.warn('Test signing skipped or failed:', signErr.message);
      }
    }
  }

  const zipName = `${bundleName}.zip`;
  const zipPath = path.join(releaseDir, zipName);
  console.log(`📦 Creating distribution zip archive: ${zipName}...`);
  try {
    execSync(`tar -a -c -f "${zipPath}" -C "${releaseDir}" "${bundleName}"`, { stdio: 'inherit' });
    console.log(`✓ Distribution archive created at release/${zipName}`);
  } catch {
    try {
      execSync(`powershell.exe -NoProfile -Command "Compress-Archive -Path '${bundleDir}' -DestinationPath '${zipPath}' -Force"`, { stdio: 'inherit' });
      console.log(`✓ Distribution archive created at release/${zipName}`);
    } catch (err) {
      console.warn('Could not create zip archive:', err.message);
    }
  }

  // Compile NSIS / Inno Setup installer if tools are installed
  const installerDir = path.join(repoRoot, 'platform', 'windows', 'installer');
  const nsiScript = path.join(installerDir, 'Clearcore-Setup.nsi');
  if (fs.existsSync(nsiScript)) {
    try {
      execSync('makensis /VERSION', { stdio: 'ignore' });
      console.log('📦 Compiling NSIS turnkey installer (Clearcore-Setup.exe)...');
      execSync(`makensis "${nsiScript}"`, { cwd: installerDir, stdio: 'inherit' });
      console.log('✓ NSIS installer compiled at release/Clearcore-Setup.exe');
    } catch {
      // makensis not found or failed, try Inno Setup
      const issScript = path.join(installerDir, 'Clearcore-Setup.iss');
      if (fs.existsSync(issScript)) {
        try {
          execSync('iscc /?', { stdio: 'ignore' });
          console.log('📦 Compiling Inno Setup installer (Clearcore-Setup.exe)...');
          execSync(`iscc "${issScript}"`, { cwd: installerDir, stdio: 'inherit' });
          console.log('✓ Inno Setup installer compiled at release/Clearcore-Setup.exe');
        } catch {
          // iscc not found
        }
      }
    }
  }
}

console.log('=====================================================');
console.log(`✅ Build and packaging complete!`);
console.log(`📁 Standalone application bundle: ${bundleDir}`);
if (platform === 'linux') {
  console.log(`🚀 To run directly (portable): ${path.join(bundleDir, 'clearcore')}`);
  console.log(`💾 To install on system/user: cd ${bundleDir} && ./install.sh`);
} else if (platform === 'win32') {
  console.log(`🚀 To run directly: ${path.join(bundleDir, 'Clearcore.exe')}`);
} else if (platform === 'darwin') {
  console.log(`🚀 To run directly: open ${path.join(bundleDir, 'Clearcore.app')}`);
  console.log(`💾 To install on system/user: cd ${bundleDir} && ./install.sh`);
}
console.log('=====================================================');
