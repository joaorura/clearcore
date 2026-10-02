#!/usr/bin/env node
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import { execSync } from 'child_process';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

const appDir = path.resolve(__dirname, '..');
const repoRoot = path.resolve(appDir, '..', '..');
const releaseDir = path.resolve(repoRoot, 'release');
const targetRelease = path.resolve(repoRoot, 'target', 'release');
const electronDist = path.resolve(appDir, 'node_modules', 'electron', 'dist');

const platform = process.platform;
const arch = process.arch;
const bundleName = `Clearcore-${platform}-${arch}`;
const bundleDir = path.join(releaseDir, bundleName);

console.log('=====================================================');
console.log(`📦 Packaging Clearcore Desktop Application for ${platform}-${arch}`);
console.log('=====================================================');

// 1. Ensure Frontend is built
const distHtml = path.join(appDir, 'dist', 'index.html');
if (!fs.existsSync(distHtml)) {
  console.log('🔨 Compiling frontend React bundle (npm run build)...');
  execSync('npm run build', { cwd: appDir, stdio: 'inherit' });
} else {
  console.log('✓ Frontend React bundle already compiled in dist/');
}

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

  // Bundle approved model assets and governance metadata
  const vendorApproved = path.join(repoRoot, 'vendor', 'approved');
  if (fs.existsSync(vendorApproved)) {
    fs.cpSync(vendorApproved, path.join(bundleDir, 'vendor', 'approved'), { recursive: true });
    console.log('✓ Bundled vendor/approved DeepFilterNet3 neural model');
  }
  const governanceDir = path.join(repoRoot, 'governance', 'model-assets');
  if (fs.existsSync(governanceDir)) {
    fs.cpSync(governanceDir, path.join(bundleDir, 'governance', 'model-assets'), { recursive: true });
    console.log('✓ Bundled governance model trust policy and manifests');
  }
}

// 8. Bundle Cross-Platform Virtual Mic Scripts into resources/scripts
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

// 9. Bundle Platform Driver Assets
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

// 10. Platform-specific standalone integration files
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

# 4. Remove PipeWire node
if command -v pw-cli >/dev/null 2>&1; then
    NODE_ID=$(pw-cli list-objects Node 2>/dev/null | awk -v name="\\"realtime-noise-source\\"" '
        $1 == "id" { id = $2; sub(/,/, "", id) }
        $0 ~ "node.name = " name { print id; exit }
    ')
    if [[ -n "\${NODE_ID}" ]]; then
        pw-cli destroy "\${NODE_ID}" >/dev/null 2>&1 || true
    fi
fi

# 5. Clean runtime locks, sockets, and shared memory
RUNTIME_DIR="\${XDG_RUNTIME_DIR:-/tmp}"
rm -f "\${RUNTIME_DIR}/clearcore_state" 2>/dev/null || true
rm -f "\${RUNTIME_DIR}/hippocamp_pipewire_helper.lock" 2>/dev/null || true
rm -f "\${RUNTIME_DIR}/realtime-noise.sock" 2>/dev/null || true
rm -f /tmp/realtime-noise-helper.log 2>/dev/null || true
rm -f /tmp/realtime-noise-service.log 2>/dev/null || true

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
sudo rm -rf "/Library/Audio/Plug-Ins/HAL/\${DRIVER_NAME}" 2>/dev/null || true
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
