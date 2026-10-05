const { app, BrowserWindow, Tray, Menu, nativeImage, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');
const net = require('net');
const os = require('os');
const { execFile, spawn } = require('child_process');
const clearcoreState = require('./clearcore-state.cjs');
const { planCaptureLinks } = require('./capture-link-plan.cjs');
const { parseHardwareJson } = require('./hardware-json.cjs');
const { resolveBackendSelection } = require('./backend-selection.cjs');
const updater = require('./updater.cjs');

// ClearCore Runtime Application Version
const APP_VERSION = '0.1.0-beta.3';
app.setVersion(APP_VERSION);

// Enforce single instance lock (in production)
const isDevMode = process.argv.includes('--dev');
if (!isDevMode) {
  const gotTheLock = app.requestSingleInstanceLock();
  if (!gotTheLock) {
    console.log('[Clearcore] Another instance is already running. Focusing existing instance and exiting.');
    app.quit();
    process.exit(0);
  }
}

let mainWindow = null;
let tray = null;
let isQuitting = false;
let currentMode = 'Active';
let daemonChildProcess = null;
let daemonSpawnedByApp = false;
let currentStatus = {
  state: 'Running',
  is_terminal: false,
  can_restart: true,
  mode: 'Active',
  crash_count_15m: 0,
  total_crashes: 0,
};

let currentVirtualMicStatus = {
  platform: process.platform,
  platform_label:
    process.platform === 'win32'
      ? 'Windows (WaveRT Driver)'
      : process.platform === 'darwin'
      ? 'macOS (CoreAudio HAL)'
      : 'Linux (PipeWire)',
  present: false,
  node_id: null,
  node_name: 'realtime-noise-source',
  node_description: 'Realtime Noise Virtual Microphone',
  driver_status: 'Unknown',
  is_default: false,
};

// Check if launched with --tray or --hidden or from autostart
const startInTray =
  process.argv.includes('--tray') ||
  process.argv.includes('--hidden') ||
  process.argv.includes('--minimized');

// Cross-platform IPC endpoint resolution
function getIpcEndpoint() {
  if (process.platform === 'win32') {
    // Windows supports localhost TCP bridge (port 49215)
    return { port: 49215, host: '127.0.0.1' };
  }
  if (process.platform === 'darwin') {
    return { path: '/tmp/realtime-noise.sock' };
  }
  const uid = typeof process.getuid === 'function' ? process.getuid() : os.userInfo().uid;
  if (process.env.XDG_RUNTIME_DIR) {
    return { path: path.join(process.env.XDG_RUNTIME_DIR, 'realtime-noise.sock') };
  }
  if (fs.existsSync(`/run/user/${uid}`)) {
    return { path: `/run/user/${uid}/realtime-noise.sock` };
  }
  return { path: '/tmp/realtime-noise.sock' };
}

// Low-level IPC request to realtime-noise-service daemon
function sendIpcRequest(command, payload = {}) {
  return new Promise((resolve, reject) => {
    const endpoint = getIpcEndpoint();
    const req =
      JSON.stringify({
        version: 'realtime-noise.v1',
        request_id: `req-${Date.now()}`,
        command,
        payload,
      }) + '\n';

    const client = net.createConnection(endpoint, () => {
      client.write(req);
    });

    let buffer = '';
    client.on('data', (chunk) => {
      buffer += chunk.toString();
      if (buffer.includes('\n')) {
        client.end();
      }
    });

    client.on('end', () => {
      try {
        const parsed = JSON.parse(buffer.trim());
        if (parsed.status === 'Ok') {
          resolve(parsed.payload);
        } else {
          reject(new Error(parsed.error ? parsed.error.message : 'IPC request rejected'));
        }
      } catch (err) {
        reject(new Error(`Failed to parse daemon response: ${err.message}`));
      }
    });

    const targetDesc = endpoint.path ? endpoint.path : `${endpoint.host}:${endpoint.port}`;
    client.on('error', (err) => {
      reject(new Error(`Daemon unreachable at ${targetDesc}: ${err.message}`));
    });

    // Timeout after 3 seconds
    client.setTimeout(3000, () => {
      client.destroy();
      reject(new Error('Daemon IPC timeout'));
    });
  });
}

// Sidecar Daemon Discovery & Supervision
function findDaemonBinaryPath() {
  const binName = process.platform === 'win32' ? 'realtime-noise-service.exe' : 'realtime-noise-service';
  const candidates = [
    // 1. Packaged locations (resources/bin or resources/)
    path.join(process.resourcesPath, 'bin', binName),
    path.join(process.resourcesPath, binName),
    // 2. Relative to application directory
    path.resolve(__dirname, '..', 'bin', binName),
    path.resolve(__dirname, '..', '..', '..', 'bin', binName),
    // 3. Workspace / development target directories
    path.resolve(__dirname, '..', '..', '..', 'target', 'release', binName),
    path.resolve(process.cwd(), 'target', 'release', binName),
    path.resolve(__dirname, '..', '..', '..', 'target', 'debug', binName),
    path.resolve(process.cwd(), 'target', 'debug', binName),
  ];

  for (const c of candidates) {
    if (fs.existsSync(c)) {
      return c;
    }
  }
  return null;
}

async function isDaemonResponsive() {
  try {
    const status = await sendIpcRequest('GetStatus');
    return Boolean(status && status.mode);
  } catch {
    return false;
  }
}

async function ensureDaemonRunning() {
  // If already running (e.g. system service or previous run), do nothing
  if (await isDaemonResponsive()) {
    console.log('[Clearcore Daemon] Serviço já está em execução e comunicando via IPC.');
    return true;
  }

  const daemonBin = findDaemonBinaryPath();
  if (!daemonBin) {
    console.warn('[Clearcore Daemon] Binário realtime-noise-service não encontrado.');
    return false;
  }

  console.log(`[Clearcore Daemon] Iniciando sidecar daemon: ${daemonBin} --run`);
  try {
    const userData = app.getPath('userData');
    if (!fs.existsSync(userData)) {
      fs.mkdirSync(userData, { recursive: true });
    }
    const logFile = path.join(userData, 'service.log');
    const logFd = fs.openSync(logFile, 'a');

    daemonChildProcess = spawn(daemonBin, ['--run'], {
      detached: false,
      stdio: ['ignore', logFd, logFd],
      windowsHide: true,
    });

    daemonSpawnedByApp = true;

    daemonChildProcess.on('error', (err) => {
      console.error('[Clearcore Daemon] Erro no processo do serviço:', err.message);
      daemonChildProcess = null;
      daemonSpawnedByApp = false;
    });

    daemonChildProcess.on('exit', (code, signal) => {
      console.warn(`[Clearcore Daemon] Processo do serviço finalizou (code=${code}, signal=${signal})`);
      daemonChildProcess = null;
      daemonSpawnedByApp = false;
    });

    // Wait up to 3.5 seconds for daemon socket to accept IPC requests
    for (let attempt = 1; attempt <= 18; attempt++) {
      await new Promise((r) => setTimeout(r, 200));
      if (await isDaemonResponsive()) {
        console.log(`[Clearcore Daemon] Conexão IPC estabelecida com sucesso na tentativa ${attempt}.`);
        return true;
      }
    }

    console.warn('[Clearcore Daemon] Daemon iniciado, mas IPC ainda não respondeu.');
    return false;
  } catch (err) {
    console.error('[Clearcore Daemon] Falha ao iniciar daemon:', err.message);
    return false;
  }
}

function stopDaemon() {
  if (daemonSpawnedByApp && daemonChildProcess) {
    console.log('[Clearcore Daemon] Encerrando daemon interno iniciado pelo aplicativo...');
    try {
      sendIpcRequest('Shutdown').catch(() => {});
    } catch {}
    try {
      daemonChildProcess.kill('SIGTERM');
    } catch {}
    daemonChildProcess = null;
    daemonSpawnedByApp = false;
  }
}

// Script Path Resolution Helper (Package and Dev aware)
function findScriptPath(filename) {
  const candidates = [
    path.join(process.resourcesPath, 'scripts', filename),
    path.join(process.resourcesPath, filename),
    path.resolve(__dirname, '..', 'scripts', filename),
    path.resolve(__dirname, '..', '..', '..', 'scripts', filename),
    path.resolve(process.cwd(), 'scripts', filename),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) {
      return c;
    }
  }
  return candidates[candidates.length - 1];
}

