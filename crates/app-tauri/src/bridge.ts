import type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo, StudioPreset, VoiceProfileStatus } from './types';
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
  /** Motivo curto quando o detector de hardware falhou (a lista vem do fallback do app). */
  detection_error?: string;
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
  startAudioService?: () => Promise<{ success: boolean; isRunning: boolean; virtualMic: VirtualMicStatus }>;
  stopAudioService?: () => Promise<{ success: boolean; isRunning: boolean; virtualMic: VirtualMicStatus }>;
  getServiceRunningState?: () => Promise<{ isRunning: boolean }>;
  getStartActivatedConfig?: () => Promise<boolean>;
  setStartActivatedConfig?: (enabled: boolean) => Promise<boolean>;
  getStudioPreset?: () => Promise<StudioPreset>;
  setStudioPreset?: (preset: StudioPreset) => Promise<{ success: boolean; preset: StudioPreset }>;
  getVoiceProfileStatus?: () => Promise<VoiceProfileStatus>;
  setVoiceProfile?: (profileData: unknown) => Promise<{ success: boolean }>;
  onServiceStateUpdate?: (cb: (data: { isRunning: boolean }) => void) => () => void;
  onStartActivatedConfigUpdate?: (cb: (enabled: boolean) => void) => () => void;
  onStatusUpdate: (cb: (data: { mode?: DenoiseMode }) => void) => () => void;
  onVirtualMicUpdate: (cb: (data: VirtualMicStatus) => void) => () => void;
  onInputDevicesUpdate: (cb: (data: { devices: InputDeviceInfo[]; selectedId: string | null }) => void) => () => void;
  onAutostartUpdate: (cb: (enabled: boolean) => void) => () => void;
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
    if (cmd === 'start_audio_service') {
      if (typeof api.startAudioService === 'function') {
        return (await api.startAudioService()) as unknown as T;
      }
    }
    if (cmd === 'stop_audio_service') {
      if (typeof api.stopAudioService === 'function') {
        return (await api.stopAudioService()) as unknown as T;
      }
    }
    if (cmd === 'get_service_running_state') {
      if (typeof api.getServiceRunningState === 'function') {
        return (await api.getServiceRunningState()) as unknown as T;
      }
    }
    if (cmd === 'get_start_activated_config') {
      if (typeof api.getStartActivatedConfig === 'function') {
        return (await api.getStartActivatedConfig()) as unknown as T;
      }
    }
    if (cmd === 'set_start_activated_config') {
      if (typeof api.setStartActivatedConfig === 'function') {
        return (await api.setStartActivatedConfig(Boolean(args?.enabled))) as unknown as T;
      }
    }
    if (cmd === 'get_studio_preset') {
      if (typeof api.getStudioPreset === 'function') {
        return (await api.getStudioPreset()) as unknown as T;
      }
    }
    if (cmd === 'set_studio_preset') {
      if (typeof api.setStudioPreset === 'function') {
        return (await api.setStudioPreset(args?.preset as StudioPreset)) as unknown as T;
      }
    }
    if (cmd === 'get_voice_profile_status') {
      if (typeof api.getVoiceProfileStatus === 'function') {
        return (await api.getVoiceProfileStatus()) as unknown as T;
      }
    }
    if (cmd === 'set_voice_profile') {
      if (typeof api.setVoiceProfile === 'function') {
        return (await api.setVoiceProfile(args?.profile)) as unknown as T;
      }
    }
  }
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }

  // Graceful browser / test fallbacks
  if (cmd === 'get_start_activated_config') {
    const val = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_start_activated') : null;
    return (val === null ? true : val === 'true') as unknown as T;
  }
  if (cmd === 'set_start_activated_config') {
    const enabled = Boolean(args?.enabled);
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('clearcore_start_activated', String(enabled));
    }
    return enabled as unknown as T;
  }
  if (cmd === 'get_service_running_state') {
    const state = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_service_running') : null;
    return { isRunning: state === null ? true : state === 'true' } as unknown as T;
  }
  if (cmd === 'start_audio_service') {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('clearcore_service_running', 'true');
    }
    return { success: true, isRunning: true, virtualMic: { present: true, node_id: 1, node_name: 'realtime-noise-source' } } as unknown as T;
  }
  if (cmd === 'stop_audio_service') {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('clearcore_service_running', 'false');
    }
    return { success: true, isRunning: false, virtualMic: { present: false, node_id: null, node_name: 'realtime-noise-source' } } as unknown as T;
  }
  if (cmd === 'get_studio_preset') {
    const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_studio_preset') : null;
    return (saved || 'Natural') as unknown as T;
  }
  if (cmd === 'set_studio_preset') {
    const preset = String(args?.preset ?? 'Natural');
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('clearcore_studio_preset', preset);
    }
    return { success: true, preset } as unknown as T;
  }
  if (cmd === 'get_voice_profile_status') {
    const enrolled = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_voice_profile_enrolled') === 'true' : false;
    const count = typeof localStorage !== 'undefined' ? Number(localStorage.getItem('clearcore_voice_sample_count') || (enrolled ? '5' : '0')) : 0;
    return {
      is_enrolled: enrolled,
      active_samples_count: count,
      embedding_dim: 192,
      neural_eq_calibrated: enrolled,
      gain_boost_db: 1.8,
    } as unknown as T;
  }
  if (cmd === 'set_voice_profile') {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('clearcore_voice_profile_enrolled', 'true');
    }
    return { success: true } as unknown as T;
  }

  return {} as T;
}
