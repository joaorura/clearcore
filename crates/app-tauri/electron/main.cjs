const { app, BrowserWindow, Tray, Menu, nativeImage, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');
const net = require('net');
const os = require('os');
const { execFile, spawn } = require('child_process');
const clearcoreState = require('./clearcore-state.cjs');
const { planCaptureLinks } = require('./capture-link-plan.cjs');
const { parseHardwareJson } = require('./hardware-json.cjs');
const { resolveBackendSelection, mapBackendToDaemon } = require('./backend-selection.cjs');
const updater = require('./updater.cjs');
const voiceProfileStore = require('./voice-profile-store.cjs');
const voiceProfileMerge = require('./voice-profile-merge.cjs');
const enrollmentIpc = require('./enrollment-ipc.cjs');
const appLogger = require('./app-logger.cjs');

// ClearCore Runtime Application Version
const APP_VERSION = '0.1.0-beta.1';
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
function sendIpcRequest(command, payload = {}, timeoutMs = 3000) {
  return new Promise((resolve, reject) => {
    const endpoint = getIpcEndpoint();
    const cmdName = typeof command === 'string' ? command : Object.keys(command)[0] || 'UnknownCommand';
    appLogger.debug('IPC_CLIENT', `Sending command to daemon: ${cmdName}`, { timeoutMs });

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
          appLogger.debug('IPC_CLIENT', `Daemon response Ok for: ${cmdName}`);
          resolve(parsed.payload);
        } else {
          const rejection = new Error(parsed.error ? parsed.error.message : 'IPC request rejected');
          if (parsed.error && typeof parsed.error.code === 'string') rejection.code = parsed.error.code;
          appLogger.warn('IPC_CLIENT', `Daemon returned error for ${cmdName}:`, parsed.error);
          reject(rejection);
        }
      } catch (err) {
        appLogger.error('IPC_CLIENT', `Failed to parse daemon response for ${cmdName}: ${err.message}`);
        reject(new Error(`Failed to parse daemon response: ${err.message}`));
      }
    });

    const targetDesc = endpoint.path ? endpoint.path : `${endpoint.host}:${endpoint.port}`;
    client.on('error', (err) => {
      appLogger.warn('IPC_CLIENT', `Daemon unreachable at ${targetDesc}: ${err.message}`);
      reject(new Error(`Daemon unreachable at ${targetDesc}: ${err.message}`));
    });

    // Timeout (default 3 seconds)
    client.setTimeout(timeoutMs, () => {
      client.destroy();
      appLogger.warn('IPC_CLIENT', `Daemon IPC timeout after ${timeoutMs}ms for ${cmdName}`);
      reject(new Error('Daemon IPC timeout'));
    });
  });
}

const { pickDaemonBinary, shouldReplaceExistingDaemon, isDevModeArgv, noCoreDumpSpawn } = require('./daemon-binary.cjs');

// Sidecar Daemon Discovery & Supervision
function findDaemonBinaryPath() {
  const binName = process.platform === 'win32' ? 'realtime-noise-service.exe' : 'realtime-noise-service';
  // Dev = somente quando iniciado com --dev (dev-runner); start-all.sh roda sem --dev.
  const isDev = isDevModeArgv(process.argv);
  const { path: found, reason } = pickDaemonBinary({
    binName,
    env: process.env,
    resourcesPath: process.resourcesPath,
    appDir: __dirname,
    cwd: process.cwd(),
    isDev,
  });
  console.log(`[Clearcore Daemon] Binário escolhido: ${found || '(nenhum)'} (motivo: ${reason}, dev=${isDev})`);
  return found;
}

async function isDaemonResponsive() {
  try {
    const status = await sendIpcRequest('GetStatus');
    return Boolean(status && status.mode);
  } catch {
    return false;
  }
}

// Auto-detect native embedded model for voice enrollment
function detectDevModels() {
  if (process.env.CLEARCORE_DEV_ENROLLMENT_ASSET || process.env.CLEARCORE_ENROLLMENT_MODEL) {
    return;
  }
  const root = path.resolve(__dirname, '..', '..', '..');
  const nativeModel = path.join(root, 'models', 'enrollment', 'enrollment.onnx');
  if (fs.existsSync(nativeModel)) {
    process.env.CLEARCORE_ENROLLMENT_MODEL = nativeModel;
    console.log(`[Clearcore Daemon] Using embedded enrollment model: ${nativeModel}`);
    return;
  }
}

