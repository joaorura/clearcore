const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('clearcoreApi', {
  getStatus: () => ipcRenderer.invoke('get_status'),
  setMode: (mode) => ipcRenderer.invoke('set_mode', { mode }),
  restartGeneration: () => ipcRenderer.invoke('restart_generation'),
  getDiagnostics: () => ipcRenderer.invoke('get_diagnostics'),
  getAutostart: () => ipcRenderer.invoke('get_autostart'),
  setAutostart: (enabled) => ipcRenderer.invoke('set_autostart', enabled),
  minimizeToTray: () => ipcRenderer.invoke('minimize_to_tray'),
  quitApp: () => ipcRenderer.invoke('quit_app'),
  onStatusUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('status-update', handler);
    return () => ipcRenderer.removeListener('status-update', handler);
  },
});

// Provide backward compatibility bridge for invokeTauri in React
contextBridge.exposeInMainWorld('__TAURI_INTERNALS__', {
  invoke: (cmd, args) => ipcRenderer.invoke(cmd, args),
});
