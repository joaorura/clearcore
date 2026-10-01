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
  const infSrc = path.join(repoRoot, 'platform', 'windows', 'driver', 'RealtimeNoise.inf');
  if (fs.existsSync(infSrc)) {
    fs.copyFileSync(infSrc, path.join(driverTargetDir, 'RealtimeNoise.inf'));
    console.log('✓ Bundled RealtimeNoise.inf driver specification');
  }
} else if (isMac) {
  const halDriver = path.join(repoRoot, 'platform', 'macos', 'HAL', 'RealtimeNoiseHAL.driver');
  if (fs.existsSync(halDriver)) {
    fs.cpSync(halDriver, path.join(driverTargetDir, 'RealtimeNoiseHAL.driver'), { recursive: true });
    console.log('✓ Bundled RealtimeNoiseHAL.driver');
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
Exec=clearcore --tray
Icon=clearcore
Terminal=false
Categories=AudioVideo;Audio;
Keywords=audio;microphone;noise;filter;clearcore;pipewire;
StartupWMClass=clearcore
`;
  fs.writeFileSync(path.join(bundleDir, 'clearcore.desktop'), desktopEntry, 'utf8');

  // install.sh
  const installSh = `#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "\${BASH_SOURCE[0]}")" && pwd)"
APP_NAME="clearcore"

echo "=== Clearcore Desktop Application Installer ==="

if [[ $EUID -eq 0 ]]; then
    # System-wide installation
    INSTALL_DIR="/opt/clearcore"
    BIN_LINK="/usr/local/bin/clearcore"
    DESKTOP_DIR="/usr/share/applications"
    ICON_DIR="/usr/share/icons/hicolor/256x256/apps"
else
    # User-local installation (no root required)
    INSTALL_DIR="\${HOME}/.local/share/clearcore"
    BIN_LINK="\${HOME}/.local/bin/clearcore"
    DESKTOP_DIR="\${HOME}/.local/share/applications"
    ICON_DIR="\${HOME}/.local/share/icons/hicolor/256x256/apps"
    mkdir -p "\${HOME}/.local/bin"
fi

echo "Installing Clearcore to \${INSTALL_DIR}..."
mkdir -p "\${INSTALL_DIR}"
cp -r "\${SCRIPT_DIR}"/* "\${INSTALL_DIR}/"
chmod +x "\${INSTALL_DIR}/clearcore"
find "\${INSTALL_DIR}/resources/bin" -type f -exec chmod +x {} + 2>/dev/null || true
find "\${INSTALL_DIR}/resources/scripts" -name "*.sh" -exec chmod +x {} + 2>/dev/null || true

echo "Creating launcher symlink at \${BIN_LINK}..."
mkdir -p "$(dirname "\${BIN_LINK}")"
ln -sf "\${INSTALL_DIR}/clearcore" "\${BIN_LINK}"

echo "Installing icon and desktop entry..."
mkdir -p "\${ICON_DIR}" "\${DESKTOP_DIR}"
if [[ -f "\${INSTALL_DIR}/resources/app/assets/icon.png" ]]; then
    cp "\${INSTALL_DIR}/resources/app/assets/icon.png" "\${ICON_DIR}/clearcore.png"
fi

sed "s|^Exec=.*|Exec=\${INSTALL_DIR}/clearcore --tray|" "\${INSTALL_DIR}/clearcore.desktop" > "\${DESKTOP_DIR}/clearcore.desktop"
chmod +x "\${DESKTOP_DIR}/clearcore.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "\${DESKTOP_DIR}" 2>/dev/null || true
fi

echo ""
echo "✅ Clearcore installed successfully!"
echo "You can launch Clearcore directly from your application menu or run 'clearcore' in your terminal."
`;
  const installPath = path.join(bundleDir, 'install.sh');
  fs.writeFileSync(installPath, installSh, 'utf8');
  fs.chmodSync(installPath, 0o755);

  // uninstall.sh
  const uninstallSh = `#!/usr/bin/env bash
set -euo pipefail

echo "=== Clearcore Uninstaller ==="

if [[ $EUID -eq 0 ]]; then
    rm -rf "/opt/clearcore"
    rm -f "/usr/local/bin/clearcore"
    rm -f "/usr/share/applications/clearcore.desktop"
    rm -f "/usr/share/icons/hicolor/256x256/apps/clearcore.png"
else
    rm -rf "\${HOME}/.local/share/clearcore"
    rm -f "\${HOME}/.local/bin/clearcore"
    rm -f "\${HOME}/.local/share/applications/clearcore.desktop"
    rm -f "\${HOME}/.local/share/icons/hicolor/256x256/apps/clearcore.png"
fi

echo "✅ Clearcore uninstalled successfully."
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
  fs.chmodSync(installPath, 0o755);

  const tarName = `${bundleName}.tar.gz`;
  const tarPath = path.join(releaseDir, tarName);
  console.log(`📦 Creating distribution archive: ${tarName}...`);
  try {
    execSync(`tar -czf "${tarPath}" -C "${releaseDir}" "${bundleName}"`, { stdio: 'inherit' });
    console.log(`✓ Distribution archive created at release/${tarName}`);
  } catch (err) {
    console.warn('Could not create tar.gz archive:', err.message);
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
