const { app, BrowserWindow, Tray, Menu, nativeImage, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');
const net = require('net');
const os = require('os');
const { execFile, spawn } = require('child_process');

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

// Path to socket
function getSocketPath() {
  if (process.platform === 'win32') {
    return '\\\\.\\pipe\\realtime-noise-control-v1';
  }
  const uid = typeof process.getuid === 'function' ? process.getuid() : os.userInfo().uid;
  return `/run/user/${uid}/realtime-noise.sock`;
}

// Low-level IPC request to realtime-noise-service daemon
function sendIpcRequest(command, payload = {}) {
  return new Promise((resolve, reject) => {
    const socketPath = getSocketPath();
    const req =
      JSON.stringify({
        version: 'realtime-noise.v1',
        request_id: `req-${Date.now()}`,
        command,
        payload,
      }) + '\n';

    const client = net.createConnection(socketPath, () => {
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

    client.on('error', (err) => {
      reject(new Error(`Daemon unreachable at ${socketPath}: ${err.message}`));
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
      else if (action === 'set_default') args.push('-SetDefault');
      else if (action === 'status') args.push('-Status');
    } else if (isMac) {
      command = '/bin/bash';
      scriptPath = findScriptPath('check-virtual-mic-macos.sh');
      args = [scriptPath, '--json'];
      if (action === 'recreate') args.push('--recreate');
      else if (action === 'set_default') args.push('--set-default');
      else if (action === 'status') args.push('--status');
    } else {
      command = '/bin/bash';
      scriptPath = findScriptPath('check-virtual-mic.sh');
      args = [scriptPath, '--json'];
      if (action === 'recreate') args.push('--recreate');
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
          driver_status: 'Error',
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
          driver_status: 'ParseError',
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

// Autostart management
function getAutostartDesktopFilePath() {
  const homeDir = os.homedir();
  return path.join(homeDir, '.config', 'autostart', 'realtime-noise.desktop');
}

function isAutostartEnabled() {
  if (process.platform === 'linux') {
    return fs.existsSync(getAutostartDesktopFilePath());
  }
  const settings = app.getLoginItemSettings();
  return settings.openAtLogin;
}

function setAutostartEnabled(enabled) {
  if (process.platform === 'linux') {
    const desktopPath = getAutostartDesktopFilePath();
    if (enabled) {
      const autostartDir = path.dirname(desktopPath);
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
Comment=ClearCore - Supressão de Ruído em Tempo Real
Exec=${execCmd}
Icon=${iconPath}
Terminal=false
Categories=AudioVideo;Audio;
X-GNOME-Autostart-enabled=true
`;
      fs.writeFileSync(desktopPath, content, 'utf8');
      fs.chmodSync(desktopPath, 0o755);
    } else {
      if (fs.existsSync(desktopPath)) {
        fs.unlinkSync(desktopPath);
      }
    }
    return isAutostartEnabled();
  }

  app.setLoginItemSettings({
    openAtLogin: enabled,
    openAsHidden: true,
    args: ['--tray'],
  });
  return isAutostartEnabled();
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
function writeClearcoreSharedState(updates = {}) {
  const runtimeDir = process.env.XDG_RUNTIME_DIR || '/tmp';
  const statePath = path.join(runtimeDir, 'clearcore_state');
  try {
    let buf = Buffer.alloc(16, 0);
    if (fs.existsSync(statePath)) {
      try {
        const existing = fs.readFileSync(statePath);
        if (existing.length >= 16) {
          existing.copy(buf);
        }
      } catch {}
    }
    if (updates.mode !== undefined) {
      const modeVal = updates.mode === 'Bypass' ? 1 : updates.mode === 'Mute' ? 2 : 0;
      buf.writeUInt32LE(modeVal, 0);
    }
    if (updates.targetNodeId !== undefined) {
      buf.writeUInt32LE(Number(updates.targetNodeId) || 0, 4);
    }
    if (updates.generation !== undefined) {
      buf.writeUInt32LE(Number(updates.generation) || 0, 8);
    }
    fs.writeFileSync(statePath, buf);
  } catch (err) {
    console.warn('[Clearcore] Could not update shared state:', err.message);
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

  const contextMenu = Menu.buildFromTemplate([
    {
      label: `Clearcore [${currentMode.toUpperCase()}]`,
      enabled: false,
    },
    { type: 'separator' },
    {
      label: currentVirtualMicStatus.present
        ? `Microfone (${platformLabel}): 🟢 Ativo (ID: ${currentVirtualMicStatus.node_id || 'OK'})`
        : `Microfone (${platformLabel}): 🔴 Não Criado (Clique para Criar / Instalar)`,
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
        stopDaemon();
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

      // Link target ports
      const outLines = pwOut.split('\n');
      for (const port of outLines) {
        const trimmed = port.trim();
        if (!trimmed || trimmed.includes('realtime-noise')) continue;
        if (nodeName && trimmed.startsWith(nodeName)) {
          if (trimmed.endsWith('_FL') || trimmed.endsWith('_1')) {
            try { require('child_process').execSync(`pw-link "${trimmed}" "realtime-noise-capture:input_FL" 2>/dev/null || pw-link "${trimmed}" "realtime-noise-capture:input_MONO" 2>/dev/null || true`); } catch {}
          }
          if (trimmed.endsWith('_FR') || trimmed.endsWith('_2')) {
            try { require('child_process').execSync(`pw-link "${trimmed}" "realtime-noise-capture:input_FR" 2>/dev/null || true`); } catch {}
          }
        }
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

  // Show window if not explicitly asked to start minimized in tray
  if (!startInTray) {
    mainWindow.once('ready-to-show', () => {
      mainWindow.show();
      mainWindow.focus();
    });
    // Fallback: force show after 1s in dev mode if ready-to-show hasn't fired yet
    if (isDev) {
      setTimeout(() => {
        if (mainWindow && !mainWindow.isDestroyed() && !mainWindow.isVisible()) {
          mainWindow.show();
          mainWindow.focus();
        }
      }, 1000);
    }
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
  createTray();
  createWindow();

  // 1. Ensure daemon is running (auto-start sidecar if not running)
  await ensureDaemonRunning();

  // 2. Active startup check for virtual microphone
  await verifyAndAutoCreateVirtualMicOnStartup();

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
  stopDaemon();
});

// IPC handlers for frontend
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
  stopDaemon();
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

  if (isLinux) {
    try {
      const scriptPath = findScriptPath('detect-hardware.sh');
      if (fs.existsSync(scriptPath)) {
        const out = require('child_process').execSync(`"${scriptPath}" --json`, { encoding: 'utf8', timeout: 5000 });
        const parsed = JSON.parse(out);
        return {
          ...parsed,
          active_backend: currentSelectedBackend,
        };
      }
    } catch (err) {
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
        const parsed = JSON.parse(out);
        return {
          ...parsed,
          active_backend: currentSelectedBackend,
        };
      }
    } catch (err) {
      console.warn('Failed to detect Windows hardware via powershell:', err.message);
    }
  }

  // Cross-platform fallback definition
  const fallbackAutoResolved = isMac && process.arch === 'arm64'
    ? { id: 'apple_coreml', name: 'Apple Silicon (CoreML)' }
    : { id: 'cpu_tract', name: 'CPU Nativo (Tract Pure-Rust)' };

  return {
    auto_resolved_backend: fallbackAutoResolved,
    backends: [
      {
        id: 'auto',
        name: 'Automático (Melhor Acelerador)',
        tier: 'Auto',
        hardware_detected: true,
        runtime_installed: true,
        device_info: 'Seleção dinâmica por prioridade de hardware e disponibilidade',
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
        hardware_detected: false,
        runtime_installed: false,
        device_info: 'GPU Dedicada NVIDIA',
        runtime_name: isWin ? 'TensorRT (nvinfer.dll)' : 'TensorRT (libnvinfer.so)',
        install_script: isWin ? '.\\scripts\\install-tensorrt.ps1' : './scripts/install-tensorrt.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-tensorrt.ps1' : './scripts/install-tensorrt.sh',
        install_instruction: 'Instale o NVIDIA CUDA Toolkit e o pacote TensorRT oficial da NVIDIA para acelerar o modelo na GPU.',
      },
      {
        id: 'openvino_npu',
        name: 'Intel OpenVINO (NPU - AI Boost)',
        tier: 'Npu',
        hardware_detected: false,
        runtime_installed: false,
        device_info: 'Intel(R) AI Boost (NPU Neural dedicada no SoC)',
        runtime_name: isWin ? 'OpenVINO NPU (openvino_intel_npu_plugin.dll)' : 'OpenVINO NPU (libopenvino_intel_npu_plugin.so)',
        install_script: isWin ? '.\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_instruction: 'Instale o Intel OpenVINO runtime e o driver Intel NPU para habilitar o processamento de baixíssimo consumo na NPU.',
      },
      {
        id: 'openvino_gpu',
        name: 'Intel OpenVINO (iGPU - Intel Graphics)',
        tier: 'IntegratedGpu',
        hardware_detected: false,
        runtime_installed: false,
        device_info: 'Intel Arc Graphics / Arrow Lake iGPU',
        runtime_name: isWin ? 'OpenVINO GPU (openvino_intel_gpu_plugin.dll)' : 'OpenVINO GPU (libopenvino_intel_gpu_plugin.so)',
        install_script: isWin ? '.\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_instruction: 'Instale o Intel OpenVINO runtime e o driver compute-runtime para acelerar na GPU integrada.',
      },
      {
        id: 'openvino_cpu',
        name: 'Intel OpenVINO (CPU - Otimizado)',
        tier: 'Cpu',
        hardware_detected: true,
        runtime_installed: false,
        device_info: 'Processador Intel Core Ultra (AVX2 / AMX / VNNI)',
        runtime_name: isWin ? 'OpenVINO CPU (openvino_intel_cpu_plugin.dll)' : 'OpenVINO CPU (libopenvino_intel_cpu_plugin.so)',
        install_script: isWin ? '.\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-openvino.ps1' : './scripts/install-openvino.sh',
        install_instruction: 'Instale o Intel OpenVINO runtime para habilitar aceleração vetorial Intel na CPU.',
      },
      {
        id: 'amd_ryzenai_npu',
        name: 'AMD Ryzen AI (NPU - XDNA)',
        tier: 'Npu',
        hardware_detected: false,
        runtime_installed: false,
        device_info: 'AMD Ryzen AI NPU (XDNA / XDNA 2)',
        runtime_name: isWin ? 'Ryzen AI Software (xrt_core.dll)' : 'Ryzen AI Software (libxrt_core.so)',
        install_script: isWin ? '.\\scripts\\install-ryzenai.ps1' : './scripts/install-ryzenai.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-ryzenai.ps1' : './scripts/install-ryzenai.sh',
        install_instruction: 'Instale o driver AMD NPU e o Ryzen AI Software para acelerar no processador neural AMD.',
      },
      {
        id: 'amd_ryzenai_gpu',
        name: 'AMD Radeon (iGPU - RDNA Graphics)',
        tier: 'IntegratedGpu',
        hardware_detected: false,
        runtime_installed: false,
        device_info: 'AMD Radeon Graphics (iGPU integrada)',
        runtime_name: isWin ? 'DirectML / Vulkan (DirectML.dll)' : 'AMD ROCm / Vulkan',
        install_script: isWin ? '.\\scripts\\install-ryzenai.ps1' : './scripts/install-ryzenai.sh',
        install_command: isWin ? 'powershell .\\scripts\\install-ryzenai.ps1' : './scripts/install-ryzenai.sh',
        install_instruction: 'Instale os drivers gráficos AMD mais recentes para aceleração gráfica integrada.',
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
        device_info: 'Processador Host CPU (Execução Nativa Segura)',
        runtime_name: 'Tract (Embarcado, Zero Dependências)',
        install_script: '',
        install_command: '',
        install_instruction: 'Mecanismo padrão 100% seguro em Rust, sempre disponível sem necessidade de drivers.',
      },
    ],
    active_backend: currentSelectedBackend,
  };
}

ipcMain.handle('get_hardware_backends', () => {
  return queryHardwareBackends();
});

ipcMain.handle('set_hardware_backend', (_event, backendId) => {
  currentSelectedBackend = String(backendId || 'auto');
  return { success: true, active_backend: currentSelectedBackend };
});

