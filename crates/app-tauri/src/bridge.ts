import type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo } from './types';
import type { DiagnosticsData } from './diagnostics';

export interface HardwareBackendItem {
  id: string;
  name: string;
  tier: string;
  hardware_detected: boolean;
  runtime_installed: boolean;
  device_info: string;
  runtime_name: string;
  install_script: string;
  install_command: string;
  install_instruction: string;
  auto_resolved_id?: string;
  auto_resolved_name?: string;
}

export interface HardwareBackendsResponse {
  backends: HardwareBackendItem[];
  active_backend: string;
  auto_resolved_backend?: {
    id: string;
    name: string;
  };
}

export interface ClearcoreApi {
  getStatus: () => Promise<EngineStatus>;
  setMode: (mode: string) => Promise<unknown>;
  restartGeneration: () => Promise<unknown>;
  getDiagnostics: () => Promise<DiagnosticsData>;
  getAutostart: () => Promise<boolean>;
  setAutostart: (enabled: boolean) => Promise<boolean>;
  minimizeToTray: () => Promise<void>;
  quitApp: () => Promise<void>;
  getVirtualMicStatus: () => Promise<VirtualMicStatus>;
  recreateVirtualMic: () => Promise<VirtualMicStatus>;
  setDefaultVirtualMic: () => Promise<VirtualMicStatus>;
  getInputDevices: () => Promise<InputDeviceInfo[]>;
  setInputDevice: (deviceId: string) => Promise<{ success: boolean; selectedId: string }>;
  getHardwareBackends: () => Promise<HardwareBackendsResponse>;
  setHardwareBackend: (backendId: string) => Promise<{ success: boolean; active_backend: string }>;
  onStatusUpdate: (cb: (data: { mode?: DenoiseMode }) => void) => () => void;
  onVirtualMicUpdate: (cb: (data: VirtualMicStatus) => void) => () => void;
  onInputDevicesUpdate: (cb: (data: { devices: InputDeviceInfo[]; selectedId: string | null }) => void) => () => void;
}

declare global {
  interface Window {
    clearcoreApi?: ClearcoreApi;
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

export async function invokeBridge<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window !== 'undefined' && window.clearcoreApi) {
    const api = window.clearcoreApi;
    if (cmd === 'get_status') return (await api.getStatus()) as unknown as T;
    if (cmd === 'set_mode') return (await api.setMode(String(args?.mode ?? 'Active'))) as unknown as T;
    if (cmd === 'restart_generation') return (await api.restartGeneration()) as unknown as T;
    if (cmd === 'get_diagnostics') return (await api.getDiagnostics()) as unknown as T;
    if (cmd === 'get_autostart') return (await api.getAutostart()) as unknown as T;
    if (cmd === 'set_autostart') return (await api.setAutostart(Boolean(args?.enabled))) as unknown as T;
    if (cmd === 'minimize_to_tray') return (await api.minimizeToTray()) as unknown as T;
    if (cmd === 'quit_app') return (await api.quitApp()) as unknown as T;
    if (cmd === 'get_virtual_mic_status') return (await api.getVirtualMicStatus()) as unknown as T;
    if (cmd === 'recreate_virtual_mic') return (await api.recreateVirtualMic()) as unknown as T;
    if (cmd === 'set_default_virtual_mic') return (await api.setDefaultVirtualMic()) as unknown as T;
    if (cmd === 'get_input_devices') return (await api.getInputDevices()) as unknown as T;
    if (cmd === 'set_input_device') return (await api.setInputDevice(String(args?.deviceId ?? ''))) as unknown as T;
    if (cmd === 'get_hardware_backends') return (await api.getHardwareBackends()) as unknown as T;
    if (cmd === 'set_hardware_backend') return (await api.setHardwareBackend(String(args?.backendId ?? 'auto'))) as unknown as T;
  }
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  return {} as T;
}
