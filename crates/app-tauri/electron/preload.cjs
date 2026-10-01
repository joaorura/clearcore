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
  getVirtualMicStatus: () => ipcRenderer.invoke('get_virtual_mic_status'),
  recreateVirtualMic: () => ipcRenderer.invoke('recreate_virtual_mic'),
  setDefaultVirtualMic: () => ipcRenderer.invoke('set_default_virtual_mic'),
  getInputDevices: () => ipcRenderer.invoke('get_input_devices'),
  setInputDevice: (deviceId) => ipcRenderer.invoke('set_input_device', deviceId),
  getHardwareBackends: () => ipcRenderer.invoke('get_hardware_backends'),
  setHardwareBackend: (backendId) => ipcRenderer.invoke('set_hardware_backend', backendId),
  onStatusUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('status-update', handler);
    return () => ipcRenderer.removeListener('status-update', handler);
  },
  onVirtualMicUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('virtual-mic-update', handler);
    return () => ipcRenderer.removeListener('virtual-mic-update', handler);
  },
  onInputDevicesUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('input-devices-update', handler);
    return () => ipcRenderer.removeListener('input-devices-update', handler);
  },
});

// Provide backward compatibility bridge for invokeTauri in React
contextBridge.exposeInMainWorld('__TAURI_INTERNALS__', {
  invoke: (cmd, args) => ipcRenderer.invoke(cmd, args),
});