// Cross-Platform Virtual Microphone Script Execution
function executeVirtualMicScript(action) {
  return new Promise((resolve) => {
    let scriptPath = '';
    let command = '';
    let args = [];

    const isWin = process.platform === 'win32';
    const isMac = process.platform === 'darwin';

    if (isWin) {
      command = 'powershell.exe';
      scriptPath = findScriptPath('check-virtual-mic-windows.ps1');
      args = ['-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', scriptPath, '-Json'];
      if (action === 'recreate') args.push('-Recreate');
      else if (action === 'stop') args.push('-Stop');
      else if (action === 'set_default') args.push('-SetDefault');
      else if (action === 'status') args.push('-Status');
    } else if (isMac) {
      command = '/bin/bash';
      scriptPath = findScriptPath('check-virtual-mic-macos.sh');
      args = [scriptPath, '--json'];
      if (action === 'recreate') args.push('--recreate');
      else if (action === 'stop') args.push('--stop');
      else if (action === 'set_default') args.push('--set-default');
      else if (action === 'status') args.push('--status');
    } else {
      command = '/bin/bash';
      scriptPath = findScriptPath('check-virtual-mic.sh');
      args = [scriptPath, '--json'];
      if (action === 'recreate') args.push('--recreate');
      else if (action === 'stop') args.push('--stop');
      else if (action === 'set_default') args.push('--set-default');
      else if (action === 'status') args.push('--status');
    }

    execFile(command, args, (error, stdout) => {
      const defaultPlatformLabel = isWin
        ? 'Windows (WaveRT Driver)'
        : isMac
        ? 'macOS (CoreAudio HAL)'
        : 'Linux (PipeWire / WirePlumber)';

      if (error && !stdout) {
        resolve({
          platform: process.platform,
          platform_label: defaultPlatformLabel,
          present: false,
          node_id: null,
          node_name: 'realtime-noise-source',
          node_description: 'Realtime Noise Virtual Microphone',
          driver_status: action === 'stop' ? 'Stopped' : 'Error',
          is_default: false,
          error: error.message,
        });
        return;
      }
      try {
        const parsed = JSON.parse(stdout.trim());
        resolve(parsed);
      } catch (err) {
        resolve({
          platform: process.platform,
          platform_label: defaultPlatformLabel,
          present: false,
          node_id: null,
          node_name: 'realtime-noise-source',
          node_description: 'Realtime Noise Virtual Microphone',
          driver_status: action === 'stop' ? 'Stopped' : 'ParseError',
          is_default: false,
          error: `Parse error: ${err.message}`,
        });
      }
    });
  });
}

function queryVirtualMicStatus() {
  return executeVirtualMicScript('status');
}

function runRecreateVirtualMic() {
  return executeVirtualMicScript('recreate');
}

function runStopVirtualMic() {
  return executeVirtualMicScript('stop');
}

function runSetDefaultVirtualMic() {
  return executeVirtualMicScript('set_default');
}

// Active startup verification: check if created; if not created, auto-create!
async function verifyAndAutoCreateVirtualMicOnStartup() {
  const plat =
    process.platform === 'win32' ? 'Windows' : process.platform === 'darwin' ? 'macOS' : 'Linux';
  console.log(`[Startup] Verificando criacao do microfone virtual em ${plat}...`);
  const initial = await queryVirtualMicStatus();
  if (initial && initial.present) {
    console.log(`[Startup] Microfone virtual verificado em ${plat} (ID: ${initial.node_id})`);
    currentVirtualMicStatus = initial;
  } else {
    console.warn(
      `[Startup] Microfone virtual NAO detectado em ${plat}! Executando recuperacao e criacao automatica...`
    );
    const recreated = await runRecreateVirtualMic();
    currentVirtualMicStatus = recreated;
    if (recreated.present) {
      console.log(
        `[Startup] Microfone virtual criado e registrado em ${plat} com sucesso! (ID: ${recreated.node_id})`
      );
    } else {
      console.error(
        `[Startup] Falha na criacao automatica do microfone virtual em ${plat}:`,
        recreated.error
      );
    }
  }
  writeClearcoreSharedState({ mode: currentMode });
  updateTrayMenu();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
  }
}

// Application Preferences (Persistent User Configuration)
function getAppSettingsFile() {
  const userData = app.getPath('userData');
  return path.join(userData, 'app-settings.json');
}

function readAppSettings() {
  try {
    const file = getAppSettingsFile();
    if (fs.existsSync(file)) {
      return JSON.parse(fs.readFileSync(file, 'utf8'));
    }
  } catch (err) {
    console.warn('[Clearcore] Could not read app-settings.json:', err.message);
  }
  return { startActivated: true };
}

function writeAppSettings(updates = {}) {
  try {
    const userData = app.getPath('userData');
    if (!fs.existsSync(userData)) {
      fs.mkdirSync(userData, { recursive: true });
    }
    const current = readAppSettings();
    const updated = { ...current, ...updates };
    fs.writeFileSync(getAppSettingsFile(), JSON.stringify(updated, null, 2), 'utf8');
    return updated;
  } catch (err) {
    console.warn('[Clearcore] Could not write app-settings.json:', err.message);
    return { startActivated: true, ...updates };
  }
}