async function ensureDaemonRunning() {
  // If already running (e.g. system service or previous run), adopt it
  const alreadyResponsive = await isDaemonResponsive();
  if (
    shouldReplaceExistingDaemon({
      env: process.env,
      spawnedByApp: daemonSpawnedByApp,
      responsive: alreadyResponsive,
    })
  ) {
    // Somente dev.sh (CLEARCORE_DEV_OWN_DAEMON=1): o dev é dono do daemon.
    console.warn('[Clearcore Daemon] Daemon anterior em execução; solicitando Shutdown para usar o binário de desenvolvimento.');
    try {
      await sendIpcRequest('Shutdown');
    } catch {}
    let down = false;
    for (let i = 0; i < 15; i++) {
      await new Promise((r) => setTimeout(r, 200));
      if (!(await isDaemonResponsive())) {
        down = true;
        break;
      }
    }
    if (!down) {
      console.error('[Clearcore Daemon] daemon anterior não encerrou; reaproveitando.');
      return true;
    }
    // Pausa para o daemon antigo terminar o remove_file do socket ao sair.
    await new Promise((r) => setTimeout(r, 400));
    console.log('[Clearcore Daemon] Daemon anterior encerrado.');
  } else if (alreadyResponsive) {
    console.log('[Clearcore Daemon] Serviço já está em execução e comunicando via IPC.');
    return true;
  }

  detectDevModels();
  const daemonBin = findDaemonBinaryPath();
  if (!daemonBin) {
    console.warn('[Clearcore Daemon] Binário realtime-noise-service não encontrado.');
    return false;
  }

  console.log(`[Clearcore Daemon] Iniciando sidecar daemon: ${daemonBin} --run`);
  appLogger.info('DAEMON_SUPERVISOR', `Starting sidecar daemon: ${daemonBin} --run`);
  try {
    const userData = app.getPath('userData');
    if (!fs.existsSync(userData)) {
      fs.mkdirSync(userData, { recursive: true });
    }
    const logFile = path.join(userData, 'service.log');
    const logFd = fs.openSync(logFile, 'a');

    // Core dump desligado: o daemon segura PCM cru em memória durante o cadastro de voz.
    const launch = noCoreDumpSpawn(daemonBin, ['--run']);
    daemonChildProcess = spawn(launch.command, launch.args, {
      detached: false,
      stdio: ['ignore', logFd, logFd],
      windowsHide: true,
    });

    daemonSpawnedByApp = true;
    appLogger.info('DAEMON_SUPERVISOR', `Daemon spawned with PID: ${daemonChildProcess.pid}`);

    daemonChildProcess.on('error', (err) => {
      appLogger.error('DAEMON_SUPERVISOR', `Daemon process error: ${err.message}`);
      console.error('[Clearcore Daemon] Erro no processo do serviço:', err.message);
      daemonChildProcess = null;
      daemonSpawnedByApp = false;
    });

    daemonChildProcess.on('exit', (code, signal) => {
      appLogger.warn('DAEMON_SUPERVISOR', `Daemon process exited with code=${code}, signal=${signal}`);
      console.warn(`[Clearcore Daemon] Processo do serviço finalizou (code=${code}, signal=${signal})`);
      daemonChildProcess = null;
      daemonSpawnedByApp = false;
    });

    // Wait up to 3.5 seconds for daemon socket to accept IPC requests
    for (let attempt = 1; attempt <= 18; attempt++) {
      await new Promise((r) => setTimeout(r, 200));
      if (await isDaemonResponsive()) {
        appLogger.info('DAEMON_SUPERVISOR', `IPC connection established after ${attempt} attempts`);
        console.log(`[Clearcore Daemon] Conexão IPC estabelecida com sucesso na tentativa ${attempt}.`);
        return true;
      }
    }

    appLogger.warn('DAEMON_SUPERVISOR', 'Daemon spawned but IPC did not respond within timeout');
    console.warn('[Clearcore Daemon] Daemon iniciado, mas IPC ainda não respondeu.');
    return false;
  } catch (err) {
    appLogger.error('DAEMON_SUPERVISOR', `Failed to launch daemon: ${err.message}`);
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
let cachedInputDevices = [];
let selectedInputDeviceId = null;
let selectedInputNodeName = null;

// Enumerate physical/system audio input devices (excluding Clearcore virtual mics)
function enumerateSystemInputDevices() {
  const isWin = process.platform === 'win32';
  const isMac = process.platform === 'darwin';
  let devices = [];

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
    let pwDumpWorked = false;
    try {
      const dumpStr = require('child_process').execSync('pw-dump Node', { encoding: 'utf8', timeout: 3000 });
      const dump = JSON.parse(dumpStr);
      for (const n of dump) {
        const nid = String(n.id);
        const props = (n.info && n.info.props) || {};
        const mc = props['media.class'] || '';
        const nodeName = props['node.name'] || '';
        const desc = props['node.description'] || props['node.nick'] || nodeName;
        const isSource = mc === 'Audio/Source' || nodeName.startsWith('bluez_input.');
        if (isSource && !nodeName.toLowerCase().includes('realtime') && !nodeName.toLowerCase().includes('clearcore')) {
          devices.push({
            id: nid,
            name: desc,
            nodeName: nodeName,
            is_default: false,
          });
        }
      }
      if (devices.length > 0) {
        pwDumpWorked = true;
      }
    } catch {}

    if (!pwDumpWorked) {
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
    } else {
      try {
        const out = require('child_process').execSync('wpctl status', { encoding: 'utf8', timeout: 3000 });
        for (const dev of devices) {
          if (out.includes(`*   ${dev.id}.`) || out.includes(`*  ${dev.id}.`) || out.includes(`* ${dev.id}.`)) {
            dev.is_default = true;
          }
        }
      } catch {}
    }

    // Detect Bluetooth cards/devices even when dormant in A2DP mode (so user can always select them)
    try {
      const devDumpStr = require('child_process').execSync('pw-dump Device', { encoding: 'utf8', timeout: 3000 });
      const devDump = JSON.parse(devDumpStr);
      for (const d of devDump) {
        const dProps = (d.info && d.info.props) || {};
        const isBt = dProps['device.bus'] === 'bluetooth' || (dProps['device.name'] && dProps['device.name'].startsWith('bluez_card.'));
        if (isBt) {
          const cardId = d.id;
          const addr = dProps['api.bluez5.address'] || dProps['device.string'] || '';
          const alias = dProps['device.alias'] || dProps['device.description'] || dProps['device.name'] || 'Bluetooth Headset';
          const nodeNameColons = addr ? `bluez_input.${addr}` : '';
          const nodeNameUnderscores = addr ? `bluez_input.${addr.replace(/:/g, '_')}` : '';

          // Check if already in devices list
          const alreadyIn = devices.some((x) =>
            (x.nodeName && (x.nodeName === nodeNameColons || x.nodeName === nodeNameUnderscores || (addr && x.nodeName.includes(addr)))) ||
            (x.name && x.name.toLowerCase() === alias.toLowerCase())
          );

          if (!alreadyIn) {
            devices.push({
              id: String(cardId),
              name: alias,
              nodeName: nodeNameColons || nodeNameUnderscores || `bluez_card.${cardId}`,
              is_default: false,
              is_bluetooth_card: true,
              cardId: cardId,
              idle: true,
            });
          } else {
            const devObj = devices.find((x) =>
              (x.nodeName && (x.nodeName === nodeNameColons || x.nodeName === nodeNameUnderscores || (addr && x.nodeName.includes(addr)))) ||
              (x.name && x.name.toLowerCase() === alias.toLowerCase())
            );
            if (devObj) {
              devObj.is_bluetooth_card = true;
              devObj.cardId = cardId;
            }
          }
        }
      }
    } catch {}

    // Retain recently seen devices (especially Bluetooth headsets that temporarily suspend or sleep)
    for (const cached of cachedInputDevices) {
      const match = devices.find((d) => (d.nodeName && d.nodeName === cached.nodeName) || d.name === cached.name);
      if (!match) {
        devices.push({ ...cached, idle: true });
      }
    }
  }

  // Update device cache
  for (const dev of devices) {
    const existingIdx = cachedInputDevices.findIndex((c) => (dev.nodeName && c.nodeName === dev.nodeName) || c.name === dev.name);
    if (existingIdx >= 0) {
      cachedInputDevices[existingIdx] = { ...cachedInputDevices[existingIdx], ...dev };
    } else {
      cachedInputDevices.push(dev);
    }
  }

  // Keep selected device synced with updated PipeWire IDs if nodeName matches
  if (selectedInputNodeName) {
    const activeMatch = devices.find((d) => d.nodeName === selectedInputNodeName);
    if (activeMatch && activeMatch.id !== selectedInputDeviceId) {
      selectedInputDeviceId = activeMatch.id;
      writeClearcoreSharedState({ targetNodeId: activeMatch.id });
    }
  }

  currentInputDevices = devices;
  return devices;
}

// Bluetooth Headset Automatic Dynamic Profile Management (A2DP for high-fidelity playback, HFP for voice capture)
let lastActiveCaptureTime = 0;
let currentBtProfileMode = 'a2dp'; // 'a2dp' | 'hfp'
let btSwitchingInProgress = false;

async function manageBluetoothProfile(isCapturing) {
  if (process.platform !== 'linux') return;
  if (!selectedInputNodeName && !selectedInputDeviceId) return;

  const isBtSelected = (selectedInputNodeName && (selectedInputNodeName.startsWith('bluez_') || selectedInputNodeName.includes(':'))) ||
    currentInputDevices.some((d) => (d.id === selectedInputDeviceId || d.nodeName === selectedInputNodeName) && d.is_bluetooth_card);

  if (!isBtSelected) return;

  const now = Date.now();
  if (isCapturing) {
    lastActiveCaptureTime = now;
  }

  // Query current live active profile directly from device
  let activeProfileName = '';
  let btDev = null;
  try {
    const devDumpStr = require('child_process').execSync('pw-dump Device', { encoding: 'utf8', timeout: 3000 });
    const devDump = JSON.parse(devDumpStr);
    btDev = devDump.find((d) => {
      const p = (d.info && d.info.props) || {};
      return p['device.bus'] === 'bluetooth' || (p['device.name'] && p['device.name'].startsWith('bluez_card.'));
    });
    if (btDev && btDev.info && btDev.info.params && btDev.info.params.Profile && btDev.info.params.Profile[0]) {
      activeProfileName = btDev.info.params.Profile[0].name || '';
    }
  } catch {}

  const isLiveHeadset = activeProfileName.includes('headset') || activeProfileName.includes('hfp') || activeProfileName.includes('hsp');

  // If capturing and currently in A2DP, activate Headset mode (HFP/HSP)
  if (isCapturing && !isLiveHeadset && !btSwitchingInProgress && btDev) {
    btSwitchingInProgress = true;
    try {
      const cardId = btDev.id;
      const enumProfiles = (btDev.info && btDev.info.params && btDev.info.params.EnumProfile) || [];
      // Respect OS / WirePlumber preference: check saved profile first, or pick highest priority headset profile
      let preferredName = null;
      try {
        const wpState = require('fs').readFileSync(
          require('path').join(require('os').homedir(), '.local', 'state', 'wireplumber', 'default-profile'),
          'utf8'
        );
        const m = wpState.match(new RegExp(`${btDev.info.props['device.name']}=([^\\s]+)`));
        if (m) preferredName = m[1].trim();
      } catch {}

      const hfpProfiles = enumProfiles.filter((p) => p.name && (p.name.includes('headset') || p.name.includes('hfp') || p.name.includes('hsp')));
      hfpProfiles.sort((a, b) => (b.priority || 0) - (a.priority || 0));

      const hfpProf = (preferredName && hfpProfiles.find((p) => p.name === preferredName)) ||
        hfpProfiles[0] ||
        enumProfiles.find((p) => p.name && p.name.startsWith('headset'));

      if (hfpProf) {
        appLogger.info('BLUETOOTH', `Activating Headset profile (${hfpProf.name}, index ${hfpProf.index}, prio ${hfpProf.priority}) according to OS configuration on card ${cardId}`);
        require('child_process').execSync(`wpctl set-profile ${cardId} ${hfpProf.index}`, { timeout: 3000 });
        currentBtProfileMode = 'hfp';

        setTimeout(() => {
          try {
            if (selectedInputDeviceId) {
              setSystemInputDevice(selectedInputDeviceId);
            }
          } catch {}
        }, 600);
      }
    } catch (err) {
      appLogger.warn('BLUETOOTH', `Failed to activate Headset profile: ${err.message}`);
    } finally {
      btSwitchingInProgress = false;
    }
  } else if (!isCapturing && isLiveHeadset && !btSwitchingInProgress && btDev) {
    // Release back to A2DP if idle for at least 3.5 seconds
    const idleElapsed = now - lastActiveCaptureTime;
    if (idleElapsed >= 3500) {
      btSwitchingInProgress = true;
      try {
        const cardId = btDev.id;
        const enumProfiles = (btDev.info && btDev.info.params && btDev.info.params.EnumProfile) || [];
        // Find best A2DP profile: AAC -> SBC-XQ -> SBC -> generic a2dp-sink
        const a2dpProf = enumProfiles.find((p) => p.name === 'a2dp-sink') ||
          enumProfiles.find((p) => p.name === 'a2dp-sink-sbc_xq') ||
          enumProfiles.find((p) => p.name === 'a2dp-sink-sbc') ||
          enumProfiles.find((p) => p.name && p.name.startsWith('a2dp-sink'));

        if (a2dpProf) {
          appLogger.info('BLUETOOTH', `Releasing mic: restoring high-fidelity playback (A2DP ${a2dpProf.name}, index ${a2dpProf.index}) on card ${cardId}`);
          require('child_process').execSync(`wpctl set-profile ${cardId} ${a2dpProf.index}`, { timeout: 3000 });
          currentBtProfileMode = 'a2dp';
        }
      } catch (err) {
        appLogger.warn('BLUETOOTH', `Failed to restore A2DP profile: ${err.message}`);
      } finally {
        btSwitchingInProgress = false;
      }
    }
  }
}

function setSystemInputDevice(deviceId) {
  selectedInputDeviceId = deviceId;

  // Find nodeName if known
  let targetNodeName = '';
  const dev = currentInputDevices.find((d) => d.id === deviceId || d.nodeName === deviceId);
  if (dev && dev.nodeName) {
    targetNodeName = dev.nodeName;
    selectedInputNodeName = dev.nodeName;
  }

  writeClearcoreSharedState({ targetNodeId: deviceId });

  if (process.platform === 'linux') {
    try {
      let nodeName = targetNodeName;
      if (!nodeName && /^\d+$/.test(deviceId)) {
        try {
          const nodeInfo = require('child_process').execSync(`pw-cli info ${deviceId}`, { encoding: 'utf8', timeout: 2000 });
          const nameMatch = nodeInfo.match(/node\.name = "([^"]+)"/);
          if (nameMatch) {
            nodeName = nameMatch[1];
            selectedInputNodeName = nodeName;
          }
        } catch {}
      }

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
      if (mainWindow && !mainWindow.isDestroyed()) {
        const local = voiceProfileStore.readVoiceProfile();
        const merged = voiceProfileMerge.mergeLocalAndServiceProfile(local, res);
        mainWindow.webContents.send('voice-profile-update', { success: true, profile: merged, ...merged });
      }
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

  // Check if any application or recorder is actively consuming the virtual microphone
  if (process.platform === 'linux' && isServiceRunning && currentVirtualMicStatus.present) {
    try {
      let isCapturingActive = false;
      const nodeDumpStr = require('child_process').execSync('pw-dump Node', { encoding: 'utf8', timeout: 2000 });
      const nodeDump = JSON.parse(nodeDumpStr);
      const vSource = nodeDump.find((n) => (n.info && n.info.props && n.info.props['node.name'] === 'realtime-noise-source'));
      if (vSource) {
        const state = (vSource.info && vSource.info.state) || '';
        // If node is streaming, active or running, clients are recording audio!
        if (state === 'running' || state === 'active' || state === 'streaming') {
          isCapturingActive = true;
        }
      }

      await manageBluetoothProfile(isCapturingActive);
    } catch {}
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

  // 1. Check user config: Start activated (default: true) or stopped, and selected runtime
  const settings = readAppSettings();
  currentSelectedBackend = settings.selectedBackend || 'auto';
  const shouldStartActivated = settings.startActivated !== false;

  if (shouldStartActivated) {
    console.log('[Startup] ClearCore configurado para iniciar ATIVADO (Padrão).');
    await ensureDaemonRunning();
    await verifyAndAutoCreateVirtualMicOnStartup();
    isServiceRunning = true;
    if (currentSelectedBackend && currentSelectedBackend !== 'auto') {
      try {
        const daemonBackend = mapBackendToDaemon(currentSelectedBackend);
        await sendIpcRequest({ SetBackend: daemonBackend });
      } catch (e) {
        console.log('[Startup] Daemon SetBackend forward skipped/failed:', e.message);
      }
    }
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
  const settings = readAppSettings();
  try {
    const res = await sendIpcRequest('GetStatus');
    if (res && typeof res === 'object') {
      if (res.filter_intensity === undefined) {
        res.filter_intensity = typeof settings.filter_intensity === 'number' ? settings.filter_intensity : 50;
      }
    }
    return res;
  } catch (err) {
    return {
      state: 'Stopped',
      mode: currentMode,
      filter_intensity: typeof settings.filter_intensity === 'number' ? settings.filter_intensity : 50,
      is_terminal: false,
      can_restart: true,
      crash_count_15m: 0,
      total_crashes: 0,
      error: err.message,
    };
  }
});

ipcMain.handle('set_mode', async (_event, args) => {
  const mode = args && args.mode ? args.mode : 'Active';
  appLogger.info('ELECTRON', `set_mode requested: ${mode} (previous: ${currentMode})`);
  const res = await sendIpcRequest({ SetMode: mode });
  currentMode = mode;
  writeClearcoreSharedState({ mode });
  updateTrayMenu();
  appLogger.info('ELECTRON', `set_mode successfully applied: ${mode}`);
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
  const devices = enumerateSystemInputDevices();
  appLogger.debug('AUDIO_DEVICE', `get_input_devices enumerated ${devices.length} devices`, devices);
  return devices;
});

ipcMain.handle('set_input_device', (_event, args) => {
  const deviceId = typeof args === 'string' ? args : (args && args.deviceId ? args.deviceId : '');
  appLogger.info('AUDIO_DEVICE', `set_input_device requested: '${deviceId}'`);
  const res = setSystemInputDevice(deviceId);
  appLogger.info('AUDIO_DEVICE', `set_input_device result:`, res);
  return res;
});

let currentSelectedBackend = readAppSettings().selectedBackend || 'auto';

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

  // Probe for OpenVINO runtime on host in fallback
  let hasOpenVinoRuntime = false;
  try {
    if (isLinux) {
      const ldOut = require('child_process').execSync('ldconfig -p 2>/dev/null | grep -i libopenvino || true', { encoding: 'utf8' });
      if (ldOut.includes('libopenvino')) hasOpenVinoRuntime = true;
      if (!hasOpenVinoRuntime && (fs.existsSync('/usr/lib64/openvino') || fs.existsSync('/opt/intel/openvino'))) {
        hasOpenVinoRuntime = true;
      }
    } else if (isWin) {
      if (fs.existsSync('C:\\Program Files (x86)\\Intel\\openvino') || fs.existsSync('C:\\Intel\\openvino')) {
        hasOpenVinoRuntime = true;
      }
    }
  } catch {}

  // Probe for TensorRT runtime on host in fallback
  let hasTensorRtRuntime = false;
  try {
    if (isLinux) {
      const ldOut = require('child_process').execSync('ldconfig -p 2>/dev/null | grep -i libnvinfer || true', { encoding: 'utf8' });
      if (ldOut.includes('libnvinfer')) hasTensorRtRuntime = true;

      const directCandidates = [
        '/opt/tensorrt/lib/libnvinfer.so',
        '/opt/tensorrt/lib/libnvinfer.so.11',
        '/usr/lib64/libnvinfer.so',
        '/usr/lib64/libnvinfer.so.11',
        '/usr/lib/x86_64-linux-gnu/libnvinfer.so',
        '/usr/local/cuda/lib64/libnvinfer.so',
        '/usr/local/tensorrt/lib/libnvinfer.so',
      ];
      for (const cand of directCandidates) {
        if (!hasTensorRtRuntime && fs.existsSync(cand)) {
          hasTensorRtRuntime = true;
          break;
        }
      }

      const homeDir = os.homedir();
      const searchBases = [
        path.join(homeDir, 'opt'),
        '/opt',
        '/usr/local',
      ];
      for (const base of searchBases) {
        if (hasTensorRtRuntime) break;
        if (fs.existsSync(base)) {
          try {
            const entries = fs.readdirSync(base);
            for (const entry of entries) {
              if (/^tensorrt/i.test(entry)) {
                const libDir = path.join(base, entry, 'lib');
                if (fs.existsSync(libDir)) {
                  if (
                    fs.existsSync(path.join(libDir, 'libnvinfer.so')) ||
                    fs.existsSync(path.join(libDir, 'libnvinfer.so.11')) ||
                    fs.existsSync(path.join(libDir, 'libnvinfer.so.10'))
                  ) {
                    hasTensorRtRuntime = true;
                    break;
                  }
                }
              }
            }
          } catch {}
        }
      }
    } else if (isWin) {
      if (fs.existsSync('C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v13.0\\bin\\nvinfer_11.dll') ||
          fs.existsSync('C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v12.0\\bin\\nvinfer_10.dll') ||
          fs.existsSync('C:\\Program Files\\NVIDIA GPU Computing Toolkit\\CUDA\\v13.0\\bin\\nvinfer.dll')) {
        hasTensorRtRuntime = true;
      }
    }
  } catch {}

  // Probe whether TensorRT model engines are compiled locally for the current GPU
  let hasTensorRtEnginesCompiled = false;
  try {
    const candidateEngineDirs = [
      path.join(process.cwd(), 'models', 'stateful', 'tensorrt'),
      path.join(process.cwd(), 'models', 'tensorrt'),
      path.join(__dirname, '..', 'models', 'stateful', 'tensorrt'),
      path.join(__dirname, '..', '..', '..', 'models', 'stateful', 'tensorrt'),
      path.join(os.homedir(), '.local', 'share', 'clearcore', 'models', 'tensorrt'),
    ];
    for (const edir of candidateEngineDirs) {
      if (
        fs.existsSync(path.join(edir, 'enc.engine')) &&
        fs.existsSync(path.join(edir, 'df_dec.engine')) &&
        fs.existsSync(path.join(edir, 'erb_dec.engine'))
      ) {
        hasTensorRtEnginesCompiled = true;
        break;
      }
    }
  } catch {}

  // Resolve Auto backend (Priority: NPU -> iGPU -> dGPU -> CPU fallback)
  let fallbackAutoResolved = { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };
  if (isMac && process.arch === 'arm64') {
    fallbackAutoResolved = { id: 'apple_coreml', name: 'Apple Silicon (CoreML)' };
  } else if (isIntel && hasOpenVinoRuntime && /ultra/i.test(cpuModel)) {
    // 1. NPU (Intel Core Ultra AI Boost - minimal power consumption, dedicated silicon)
    fallbackAutoResolved = { id: 'openvino_npu', name: 'Intel OpenVINO (NPU - AI Boost)' };
  } else if (isIntel && hasOpenVinoRuntime) {
    // 2. iGPU (Intel Arc / Iris integrated graphics - low power, offloads CPU)
    fallbackAutoResolved = { id: 'openvino_gpu', name: 'Intel OpenVINO (iGPU - Intel Graphics)' };
  } else if (hasNvidiaGpu && hasTensorRtRuntime) {
    // 3. dGPU (Dedicated NVIDIA GPU via TensorRT)
    fallbackAutoResolved = { id: 'nvidia_tensorrt', name: 'NVIDIA GPU (TensorRT / CUDA)' };
  } else if (isAmd) {
    // On AMD: never select OpenVINO! Prefer Pure-Rust CPU Tract (or Ryzen AI if configured)
    fallbackAutoResolved = { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };
  } else if (isIntel && hasOpenVinoRuntime) {
    // 4. CPU (Intel CPU optimized via OpenVINO AVX/VNNI)
    fallbackAutoResolved = { id: 'openvino_cpu', name: 'Intel OpenVINO (CPU - Otimizado)' };
  } else {
    // 4. CPU Fallback (Tract pure Rust)
    fallbackAutoResolved = { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };
  }

  // Detect package commands on Linux
  let linuxPkgTrt = 'sudo apt install -y libnvinfer10 libnvonnxparsers10 || pip install tensorrt';
  let linuxPkgOvNpu = 'sudo apt install -y intel-npu-driver openvino || pip install openvino';
  let linuxPkgOvGpu = 'sudo apt install -y intel-opencl-icd openvino || pip install openvino';
  let linuxPkgOvCpu = 'sudo apt install -y openvino || pip install openvino';
  if (isLinux) {
    try {
      const osRel = fs.readFileSync('/etc/os-release', 'utf8').toLowerCase();
      if (/fedora|rhel|centos|rocky/.test(osRel)) {
        linuxPkgTrt = 'sudo dnf install -y tensorrt || pip install tensorrt';
        linuxPkgOvNpu = 'sudo dnf install -y intel-npu-driver openvino || pip install openvino';
        linuxPkgOvGpu = 'sudo dnf install -y intel-compute-runtime openvino || pip install openvino';
        linuxPkgOvCpu = 'sudo dnf install -y openvino || pip install openvino';
      } else if (/arch|manjaro/.test(osRel)) {
        linuxPkgTrt = 'sudo pacman -S --needed tensorrt || pip install tensorrt';
        linuxPkgOvNpu = 'sudo pacman -S --needed intel-npu-driver-bin openvino || pip install openvino';
        linuxPkgOvGpu = 'sudo pacman -S --needed intel-compute-runtime openvino || pip install openvino';
        linuxPkgOvCpu = 'sudo pacman -S --needed openvino || pip install openvino';
      }
    } catch {}
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
        runtime_installed: hasTensorRtRuntime,
        device_info: hasNvidiaGpu ? 'GPU Dedicada NVIDIA Detectada' : 'Nenhuma GPU dedicada NVIDIA detectada neste sistema.',
        runtime_name: isWin ? 'TensorRT (nvinfer.dll)' : 'TensorRT (libnvinfer.so)',
        install_script: '',
        install_command: isWin ? 'pip install tensorrt' : linuxPkgTrt,
        install_instruction: 'GPU NVIDIA detectada. Baixe o pacote TAR no portal oficial (https://developer.nvidia.com/tensorrt/download), extraia e configure no ldconfig. Documentação de referência: https://docs.nvidia.com/deeplearning/tensorrt/latest/installing-tensorrt/installing.html',
      },
      {
        id: 'openvino_npu',
        name: 'Intel OpenVINO (NPU - AI Boost)',
        tier: 'Npu',
        hardware_detected: isIntel && /ultra/i.test(cpuModel),
        runtime_installed: hasOpenVinoRuntime && isIntel,
        device_info: isIntel
          ? 'Intel(R) AI Boost (NPU Neural dedicada no SoC Core Ultra)'
          : `Incompatível: Processador AMD detectado (${cpuModel}). A NPU Intel AI Boost requer processador Intel Core Ultra.`,
        runtime_name: isWin ? 'OpenVINO NPU (openvino_intel_npu_plugin.dll)' : 'OpenVINO NPU (libopenvino_intel_npu_plugin.so)',
        install_script: '',
        install_command: isWin ? 'pip install openvino' : linuxPkgOvNpu,
        install_instruction: isIntel
          ? 'Instale o Intel OpenVINO runtime e o driver intel-npu-driver para habilitar o processamento neural na NPU. Documentação: https://docs.openvino.ai/'
          : 'O OpenVINO não funciona em processadores AMD.',
      },
      {
        id: 'openvino_gpu',
        name: 'Intel OpenVINO (iGPU - Intel Graphics)',
        tier: 'IntegratedGpu',
        hardware_detected: isIntel,
        runtime_installed: hasOpenVinoRuntime && isIntel,
        device_info: isIntel
          ? 'GPU Integrada Intel Arc / Graphics'
          : `Incompatível: Processador AMD detectado (${cpuModel}). Requer GPU integrada Intel.`,
        runtime_name: isWin ? 'OpenVINO GPU (openvino_intel_gpu_plugin.dll)' : 'OpenVINO GPU (libopenvino_intel_gpu_plugin.so)',
        install_script: '',
        install_command: isWin ? 'pip install openvino' : linuxPkgOvGpu,
        install_instruction: isIntel
          ? 'Instale o Intel OpenVINO runtime e o driver compute-runtime para acelerar na GPU integrada. Documentação: https://docs.openvino.ai/'
          : 'O OpenVINO não funciona em processadores AMD.',
      },
      {
        id: 'openvino_cpu',
        name: 'Intel OpenVINO (CPU - Otimizado)',
        tier: 'Cpu',
        hardware_detected: isIntel,
        runtime_installed: hasOpenVinoRuntime && isIntel,
        device_info: isIntel
          ? `${cpuModel} (Aceleração vetorial Intel AVX2 / AMX / VNNI)`
          : `Incompatível: Processador AMD detectado (${cpuModel}). OpenVINO é exclusivo para Intel.`,
        runtime_name: isWin ? 'OpenVINO CPU (openvino_intel_cpu_plugin.dll)' : 'OpenVINO CPU (libopenvino_intel_cpu_plugin.so)',
        install_script: '',
        install_command: isWin ? 'pip install openvino' : linuxPkgOvCpu,
        install_instruction: isIntel
          ? 'Instale o Intel OpenVINO runtime para habilitar aceleração vetorial Intel na CPU. Documentação: https://docs.openvino.ai/'
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
    model_compilation_needed: hasNvidiaGpu && !hasTensorRtEnginesCompiled,
    model_compiled: hasTensorRtEnginesCompiled,
    ...(detectionError ? { detection_error: detectionError } : {}),
  };
}

ipcMain.handle('get_hardware_backends', () => {
  return queryHardwareBackends();
});

ipcMain.handle('set_hardware_backend', async (_event, backendId) => {
  appLogger.info('ACCELERATOR', `set_hardware_backend requested:`, backendId);
  const result = resolveBackendSelection(backendId);
  if (result.success) {
    currentSelectedBackend = result.active_backend;
    writeAppSettings({ selectedBackend: result.active_backend });
    appLogger.info('ACCELERATOR', `Selected backend resolved to: ${result.active_backend}`);
    if (await isDaemonResponsive()) {
      try {
        const daemonBackend = mapBackendToDaemon(result.active_backend);
        appLogger.info('ACCELERATOR', `Forwarding SetBackend to daemon: '${daemonBackend}'`);
        const daemonResp = await sendIpcRequest({ SetBackend: daemonBackend });
        appLogger.info('ACCELERATOR', `Daemon responded to SetBackend:`, daemonResp);
        if (daemonResp && typeof daemonResp === 'object') {
          return {
            ...result,
            daemon_response: daemonResp,
          };
        }
      } catch (e) {
        appLogger.warn('ACCELERATOR', `Daemon SetBackend forward failed: ${e.message}`);
        console.log('[Clearcore IPC] Daemon SetBackend forward skipped/failed:', e.message);
      }
    }
  } else {
    appLogger.warn('ACCELERATOR', `resolveBackendSelection failed:`, result);
  }
  return result;
});

ipcMain.handle('compile_tensorrt_models', async () => {
  try {
    const homeDir = os.homedir();
    let trtexecPath = 'trtexec';

    // Locate trtexec binary if not in default PATH
    const trtCandidates = [
      path.join(homeDir, 'opt', 'TensorRT-11.3.0.99', 'bin', 'trtexec'),
      '/opt/tensorrt/bin/trtexec',
      '/usr/local/tensorrt/bin/trtexec',
      '/usr/bin/trtexec',
    ];
    for (const cand of trtCandidates) {
      if (fs.existsSync(cand)) {
        trtexecPath = cand;
        break;
      }
    }

    const modelsBaseDir = path.join(process.cwd(), 'models', 'stateful');
    const outDir = path.join(modelsBaseDir, 'tensorrt');
    if (!fs.existsSync(outDir)) {
      fs.mkdirSync(outDir, { recursive: true });
    }

    const { execSync } = require('child_process');
    const models = ['enc', 'df_dec', 'erb_dec'];
    for (const m of models) {
      const onnx = path.join(modelsBaseDir, `${m}.onnx`);
      const engine = path.join(outDir, `${m}.engine`);
      if (fs.existsSync(onnx)) {
        console.log(`[TensorRT Compiler] Compiling ${m}.onnx -> ${m}.engine using ${trtexecPath}...`);
        execSync(`"${trtexecPath}" --onnx="${onnx}" --saveEngine="${engine}"`, { timeout: 120000 });
      }
    }

    return { success: true, compiled: true };
  } catch (err) {
    console.error('[TensorRT Compiler] Compilation error:', err);
    return { success: false, error: err.message };
  }
});

// Studio DSP Preset Handlers
ipcMain.handle('get_studio_preset', () => {
  if (process.platform === 'linux') {
    const current = clearcoreState.readPreset(clearcoreStatePath());
    if (current) return current;
  }
  const settings = readAppSettings();
  if (settings && settings.preset) return settings.preset;
  return 'Natural';
});

ipcMain.handle('set_studio_preset', async (_event, args) => {
  const preset = typeof args === 'string' ? args : (args && args.preset ? args.preset : 'Natural');
  if (clearcoreState.isPreset(preset)) {
    writeAppSettings({ preset });
    if (process.platform === 'linux') {
      writeClearcoreSharedState({ preset });
    }
    if (await isDaemonResponsive()) {
      try {
        await sendIpcRequest({ SetPreset: preset });
      } catch (e) {
        console.log('[Clearcore IPC] Daemon SetPreset forward skipped:', e.message);
      }
    }
    return { success: true, preset };
  }
  return { success: false, preset: 'Off' };
});

// Noise Suppression Intensity Handlers
ipcMain.handle('get_filter_intensity', () => {
  const settings = readAppSettings();
  if (settings && typeof settings.filter_intensity === 'number') {
    return settings.filter_intensity;
  }
  return 50;
});

ipcMain.handle('set_filter_intensity', async (_event, args) => {
  const rawIntensity = typeof args === 'number' ? args : (args && typeof args.intensity === 'number' ? args.intensity : 50);
  const clamped = Math.max(0, Math.min(100, Math.round(rawIntensity)));
  writeAppSettings({ filter_intensity: clamped });
  let daemonResponse = null;
  if (await isDaemonResponsive()) {
    try {
      daemonResponse = await sendIpcRequest({ SetFilterIntensity: { intensity: clamped } });
    } catch (e) {
      console.log('[Clearcore IPC] Daemon SetFilterIntensity forward skipped:', e.message);
    }
  }
  return { success: true, filter_intensity: clamped, daemon: daemonResponse };
});

// Voice Auto-Leveler (AGC) Intensity Handlers
ipcMain.handle('get_voice_leveler', () => {
  const settings = readAppSettings();
  if (settings && typeof settings.voice_leveler === 'number') {
    return settings.voice_leveler;
  }
  return 0;
});

ipcMain.handle('set_voice_leveler', async (_event, args) => {
  const rawIntensity = typeof args === 'number' ? args : (args && typeof args.intensity === 'number' ? args.intensity : 0);
  const clamped = Math.max(0, Math.min(100, Math.round(rawIntensity)));
  writeAppSettings({ voice_leveler: clamped });
  let daemonResponse = null;
  if (await isDaemonResponsive()) {
    try {
      daemonResponse = await sendIpcRequest({ SetVoiceLeveler: { intensity: clamped } });
    } catch (e) {
      console.log('[Clearcore IPC] Daemon SetVoiceLeveler forward skipped:', e.message);
    }
  }
  return { success: true, intensity: clamped, voice_leveler: clamped, daemon: daemonResponse };
});

// Voice Profile & Speaker Isolation IPC Handlers
async function getServiceVoiceProfileStatus() {
  try {
    return (await sendIpcRequest('GetStatus', {}, 1500)) || {};
  } catch (_e) {
    return {};
  }
}

async function readMergedVoiceProfile() {
  const local = voiceProfileStore.readVoiceProfile();
  const service = await getServiceVoiceProfileStatus();
  const merged = voiceProfileMerge.mergeLocalAndServiceProfile(local, service);
  return { success: true, profile: merged, ...merged };
}

ipcMain.handle('set_voice_profile', async (_event, args) => {
  try {
    const rawProfile = (args && typeof args === 'object' && args.profile) ? args.profile : (args && typeof args === 'object' ? args : {});
    // Service-derived fields must never be persisted in the local store.
    const updated = voiceProfileStore.writeVoiceProfile(
      voiceProfileMerge.stripServiceVoiceProfileFields(rawProfile),
    );
    let forwardError = null;

    // Forward to daemon if running
    if (await isDaemonResponsive()) {
      try {
        // The local status is never sent as profile_json: the service owns the profile state
        // (built through the enrollment_* channels). Only an explicit un-enroll is forwarded.
        if (rawProfile.is_enrolled === false) {
          await sendIpcRequest('ClearVoiceProfile');
        }
      } catch (e) {
        forwardError = voiceProfileMerge.classifyForwardError(e);
        console.log('[Clearcore IPC] Daemon voice profile forward failed:', forwardError);
      }
    } else {
      forwardError = 'service_unavailable';
    }

    const result = voiceProfileMerge.buildSetVoiceProfileResult({
      updated,
      serviceStatus: await getServiceVoiceProfileStatus(),
      forwardError,
    });
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send('voice-profile-update', result);
    }

    return { success: true, profile: result, ...result };
  } catch (err) {
    console.error('[Clearcore IPC] Error in set_voice_profile:', err);
    return { success: false, error: err.message };
  }
});

ipcMain.handle('set_voice_isolation', async (_event, args) => {
  const enabled = typeof args === 'boolean' ? args : Boolean(args && typeof args === 'object' ? args.enabled : args);
  let forwardError = null;
  let serviceResp = null;
  if (await isDaemonResponsive()) {
    try {
      serviceResp = await sendIpcRequest({ SetVoiceIsolation: { enabled } });
    } catch (e) {
      forwardError = e.message;
      console.log('[Clearcore IPC] Daemon SetVoiceIsolation failed:', e.message);
    }
  } else {
    forwardError = 'service_unavailable';
  }
  const merged = await readMergedVoiceProfile();
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send('voice-profile-update', merged);
  }
  return { success: !forwardError, voice_isolation_enabled: enabled, profile: merged, serviceResp };
});

