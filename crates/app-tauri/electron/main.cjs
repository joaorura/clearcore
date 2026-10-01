const { app, BrowserWindow, Tray, Menu, nativeImage, ipcMain } = require('electron');
const path = require('path');
const fs = require('fs');
const net = require('net');
const os = require('os');

// Enforce single instance lock
const gotTheLock = app.requestSingleInstanceLock();
if (!gotTheLock) {
  app.quit();
  process.exit(0);
}

let mainWindow = null;
let tray = null;
let isQuitting = false;
let currentMode = 'Active';
let currentStatus = {
  state: 'Running',
  is_terminal: false,
  can_restart: true,
  mode: 'Active',
  crash_count_15m: 0,
  total_crashes: 0,
};

// Check if launched with --tray or --hidden or from autostart
const startInTray = process.argv.includes('--tray') || process.argv.includes('--hidden') || process.argv.includes('--minimized');

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
    const req = JSON.stringify({
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
      const execPath = process.execPath;
      const appPath = path.resolve(__dirname, '..');
      const content = `[Desktop Entry]
Type=Application
Name=Clearcore Realtime Noise Suppression
Comment=Audio Noise Suppression Virtual Microphone (Tray Companion)
Exec="${execPath}" "${path.join(__dirname, 'main.cjs')}" --tray
Icon=${path.join(appPath, 'assets', 'icon.png')}
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
  const assetsDir = path.join(__dirname, '..', 'assets');
  switch (mode) {
    case 'Active':
      return path.join(assetsDir, 'tray-active.png');
    case 'Bypass':
      return path.join(assetsDir, 'tray-bypass.png');
    case 'Mute':
      return path.join(assetsDir, 'tray-mute.png');
    default:
      return path.join(assetsDir, 'tray-active.png');
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

  const contextMenu = Menu.buildFromTemplate([
    {
      label: `Clearcore [${currentMode.toUpperCase()}]`,
      enabled: false,
    },
    { type: 'separator' },
    {
      label: 'Ativo (DeepFilterNet3)',
      type: 'radio',
      checked: currentMode === 'Active',
      click: async () => {
        try {
          await sendIpcRequest({ SetMode: 'Active' });
          currentMode = 'Active';
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
      label: 'Sair do Clearcore',
      click: () => {
        isQuitting = true;
        app.quit();
      },
    },
  ]);

  tray.setToolTip(`Clearcore Noise Suppression (${currentMode})`);
  tray.setContextMenu(contextMenu);
}

function createTray() {
  const iconPath = getTrayIconPath(currentMode);
  const icon = fs.existsSync(iconPath)
    ? nativeImage.createFromPath(iconPath)
    : nativeImage.createEmpty();

  tray = new Tray(icon);
  tray.setToolTip('Clearcore Realtime Noise Suppression');

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

function createWindow() {
  const assetsDir = path.join(__dirname, '..', 'assets');
  const iconPath = path.join(assetsDir, 'icon.png');

  mainWindow = new BrowserWindow({
    width: 820,
    height: 680,
    minWidth: 640,
    minHeight: 520,
    show: false, // Start hidden to obey "inicie na bandeja"
    icon: fs.existsSync(iconPath) ? iconPath : undefined,
    webPreferences: {
      preload: path.join(__dirname, 'preload.cjs'),
      nodeIntegration: false,
      contextIsolation: true,
      sandbox: false,
    },
  });

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
  if (process.argv.includes('--dev')) {
    mainWindow.loadURL('http://127.0.0.1:5173').catch(() => {
      mainWindow.loadFile(distHtml);
    });
  } else if (fs.existsSync(distHtml)) {
    mainWindow.loadFile(distHtml);
  } else {
    mainWindow.loadURL('http://127.0.0.1:5173');
  }

  // Only show the window if NOT asked to start in tray
  if (!startInTray) {
    mainWindow.once('ready-to-show', () => {
      mainWindow.show();
    });
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
}

app.whenReady().then(() => {
  createTray();
  createWindow();

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
});

// IPC handlers for frontend
ipcMain.handle('get_status', async () => {
  return await sendIpcRequest('GetStatus');
});

ipcMain.handle('set_mode', async (_event, args) => {
  const mode = args && args.mode ? args.mode : 'Active';
  const res = await sendIpcRequest({ SetMode: mode });
  currentMode = mode;
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
  app.quit();
});