let isServiceRunning = true;

async function startAudioService() {
  console.log('[Clearcore Operation] Iniciando serviço e ativando microfone virtual...');
  isServiceRunning = true;
  await ensureDaemonRunning();
  const res = await runRecreateVirtualMic();
  currentVirtualMicStatus = res;
  writeClearcoreSharedState({ mode: currentMode });
  updateTrayMenu();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('service-state-update', { isRunning: true });
    mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
  }
  return { success: true, isRunning: true, virtualMic: currentVirtualMicStatus };
}

async function stopAudioService() {
  console.log('[Clearcore Operation] Parando serviço e desativando microfone virtual...');
  isServiceRunning = false;
  const res = await runStopVirtualMic();
  currentVirtualMicStatus = res;
  writeClearcoreSharedState({ mode: 'Mute' });
  updateTrayMenu();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('service-state-update', { isRunning: false });
    mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
  }
  return { success: true, isRunning: false, virtualMic: currentVirtualMicStatus };
}

function cleanupAndStopAll() {
  console.log('[Clearcore Lifecycle] Encerrando app: garantindo remoção do microfone virtual e parada do daemon...');
  try {
    if (process.platform === 'linux') {
      try {
        const { execSync } = require('child_process');
        const scriptPath = findScriptPath('check-virtual-mic.sh');
        if (scriptPath && fs.existsSync(scriptPath)) {
          try {
            execSync(`bash "${scriptPath}" --stop`, { timeout: 2000 });
          } catch {}
        }
        execSync(
          'systemctl --user stop realtime-noise-helper.service 2>/dev/null || true; ' +
          'systemctl --user reset-failed realtime-noise-helper.service 2>/dev/null || true; ' +
          'pkill -f pipewire_helper 2>/dev/null || true; ' +
          'pkill -f "pw-loopback.*realtime-noise" 2>/dev/null || true; ' +
          'if command -v pw-cli >/dev/null 2>&1; then ' +
          '  pw-cli list-objects Node 2>/dev/null | awk \'$1=="id"{id=$2;sub(/,/,"",id)} $0~/node\\.name = "realtime-noise/{if (id!="") print id}\' | while read -r id; do [ -n "$id" ] && pw-cli destroy "$id" 2>/dev/null || true; done; ' +
          'fi; ' +
          'rm -f "${XDG_RUNTIME_DIR:-/tmp}/hippocamp_pipewire_helper.lock" /tmp/hippocamp_pipewire_helper.lock 2>/dev/null || true;',
          { timeout: 2000 }
        );
      } catch {}
    } else if (process.platform === 'win32') {
      try {
        const { execSync } = require('child_process');
        execSync(
          'powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Get-PnpDevice | Where-Object { $_.InstanceId -like \'*RealtimeNoise*\' } | Disable-PnpDevice -Confirm:$false -ErrorAction SilentlyContinue; Stop-Service -Name RealtimeNoise -ErrorAction SilentlyContinue"',
          { timeout: 2000 }
        );
      } catch {}
    }
  } catch {}
  stopDaemon();
}

// Autostart management
function getAutostartPaths() {
  const homeDir = os.homedir();
  const autostartDir = path.join(homeDir, '.config', 'autostart');
  return {
    autostartDir,
    clearcoreDesktop: path.join(autostartDir, 'clearcore.desktop'),
    realtimeNoiseDesktop: path.join(autostartDir, 'realtime-noise.desktop'),
  };
}

function isAutostartEnabled() {
  if (process.platform === 'linux') {
    const { clearcoreDesktop, realtimeNoiseDesktop } = getAutostartPaths();
    return fs.existsSync(clearcoreDesktop) || fs.existsSync(realtimeNoiseDesktop);
  }
  const settings = app.getLoginItemSettings();
  return Boolean(settings.openAtLogin);
}

function setAutostartEnabled(enabled) {
  if (process.platform === 'linux') {
    const { autostartDir, clearcoreDesktop, realtimeNoiseDesktop } = getAutostartPaths();
    if (enabled) {
      if (!fs.existsSync(autostartDir)) {
        fs.mkdirSync(autostartDir, { recursive: true });
      }
      const isPackaged = app.isPackaged || !process.execPath.endsWith('electron');
      const execCmd = isPackaged
        ? `"${process.execPath}" --tray`
        : `"${process.execPath}" "${path.join(__dirname, 'main.cjs')}" --tray`;
      const iconPath = getTrayIconPath('Active');
      const content = `[Desktop Entry]
Type=Application
Name=ClearCore
Comment=ClearCore - Supressão de Ruído em Tempo Real (Bandeja)
Exec=${execCmd}
Icon=${iconPath}
Terminal=false
Categories=AudioVideo;Audio;
X-GNOME-Autostart-enabled=true
`;
      fs.writeFileSync(clearcoreDesktop, content, 'utf8');
      fs.chmodSync(clearcoreDesktop, 0o755);
      if (fs.existsSync(realtimeNoiseDesktop)) {
        try { fs.unlinkSync(realtimeNoiseDesktop); } catch {}
      }
    } else {
      if (fs.existsSync(clearcoreDesktop)) {
        try { fs.unlinkSync(clearcoreDesktop); } catch {}
      }
      if (fs.existsSync(realtimeNoiseDesktop)) {
        try { fs.unlinkSync(realtimeNoiseDesktop); } catch {}
      }
    }
  } else {
    // Windows & macOS native login item support via Electron
    app.setLoginItemSettings({
      openAtLogin: enabled,
      openAsHidden: true,
      args: ['--tray'],
    });
  }

  const newState = isAutostartEnabled();
  updateTrayMenu();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('autostart-update', newState);
  }
  return newState;
}

// Icon helper
function getTrayIconPath(mode) {
  const file =
    mode === 'Bypass' ? 'tray-bypass.png' : mode === 'Mute' ? 'tray-mute.png' : 'tray-active.png';
  const candidates = [
    path.join(process.resourcesPath, 'app', 'assets', file),
    path.join(process.resourcesPath, 'assets', file),
    path.resolve(__dirname, '..', 'assets', file),
    path.resolve(__dirname, 'assets', file),
  ];
  for (const c of candidates) {
    if (fs.existsSync(c)) return c;
  }
  return candidates[2];
}

// Shared Memory state synchronization with native helper
function clearcoreStatePath() {
  const runtimeDir = process.env.XDG_RUNTIME_DIR || '/tmp';
  return path.join(runtimeDir, 'clearcore_state');
}