ipcMain.handle('get_voice_profile', () => readMergedVoiceProfile());

ipcMain.handle('get_voice_profile_status', () => readMergedVoiceProfile());

// Voice enrollment channels (enrollment_add_sample, enrollment_list_samples, ...).
enrollmentIpc.registerEnrollmentHandlers(ipcMain, { sendIpcRequest });
ipcMain.handle('log_voice_debug', (_event, args) => {
  enrollmentIpc.appendDebugLog(args?.origin || 'FRONTEND', args?.message || '', args?.data);
  appLogger.log(args?.level || 'DEBUG', args?.origin || 'FRONTEND', args?.message || '', args?.data);
  return { ok: true };
});

ipcMain.handle('log_message', (_event, args) => {
  const level = args?.level || 'INFO';
  const target = args?.target || 'RENDERER';
  const message = args?.message || '';
  const data = args?.data;
  appLogger.log(level, target, message, data);
  return { ok: true };
});

ipcMain.handle('get_call_takes', async () => {
  // Cache is only a fallback while the service is unreachable; the service is the source of truth.
  let takes = voiceProfileStore.readCallTakes();
  if (await isDaemonResponsive()) {
    try {
      const daemonResp = await sendIpcRequest('ListIntakeSuggestions');
      if (daemonResp && Array.isArray(daemonResp.suggestions)) {
        takes = daemonResp.suggestions.map((s) => ({
          id: s.id,
          timestamp: s.timestamp,
          durationSec: s.duration_secs,
          snrDb: s.snr,
          audioUrl: s.audio_path || undefined,
          speech_seconds: s.speech_seconds,
          device_label: s.device_label,
        }));
        voiceProfileStore.writeCallTakes(takes);
      }
    } catch (e) {
      console.log('[Clearcore IPC] ListIntakeSuggestions query skipped:', e.message);
    }
  }
  return { success: true, takes, count: takes.length };
});

