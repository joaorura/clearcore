const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('clearcoreApi', {
  getVersion: () => ipcRenderer.invoke('get_app_version'),
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
  startAudioService: () => ipcRenderer.invoke('start_audio_service'),
  stopAudioService: () => ipcRenderer.invoke('stop_audio_service'),
  getServiceRunningState: () => ipcRenderer.invoke('get_service_running_state'),
  getStartActivatedConfig: () => ipcRenderer.invoke('get_start_activated_config'),
  setStartActivatedConfig: (enabled) => ipcRenderer.invoke('set_start_activated_config', enabled),
  getStudioPreset: () => ipcRenderer.invoke('get_studio_preset'),
  setStudioPreset: (preset) => ipcRenderer.invoke('set_studio_preset', preset),
  getVoiceProfile: () => ipcRenderer.invoke('get_voice_profile'),
  getVoiceProfileStatus: () => ipcRenderer.invoke('get_voice_profile_status'),
  setVoiceProfile: (profile) => ipcRenderer.invoke('set_voice_profile', { profile }),
  getCallTakes: () => ipcRenderer.invoke('get_call_takes'),
  approveCallTake: (id, name, take) => ipcRenderer.invoke('approve_call_take', { id, name, take }),
  dismissCallTake: (id) => ipcRenderer.invoke('dismiss_call_take', { id }),
  exportDiagnostics: () => ipcRenderer.invoke('export_diagnostics'),
  onVoiceProfileUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('voice-profile-update', handler);
    return () => ipcRenderer.removeListener('voice-profile-update', handler);
  },
  onServiceStateUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('service-state-update', handler);
    return () => ipcRenderer.removeListener('service-state-update', handler);
  },
  onStartActivatedConfigUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('start-activated-config-update', handler);
    return () => ipcRenderer.removeListener('start-activated-config-update', handler);
  },
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
  onAutostartUpdate: (callback) => {
    const handler = (_event, data) => callback(data);
    ipcRenderer.on('autostart-update', handler);
    return () => ipcRenderer.removeListener('autostart-update', handler);
  },
});

// Provide backward compatibility bridge for invokeTauri in React
contextBridge.exposeInMainWorld('__TAURI_INTERNALS__', {
  invoke: (cmd, args) => ipcRenderer.invoke(cmd, args),
});