function writeClearcoreSharedState(updates = {}) {
  try {
    clearcoreState.updateStateFile(clearcoreStatePath(), updates);
  } catch (err) {
    console.warn('[Clearcore] Could not update shared state:', err.message);
  }
}

// The service owns the studio preset (settings.json); the native helper only reads the copy kept in
// the shared state file. Call it with the preset reported by GetStatus (`res.preset`). The IPC
// protocol has no command to change the preset yet, and the service does not report one yet, so
// `preset` is undefined today and nothing is written until it does.
function syncPresetToHelper(preset) {
  if (process.platform !== 'linux' || !clearcoreState.isPreset(preset)) return;
  if (clearcoreState.readPreset(clearcoreStatePath()) !== preset) {
    writeClearcoreSharedState({ preset });
  }
}

function updateTrayMenu() {
  if (!tray) return;

  const iconPath = getTrayIconPath(currentMode);
  if (fs.existsSync(iconPath)) {
    const icon = nativeImage.createFromPath(iconPath);
    tray.setImage(icon);
  }

  const autostart = isAutostartEnabled();
  const platformLabel = currentVirtualMicStatus.platform_label || 'Virtual';

  const appSettings = readAppSettings();
  const startActivated = appSettings.startActivated !== false;

  const contextMenu = Menu.buildFromTemplate([
    {
      label: `ClearCore [${isServiceRunning ? currentMode.toUpperCase() : 'PARADO'}]`,
      enabled: false,
    },
    {
      label: isServiceRunning ? '🛑 Parar Serviço de Áudio' : '▶️ Iniciar Serviço de Áudio',
      click: async () => {
        if (isServiceRunning) {
          await stopAudioService();
        } else {
          await startAudioService();
        }
      },
    },
    { type: 'separator' },
    {
      label: currentVirtualMicStatus.present
        ? `Microfone (${platformLabel}): 🟢 Ativo (ID: ${currentVirtualMicStatus.node_id || 'OK'})`
        : `Microfone (${platformLabel}): 🔴 Desconectado`,
      click: async () => {
        if (!currentVirtualMicStatus.present) {
          const res = await runRecreateVirtualMic();
          currentVirtualMicStatus = res;
          updateTrayMenu();
          if (mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
          }
        }
      },
    },
    ...(currentVirtualMicStatus.present
      ? [
          {
            label: currentVirtualMicStatus.is_default
              ? '✓ Microfone Padrão do Sistema'
              : 'Definir como Microfone Padrão',
            enabled: !currentVirtualMicStatus.is_default,
            click: async () => {
              const res = await runSetDefaultVirtualMic();
              currentVirtualMicStatus = res;
              updateTrayMenu();
              if (mainWindow && !mainWindow.isDestroyed()) {
                mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
              }
            },
          },
        ]
      : [
          {
            label: 'Criar / Instalar Microfone Virtual',
            click: async () => {
              const res = await runRecreateVirtualMic();
              currentVirtualMicStatus = res;
              updateTrayMenu();
              if (mainWindow && !mainWindow.isDestroyed()) {
                mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
              }
            },
          },
        ]),
    { type: 'separator' },
    {
      label: 'Ativo (DeepFilterNet3)',
      type: 'radio',
      checked: currentMode === 'Active',
      click: async () => {
        try {
          await sendIpcRequest({ SetMode: 'Active' });
          currentMode = 'Active';
          writeClearcoreSharedState({ mode: 'Active' });
          updateTrayMenu();
          if (mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.webContents.send('status-update', { mode: 'Active' });
          }
        } catch (err) {
          console.error('Failed to set Active mode:', err);
        }
      },
    },
    {
      label: 'Bypass (Passagem Direta)',
      type: 'radio',
      checked: currentMode === 'Bypass',
      click: async () => {
        try {
          await sendIpcRequest({ SetMode: 'Bypass' });
          currentMode = 'Bypass';
          writeClearcoreSharedState({ mode: 'Bypass' });
          updateTrayMenu();
          if (mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.webContents.send('status-update', { mode: 'Bypass' });
          }
        } catch (err) {
          console.error('Failed to set Bypass mode:', err);
        }
      },
    },
    {
      label: 'Mudo (Silêncio Digital)',
      type: 'radio',
      checked: currentMode === 'Mute',
      click: async () => {
        try {
          await sendIpcRequest({ SetMode: 'Mute' });
          currentMode = 'Mute';
          writeClearcoreSharedState({ mode: 'Mute' });
          updateTrayMenu();
          if (mainWindow && !mainWindow.isDestroyed()) {
            mainWindow.webContents.send('status-update', { mode: 'Mute' });
          }
        } catch (err) {
          console.error('Failed to set Mute mode:', err);
        }
      },
    },
    { type: 'separator' },
    {
      label: 'Abrir Painel de Controle',
      click: () => {
        if (mainWindow) {
          mainWindow.show();
          mainWindow.focus();
        }
      },
    },
    {
      label: 'Ocultar para a Bandeja',
      click: () => {
        if (mainWindow) {
          mainWindow.hide();
        }
      },
    },
    { type: 'separator' },
    {
      label: 'Iniciar com o Sistema',
      type: 'checkbox',
      checked: autostart,
      click: (item) => {
        setAutostartEnabled(item.checked);
        updateTrayMenu();
      },
    },
    {
      label: 'Iniciar Ativado ao Abrir o App',
      type: 'checkbox',
      checked: startActivated,
      click: (item) => {
        writeAppSettings({ startActivated: item.checked });
        updateTrayMenu();
        if (mainWindow && !mainWindow.isDestroyed()) {
          mainWindow.webContents.send('start-activated-config-update', item.checked);
        }
      },
    },
    {
      label: 'Reiniciar Geração de Áudio',
      click: async () => {
        try {
          await sendIpcRequest('RestartGeneration');
        } catch (err) {
          console.error('Restart generation failed:', err);
        }
      },
    },
    { type: 'separator' },
    {
      label: 'Sair do ClearCore',
      click: () => {
        isQuitting = true;
        cleanupAndStopAll();
        app.quit();
      },
    },
  ]);

  tray.setToolTip(`ClearCore (${currentMode})`);
  tray.setContextMenu(contextMenu);
}

function createTray() {
  const iconPath = getTrayIconPath(currentMode);
  const icon = fs.existsSync(iconPath)
    ? nativeImage.createFromPath(iconPath)
    : nativeImage.createEmpty();

  tray = new Tray(icon);
  tray.setToolTip('ClearCore');

  tray.on('click', () => {
    if (!mainWindow) return;
    if (mainWindow.isVisible()) {
      mainWindow.hide();
    } else {
      mainWindow.show();
      mainWindow.focus();
    }
  });

  updateTrayMenu();
}