ipcMain.handle('approve_call_take', async (_event, args) => {
  const id = args && args.id ? args.id : (typeof args === 'string' ? args : '');
  const name = args && args.name ? args.name : null;
  try {
    // No free-form message is returned: the renderer maps errorCode to its own label.
    await sendIpcRequest({ ApproveIntakeSuggestion: { id, name } }, {}, 60000);
  } catch (e) {
    return { errorCode: enrollmentIpc.classifyEnrollError(e) };
  }
  return { success: true, ...voiceProfileStore.approveCallTake(id) };
});

ipcMain.handle('dismiss_call_take', async (_event, args) => {
  try {
    const id = typeof args === 'string' ? args : (args && args.id ? args.id : '');
    const result = voiceProfileStore.dismissCallTake(id);

    if (await isDaemonResponsive()) {
      try {
        await sendIpcRequest({
          DiscardIntakeSuggestion: { id },
        });
      } catch (e) {
        console.log('[Clearcore IPC] DiscardIntakeSuggestion forward skipped:', e.message);
      }
    }

    return { success: true, ...result };
  } catch (err) {
    return { success: false, error: err.message };
  }
});

ipcMain.handle('export_diagnostics', async () => {
  try {
    let diag = null;
    if (await isDaemonResponsive()) {
      try {
        diag = await sendIpcRequest('GetDiagnostics');
      } catch {}
    }
    const report = {
      app_version: APP_VERSION,
      timestamp: new Date().toISOString(),
      platform: process.platform,
      arch: process.arch,
      virtual_mic: currentVirtualMicStatus,
      status: currentStatus,
      diagnostics: diag,
    };
    return JSON.stringify(report, null, 2);
  } catch (err) {
    return JSON.stringify({ error: err.message }, null, 2);
  }
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