let currentInputDevices = [];
let selectedInputDeviceId = null;

// Enumerate physical/system audio input devices (excluding Clearcore virtual mics)
function enumerateSystemInputDevices() {
  const isWin = process.platform === 'win32';
  const isMac = process.platform === 'darwin';
  const devices = [];

  if (isWin) {
    try {
      const psCmd = 'Get-CimInstance Win32_SoundDevice | Select-Object -Property DeviceID, Name, Status | ConvertTo-Json';
      const out = require('child_process').execSync(`powershell.exe -NoProfile -Command "${psCmd}"`, { encoding: 'utf8', timeout: 3000 });
      const parsed = JSON.parse(out);
      const items = Array.isArray(parsed) ? parsed : [parsed];
      items.forEach((item, idx) => {
        if (item && item.Name && !item.Name.toLowerCase().includes('realtime') && !item.Name.toLowerCase().includes('clearcore')) {
          devices.push({
            id: item.DeviceID || `win-audio-${idx}`,
            name: item.Name,
            is_default: idx === 0,
          });
        }
      });
    } catch {
      // Fallback
    }
  } else if (isMac) {
    try {
      const out = require('child_process').execSync('system_profiler SPAudioDataType -json', { encoding: 'utf8', timeout: 3000 });
      const data = JSON.parse(out);
      const audioData = data.SPAudioDataType || [];
      audioData.forEach((item) => {
        const devList = item._items || [];
        devList.forEach((dev) => {
          if (dev._name && !dev._name.toLowerCase().includes('realtime') && !dev._name.toLowerCase().includes('clearcore')) {
            devices.push({
              id: dev._name,
              name: dev._name,
              is_default: false,
            });
          }
        });
      });
    } catch {
      // Fallback
    }
  } else {
    // Linux PipeWire / WirePlumber
    try {
      const out = require('child_process').execSync('wpctl status', { encoding: 'utf8', timeout: 3000 });
      const lines = out.split('\n');
      let inAudio = false;
      let inSources = false;
      for (const line of lines) {
        if (line.trim().startsWith('Audio')) {
          inAudio = true;
          continue;
        }
        if (inAudio && (line.trim().startsWith('Video') || line.trim().startsWith('Settings'))) {
          inAudio = false;
          inSources = false;
          continue;
        }
        if (inAudio && line.includes('Sources:')) {
          inSources = true;
          continue;
        }
        if (inSources && (line.includes('Filters:') || line.includes('Streams:') || line.includes('Sinks:'))) {
          inSources = false;
          continue;
        }
        if (inSources) {
          const match = line.match(/(?:\*|\s)\s*(\d+)\.\s+([^\[]+)(?:\[.*\])?/);
          if (match) {
            const id = match[1].trim();
            const name = match[2].trim();
            const isDefault = line.includes('*');
            if (!name.toLowerCase().includes('realtime') && !name.toLowerCase().includes('clearcore')) {
              devices.push({ id, name, is_default: isDefault });
            }
          }
        }
      }
    } catch {
      // Fallback
    }
  }

  currentInputDevices = devices;
  return devices;
}

function setSystemInputDevice(deviceId) {
  selectedInputDeviceId = deviceId;
  writeClearcoreSharedState({ targetNodeId: deviceId });

  if (process.platform === 'linux' && /^\d+$/.test(deviceId)) {
    try {
      const targetId = deviceId;
      let nodeName = '';
      try {
        const nodeInfo = require('child_process').execSync(`pw-cli info ${targetId}`, { encoding: 'utf8', timeout: 2000 });
        const nameMatch = nodeInfo.match(/node\.name = "([^"]+)"/);
        if (nameMatch) nodeName = nameMatch[1];
      } catch {}

      const pwOut = require('child_process').execSync('pw-link -o', { encoding: 'utf8', timeout: 2000 });
      const devLinks = require('child_process').execSync('pw-link -l', { encoding: 'utf8', timeout: 2000 });

      // Unlink previous capture input links
      const capMatches = [...devLinks.matchAll(/\|\<-\s*([^\s]+)/g)];
      for (const cm of capMatches) {
        const srcPort = cm[1];
        try {
          require('child_process').execSync(`pw-link -d "${srcPort}" "realtime-noise-capture:input_FL" 2>/dev/null || true`);
          require('child_process').execSync(`pw-link -d "${srcPort}" "realtime-noise-capture:input_FR" 2>/dev/null || true`);
          require('child_process').execSync(`pw-link -d "${srcPort}" "realtime-noise-capture:input_MONO" 2>/dev/null || true`);
        } catch {}
      }

      // Link target ports: stereo FL/FR -> FL/FR; mono (e.g. Bluetooth headset) -> every input
      let pwIn = '';
      try { pwIn = require('child_process').execSync('pw-link -i', { encoding: 'utf8', timeout: 2000 }); } catch {}
      const plan = planCaptureLinks({
        nodeName,
        outPorts: pwOut.split('\n'),
        inPorts: pwIn.split('\n'),
      });
      for (const { src, dst } of plan) {
        try { require('child_process').execFileSync('pw-link', [src, dst], { stdio: 'ignore', timeout: 2000 }); } catch {}
      }
    } catch (err) {
      console.warn('[Clearcore] Link update for input device:', err.message);
    }
  }
  updateTrayMenu();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('input-devices-update', {
      devices: currentInputDevices,
      selectedId: selectedInputDeviceId,
    });
  }
  return { success: true, selectedId: selectedInputDeviceId };
}


function createWindow() {
  const assetsDir = path.join(__dirname, '..', 'assets');
  const iconPath = path.join(assetsDir, 'icon.png');

  mainWindow = new BrowserWindow({
    width: 860,
    height: 760,
    minWidth: 700,
    minHeight: 580,
    show: false, // Start hidden to obey "inicie na bandeja"
    icon: fs.existsSync(iconPath) ? iconPath : undefined,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: false,
    },
  });

  // Enable microphone/media permissions for navigator.mediaDevices
  if (mainWindow.webContents && mainWindow.webContents.session) {
    mainWindow.webContents.session.setPermissionCheckHandler((_webContents, permission) => {
      if (permission === 'media' || permission === 'microphone') {
        return true;
      }
      return true;
    });
    mainWindow.webContents.session.setPermissionRequestHandler((_webContents, permission, callback) => {
      if (permission === 'media' || permission === 'microphone') {
        return callback(true);
      }
      return callback(true);
    });
  }

  // Minimize to tray
  mainWindow.on('minimize', (event) => {
    event.preventDefault();
    mainWindow.hide();
  });

  // Close hides to tray unless quitting
  mainWindow.on('close', (event) => {
    if (!isQuitting) {
      event.preventDefault();
      mainWindow.hide();
    }
  });

  // Load React app
  const distHtml = path.join(__dirname, '..', 'dist', 'index.html');
  const isDev = process.argv.includes('--dev');

  if (isDev) {
    const devUrl = 'http://127.0.0.1:5173';
    const tryLoadDevUrl = async () => {
      try {
        await mainWindow.loadURL(devUrl);
      } catch (err) {
        console.warn(`[Dev] Could not connect to Vite at ${devUrl} (${err.message}), retrying in 500ms...`);
        setTimeout(async () => {
          try {
            await mainWindow.loadURL(devUrl);
          } catch {
            console.warn('[Dev] Falling back to built dist/index.html');
            if (fs.existsSync(distHtml)) {
              mainWindow.loadFile(distHtml);
            }
          }
        }, 500);
      }
    };
    tryLoadDevUrl();
  } else if (fs.existsSync(distHtml)) {
    mainWindow.loadFile(distHtml);
  } else {
    mainWindow.loadURL('http://127.0.0.1:5173');
  }

  // Show window in foreground if not explicitly asked to start minimized in tray
  if (!startInTray) {
    const showAndFocus = () => {
      if (mainWindow && !mainWindow.isDestroyed() && !mainWindow.isVisible()) {
        mainWindow.show();
        mainWindow.focus();
      }
    };

    mainWindow.once('ready-to-show', showAndFocus);
    mainWindow.webContents.once('did-finish-load', showAndFocus);

    // Guaranteed fallbacks: ensure foreground window appears across Wayland / X11 / Windows
    setTimeout(showAndFocus, 400);
    setTimeout(showAndFocus, 1200);
  }
}

// Sync status periodically
async function pollDaemonStatus() {
  try {
    const res = await sendIpcRequest('GetStatus');
    if (res && res.mode) {
      currentStatus = res;
      if (res.mode !== currentMode) {
        currentMode = res.mode;
        updateTrayMenu();
      }
      syncPresetToHelper(res.preset);
    }
  } catch {
    // Daemon not running yet or unreachable
  }

  try {
    const micRes = await queryVirtualMicStatus();
    if (
      micRes.present !== currentVirtualMicStatus.present ||
      micRes.node_id !== currentVirtualMicStatus.node_id ||
      micRes.is_default !== currentVirtualMicStatus.is_default ||
      micRes.platform_label !== currentVirtualMicStatus.platform_label
    ) {
      currentVirtualMicStatus = micRes;
      updateTrayMenu();
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send('virtual-mic-update', currentVirtualMicStatus);
      }
    }
  } catch {
    // Ignore error
  }
}

app.whenReady().then(async () => {
  // The tray is optional: GNOME shows no tray icon without the AppIndicator extension,
  // and a failure here must not stop the window from opening (it stays reachable from the
  // app menu: launching again hits 'second-instance' and shows the window).
  try {
    createTray();
  } catch (err) {
    tray = null;
    console.warn('[Clearcore] System tray unavailable, continuing without it:', err.message);
  }
  createWindow();

  // 1. Check user config: Start activated (default: true) or stopped
  const settings = readAppSettings();
  const shouldStartActivated = settings.startActivated !== false;

  if (shouldStartActivated) {
    console.log('[Startup] ClearCore configurado para iniciar ATIVADO (Padrão).');
    await ensureDaemonRunning();
    await verifyAndAutoCreateVirtualMicOnStartup();
    isServiceRunning = true;
  } else {
    console.log('[Startup] ClearCore configurado para subir DESATIVADO (Parado).');
    isServiceRunning = false;
    currentVirtualMicStatus = await runStopVirtualMic();
    writeClearcoreSharedState({ mode: 'Mute' });
    updateTrayMenu();
  }

  // Initial poll and recurring heartbeat
  pollDaemonStatus();
  setInterval(pollDaemonStatus, 2500);

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    } else if (mainWindow) {
      mainWindow.show();
    }
  });
});

app.on('second-instance', () => {
  if (mainWindow) {
    if (mainWindow.isMinimized()) mainWindow.restore();
    mainWindow.show();
    mainWindow.focus();
  }
});

app.on('before-quit', () => {
  isQuitting = true;
  cleanupAndStopAll();
});

process.on('SIGINT', () => {
  isQuitting = true;
  cleanupAndStopAll();
  process.exit(0);
});

process.on('SIGTERM', () => {
  isQuitting = true;
  cleanupAndStopAll();
  process.exit(0);
});

// IPC handlers for frontend
ipcMain.handle('get_app_version', () => {
  return APP_VERSION;
});

ipcMain.handle('start_audio_service', async () => {
  return await startAudioService();
});

ipcMain.handle('stop_audio_service', async () => {
  return await stopAudioService();
});

ipcMain.handle('get_service_running_state', () => {
  return { isRunning: isServiceRunning };
});

ipcMain.handle('get_start_activated_config', () => {
  return readAppSettings().startActivated !== false;
});

ipcMain.handle('set_start_activated_config', (_event, enabled) => {
  const updated = writeAppSettings({ startActivated: Boolean(enabled) });
  updateTrayMenu();
  return updated.startActivated;
});

ipcMain.handle('get_status', async () => {
  return await sendIpcRequest('GetStatus');
});

ipcMain.handle('set_mode', async (_event, args) => {
  const mode = args && args.mode ? args.mode : 'Active';
  const res = await sendIpcRequest({ SetMode: mode });
  currentMode = mode;
  writeClearcoreSharedState({ mode });
  updateTrayMenu();
  return res;
});

ipcMain.handle('restart_generation', async () => {
  return await sendIpcRequest('RestartGeneration');
});

ipcMain.handle('get_diagnostics', async () => {
  return await sendIpcRequest('GetDiagnostics');
});

ipcMain.handle('get_autostart', () => {
  return isAutostartEnabled();
});

ipcMain.handle('set_autostart', (_event, enabled) => {
  return setAutostartEnabled(Boolean(enabled));
});

ipcMain.handle('minimize_to_tray', () => {
  if (mainWindow) {
    mainWindow.hide();
  }
});

ipcMain.handle('quit_app', () => {
  isQuitting = true;
  cleanupAndStopAll();
  app.quit();
});

ipcMain.handle('get_virtual_mic_status', async () => {
  const res = await queryVirtualMicStatus();
  currentVirtualMicStatus = res;
  return res;
});

ipcMain.handle('recreate_virtual_mic', async () => {
  const res = await runRecreateVirtualMic();
  currentVirtualMicStatus = res;
  updateTrayMenu();
  return res;
});

ipcMain.handle('set_default_virtual_mic', async () => {
  const res = await runSetDefaultVirtualMic();
  currentVirtualMicStatus = res;
  updateTrayMenu();
  return res;
});

ipcMain.handle('get_input_devices', () => {
  return enumerateSystemInputDevices();
});

ipcMain.handle('set_input_device', (_event, args) => {
  const deviceId = typeof args === 'string' ? args : (args && args.deviceId ? args.deviceId : '');
  return setSystemInputDevice(deviceId);
});

let currentSelectedBackend = 'auto';

function queryHardwareBackends() {
  const isLinux = process.platform === 'linux';
  const isWin = process.platform === 'win32';
  const isMac = process.platform === 'darwin';

  // Motivo curto da falha do detector (script/powershell); vai no resultado como
  // `detection_error` para a UI nao mostrar "Runtime Ausente" sem explicar nada.
  let detectionError = null;
  const shortError = (err) => String((err && err.message) || err).split('\n')[0].slice(0, 200);

  if (isLinux) {
    try {
      const scriptPath = findScriptPath('detect-hardware.sh');
      if (fs.existsSync(scriptPath)) {
        const out = require('child_process').execSync(`"${scriptPath}" --json`, { encoding: 'utf8', timeout: 15000 });
        const result = parseHardwareJson(out);
        if (result.ok) {
          return {
            ...result.data,
            active_backend: currentSelectedBackend,
          };
        }
        detectionError = result.error;
        console.warn('Failed to parse hardware detection output:', result.error);
      }
    } catch (err) {
      detectionError = `falha ao executar detect-hardware.sh: ${shortError(err)}`;
      console.warn('Failed to detect hardware via script:', err.message);
    }
  }

  if (isWin) {
    try {
      const scriptPath = findScriptPath('detect-hardware-windows.ps1');
      if (fs.existsSync(scriptPath)) {
        const out = require('child_process').execSync(
          `powershell.exe -NoProfile -ExecutionPolicy Bypass -File "${scriptPath}" -Json`,
          { encoding: 'utf8', timeout: 6000 }
        );
        const result = parseHardwareJson(out);
        if (result.ok) {
          return {
            ...result.data,
            active_backend: currentSelectedBackend,
          };
        }
        detectionError = result.error;
        console.warn('Failed to parse Windows hardware detection output:', result.error);
      }
    } catch (err) {
      detectionError = `falha ao executar detect-hardware-windows.ps1: ${shortError(err)}`;
      console.warn('Failed to detect Windows hardware via powershell:', err.message);
    }
  }

  // Cross-platform intelligent hardware detection (dynamically inspects CPU and GPU)
  const cpus = os.cpus();
  const cpuModel = (cpus && cpus[0] && cpus[0].model) ? cpus[0].model.trim() : 'Processador Host CPU';
  const isIntel = /intel/i.test(cpuModel);
  const isAmd = /amd|ryzen/i.test(cpuModel);

  // Probe for discrete NVIDIA GPU without hanging
  let hasNvidiaGpu = false;
  try {
    if (isLinux && fs.existsSync('/dev/nvidia0')) hasNvidiaGpu = true;
    if (isWin && fs.existsSync('C:\\Windows\\System32\\nvapi64.dll')) hasNvidiaGpu = true;
  } catch {}

  // Resolve Auto backend
  let fallbackAutoResolved = { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };
  if (isMac && process.arch === 'arm64') {
    fallbackAutoResolved = { id: 'apple_coreml', name: 'Apple Silicon (CoreML)' };
  } else if (hasNvidiaGpu) {
    fallbackAutoResolved = { id: 'nvidia_tensorrt', name: 'NVIDIA GPU (TensorRT / CUDA)' };
  } else if (isAmd) {
    // On AMD: never select OpenVINO! Prefer Pure-Rust CPU Tract (or Ryzen AI if configured)
    fallbackAutoResolved = { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };
  } else if (isIntel) {
    fallbackAutoResolved = { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };
  }

  return {
    auto_resolved_backend: fallbackAutoResolved,
    backends: [
      {
        id: 'auto',
        name: 'Automático (Melhor Acelerador)',
        tier: 'Auto',
        hardware_detected: true,
        runtime_installed: true,
        device_info: isAmd
          ? `Processador AMD detectado (${cpuModel}). Seleção automática segura via Tract Pure-Rust.`
          : 'Seleção dinâmica por prioridade de hardware e disponibilidade.',
        runtime_name: 'Agendador Automático ClearCore',
        auto_resolved_id: fallbackAutoResolved.id,
        auto_resolved_name: fallbackAutoResolved.name,
        install_script: '',
        install_command: '',
        install_instruction: '',
      },
      {
        id: 'nvidia_tensorrt',
        name: 'NVIDIA GPU (TensorRT / CUDA)',
        tier: 'DedicatedGpu',
        hardware_detected: hasNvidiaGpu,
        runtime_installed: false,
        device_info: hasNvidiaGpu ? 'GPU Dedicada NVIDIA Detectada' : 'Nenhuma GPU dedicada NVIDIA detectada neste sistema.',
        runtime_name: isWin ? 'TensorRT (nvinfer.dll)' : 'TensorRT (libnvinfer.so)',
        // Detected only: no inference path yet (mirrors DetectedHardware::runs_inference() in Rust),
        // so there is nothing to install that would make the model faster.
        install_script: '',
        install_command: '',
        install_instruction: 'GPU NVIDIA detectada; o caminho de inferência (TensorRT) ainda não está implementado, então não há nada para instalar.',
      },
      {
        id: 'openvino_npu',
        name: 'Intel OpenVINO (NPU - AI Boost)',
        tier: 'Npu',
        hardware_detected: isIntel && /ultra/i.test(cpuModel),
        runtime_installed: false,
        device_info: isIntel
          ? 'Intel(R) AI Boost (NPU Neural dedicada no SoC Core Ultra)'
          : `Incompatível: Processador AMD detectado (${cpuModel}). A NPU Intel AI Boost requer processador Intel Core Ultra.`,
        runtime_name: isWin ? 'OpenVINO NPU (openvino_intel_npu_plugin.dll)' : 'OpenVINO NPU (libopenvino_intel_npu_plugin.so)',
        install_script: isWin ? '.\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_instruction: isIntel
          ? 'Instale o Intel OpenVINO runtime e o driver Intel NPU para habilitar o processamento na NPU.'
          : 'O OpenVINO não funciona em processadores AMD.',
      },
      {
        id: 'openvino_gpu',
        name: 'Intel OpenVINO (iGPU - Intel Graphics)',
        tier: 'IntegratedGpu',
        hardware_detected: isIntel,
        runtime_installed: false,
        device_info: isIntel
          ? 'GPU Integrada Intel Arc / Graphics'
          : `Incompatível: Processador AMD detectado (${cpuModel}). Requer GPU integrada Intel.`,
        runtime_name: isWin ? 'OpenVINO GPU (openvino_intel_gpu_plugin.dll)' : 'OpenVINO GPU (libopenvino_intel_gpu_plugin.so)',
        install_script: isWin ? '.\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_instruction: isIntel
          ? 'Instale o Intel OpenVINO runtime e o driver compute-runtime para acelerar na GPU integrada.'
          : 'O OpenVINO não funciona em processadores AMD.',
      },
      {
        id: 'openvino_cpu',
        name: 'Intel OpenVINO (CPU - Otimizado)',
        tier: 'Cpu',
        hardware_detected: isIntel,
        runtime_installed: false,
        device_info: isIntel
          ? `${cpuModel} (Aceleração vetorial Intel AVX2 / AMX / VNNI)`
          : `Incompatível: Processador AMD detectado (${cpuModel}). OpenVINO é exclusivo para Intel.`,
        runtime_name: isWin ? 'OpenVINO CPU (openvino_intel_cpu_plugin.dll)' : 'OpenVINO CPU (libopenvino_intel_cpu_plugin.so)',
        install_script: isWin ? '.\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_instruction: isIntel
          ? 'Instale o Intel OpenVINO runtime para habilitar aceleração vetorial Intel na CPU.'
          : 'OpenVINO não é compatível com processadores AMD. Utilize o CPU Nativo (Tract Pure-Rust).',
      },
      {
        id: 'amd_ryzenai_npu',
        name: 'AMD Ryzen AI (NPU - XDNA)',
        tier: 'Npu',
        hardware_detected: isAmd && (/ai\s*\d|7\d{3}|8\d{3}|xdna/i.test(cpuModel) || fs.existsSync('/dev/amdxdna')),
        runtime_installed: false,
        device_info: isAmd
          ? `${cpuModel} (NPU AMD Ryzen AI XDNA)`
          : 'Requer processador AMD Ryzen AI com NPU XDNA integrada.',
        runtime_name: isWin ? 'Ryzen AI Software (xrt_core.dll)' : 'Ryzen AI Software (libxrt_core.so)',
        // Detected only: no inference path yet (mirrors DetectedHardware::runs_inference() in Rust).
        install_script: '',
        install_command: '',
        install_instruction: 'NPU AMD detectada; o caminho de inferência (Ryzen AI) ainda não está implementado, então não há nada para instalar.',
      },
      {
        id: 'amd_ryzenai_gpu',
        name: 'AMD Radeon (iGPU - RDNA Graphics)',
        tier: 'IntegratedGpu',
        hardware_detected: isAmd,
        runtime_installed: false,
        device_info: isAmd
          ? `${cpuModel} (Gráficos Integrados AMD Radeon)`
          : 'Requer processador AMD Ryzen com GPU integrada Radeon.',
        runtime_name: isWin ? 'DirectML / Vulkan (DirectML.dll)' : 'AMD ROCm / Vulkan',
        // Detected only: no DirectML / Vulkan inference path yet.
        install_script: '',
        install_command: '',
        install_instruction: 'GPU AMD detectada; o caminho de inferência (DirectML / Vulkan) ainda não está implementado, então não há nada para instalar.',
      },
      {
        id: 'apple_coreml',
        name: 'Apple Silicon (CoreML)',
        tier: 'Npu',
        hardware_detected: isMac && process.arch === 'arm64',
        runtime_installed: isMac && process.arch === 'arm64',
        device_info: isMac ? 'Apple Silicon M-Series Neural Engine' : 'Exclusivo para computadores Apple Mac com chip Apple Silicon',
        runtime_name: 'Apple CoreML Framework',
        install_script: '',
        install_command: '',
        install_instruction: isMac ? 'CoreML é nativo do macOS.' : 'Requer computador Apple Silicon rodando macOS.',
      },
      {
        id: 'cpu_tract',
        name: 'CPU Nativo (Tract Pure-Rust)',
        tier: 'Cpu',
        hardware_detected: true,
        runtime_installed: true,
        device_info: `${cpuModel} (Execução Nativa Segura em Rust puro - AVX2)`,
        runtime_name: 'Tract (Embarcado, Zero Dependências)',
        install_script: '',
        install_command: '',
        install_instruction: 'Mecanismo padrão 100% seguro em Rust (#![forbid(unsafe_code)]), compatível com AMD e Intel.',
      },
    ],
    active_backend: currentSelectedBackend,
    ...(detectionError ? { detection_error: detectionError } : {}),
  };
}

ipcMain.handle('get_hardware_backends', () => {
  return queryHardwareBackends();
});

ipcMain.handle('set_hardware_backend', (_event, backendId) => {
  // So 'auto' e 'cpu_tract' processam audio hoje (o motor e sempre o Tract na CPU);
  // os demais voltam { success: false, reason: 'not_implemented' } sem mudar nada.
  const result = resolveBackendSelection(backendId);
  if (result.success) {
    currentSelectedBackend = result.active_backend;
  }
  return result;
});

ipcMain.handle('get_studio_preset', () => {
  if (process.platform === 'linux') {
    const current = clearcoreState.readPreset(clearcoreStatePath());
    return current || 'Natural';
  }
  return 'Natural';
});

ipcMain.handle('set_studio_preset', (_event, args) => {
  const preset = typeof args === 'string' ? args : (args && args.preset ? args.preset : 'Natural');
  if (clearcoreState.isPreset(preset)) {
    writeClearcoreSharedState({ preset });
    return { success: true, preset };
  }
  return { success: false, preset: 'Off' };
});

// Auto-Updater IPC Handlers
ipcMain.handle('updater:check', async () => {
  return await updater.checkForUpdates(APP_VERSION);
});

ipcMain.handle('updater:download-and-install', async (_event, assetInfo) => {
  try {
    if (!assetInfo || !assetInfo.downloadUrl) {
      throw new Error('Informações do instalador não fornecidas');
    }
    const tempDir = app.getPath('temp');
    const targetFile = path.join(tempDir, assetInfo.name || 'clearcore-update');

    await updater.downloadFile(assetInfo.downloadUrl, targetFile, (progress) => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send('updater:progress', progress);
      }
    });

    const installResult = await updater.launchInstaller(targetFile, assetInfo.installerType);
    return { success: true, ...installResult };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

// Silent background check 5s after startup
app.whenReady().then(() => {
  setTimeout(async () => {
    try {
      const updateInfo = await updater.checkForUpdates(APP_VERSION);
      if (updateInfo.updateAvailable && mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send('updater:available', updateInfo);
      }
    } catch (e) {
      console.log('[AutoUpdater] Silent check skipped:', e.message);
    }
  }, 5000);
});



