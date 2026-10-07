import type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo, StudioPreset, VoiceProfileStatus, VoiceSample, CallSuggestionTake } from './types';
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
  /** Indica se há GPU NVIDIA detectada cujo engine TensorRT ainda precisa ser compilado */
  model_compilation_needed?: boolean;
  /** Indica se o engine TensorRT já foi compilado para a GPU local */
  model_compiled?: boolean;
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
  setHardwareBackend?: (backendId: unknown) => Promise<{ success: boolean; reason?: string; active_backend: string }>;
  startAudioService?: () => Promise<{ success: boolean; isRunning: boolean; virtualMic: VirtualMicStatus }>;
  stopAudioService?: () => Promise<{ success: boolean; isRunning: boolean; virtualMic: VirtualMicStatus }>;
  getServiceRunningState?: () => Promise<{ isRunning: boolean }>;
  getStartActivatedConfig?: () => Promise<boolean>;
  setStartActivatedConfig?: (enabled: boolean) => Promise<boolean>;
  getStudioPreset?: () => Promise<StudioPreset>;
  setStudioPreset?: (preset: StudioPreset) => Promise<{ success: boolean; preset: StudioPreset }>;
  getVoiceLeveler?: () => Promise<number>;
  setVoiceLeveler?: (intensity: number) => Promise<{ success: boolean; intensity: number; voice_leveler?: number }>;
  getFilterIntensity?: () => Promise<number>;
  setFilterIntensity?: (intensity: number) => Promise<{ success: boolean; filter_intensity: number }>;
  getVoiceProfile?: () => Promise<VoiceProfileStatus & { success: boolean; profile: VoiceProfileStatus }>;
  getVoiceProfileStatus?: () => Promise<VoiceProfileStatus>;
  setVoiceProfile?: (profileData: unknown) => Promise<{ success: boolean; profile?: VoiceProfileStatus }>;
  setVoiceIsolation?: (enabled: boolean) => Promise<{ success: boolean; voice_isolation_enabled?: boolean; profile?: VoiceProfileStatus }>;
  getCallTakes?: () => Promise<{ success: boolean; takes: CallSuggestionTake[] } | CallSuggestionTake[]>;
  listVoiceSamples?: () => Promise<unknown>;
  listSamples?: () => Promise<unknown>;
  approveCallTake?: (id: string, name?: string, take?: unknown) => Promise<{ success: boolean; id: string; sample?: VoiceSample }>;
  dismissCallTake?: (id: string) => Promise<{ success: boolean; id: string; takes?: CallSuggestionTake[] }>;
  exportDiagnostics?: () => Promise<string>;
  logMessage?: (level: string, target: string, message: string, data?: unknown) => Promise<{ ok: boolean }>;
  onVoiceProfileUpdate?: (cb: (profile: VoiceProfileStatus) => void) => () => void;
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

const memStorage: Record<string, string> = {};
function storageGet(key: string): string | null {
  if (typeof localStorage !== 'undefined') {
    try {
      const val = localStorage.getItem(key);
      if (val !== null) return val;
    } catch {
      // fallback
    }
  }
  return memStorage[key] ?? null;
}

function storageSet(key: string, val: string): void {
  memStorage[key] = val;
  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.setItem(key, val);
    } catch {
      // fallback
    }
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
    if (cmd === 'set_hardware_backend') {
      if (typeof api.setHardwareBackend === 'function') {
        const id = args?.backendId ?? args?.backend ?? args;
        return (await api.setHardwareBackend(id)) as unknown as T;
      }
    }
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
    if (cmd === 'get_voice_leveler') {
      if (typeof api.getVoiceLeveler === 'function') {
        return (await api.getVoiceLeveler()) as unknown as T;
      }
    }
    if (cmd === 'set_voice_leveler') {
      if (typeof api.setVoiceLeveler === 'function') {
        const intensity = typeof args?.intensity === 'number' ? args.intensity : (typeof args === 'number' ? args : 0);
        return (await api.setVoiceLeveler(intensity)) as unknown as T;
      }
    }
    if (cmd === 'get_filter_intensity') {
      if (typeof api.getFilterIntensity === 'function') {
        return (await api.getFilterIntensity()) as unknown as T;
      }
    }
    if (cmd === 'set_filter_intensity') {
      if (typeof api.setFilterIntensity === 'function') {
        const intensity = typeof args?.intensity === 'number' ? args.intensity : (typeof args === 'number' ? args : 50);
        return (await api.setFilterIntensity(intensity)) as unknown as T;
      }
    }
    if (cmd === 'get_voice_profile' || cmd === 'get_voice_profile_status') {
      if (typeof api.getVoiceProfile === 'function') {
        return (await api.getVoiceProfile()) as unknown as T;
      }
      if (typeof api.getVoiceProfileStatus === 'function') {
        return (await api.getVoiceProfileStatus()) as unknown as T;
      }
    }
    if (cmd === 'set_voice_profile') {
      if (typeof api.setVoiceProfile === 'function') {
        return (await api.setVoiceProfile(args?.profile ?? args)) as unknown as T;
      }
    }
    if (cmd === 'set_voice_isolation') {
      if (typeof api.setVoiceIsolation === 'function') {
        const enabled = typeof args?.enabled === 'boolean' ? args.enabled : Boolean(args);
        return (await api.setVoiceIsolation(enabled)) as unknown as T;
      }
    }
    if (cmd === 'get_call_takes') {
      if (typeof api.getCallTakes === 'function') {
        return (await api.getCallTakes()) as unknown as T;
      }
    }
    if (cmd === 'enrollment_list_samples' || cmd === 'list_voice_samples') {
      if (typeof api.listVoiceSamples === 'function') {
        return (await api.listVoiceSamples()) as unknown as T;
      }
      if (typeof api.listSamples === 'function') {
        return (await api.listSamples()) as unknown as T;
      }
    }
    if (cmd === 'approve_call_take') {
      if (typeof api.approveCallTake === 'function') {
        return (await api.approveCallTake(String(args?.id ?? ''), args?.name as string | undefined, args?.take)) as unknown as T;
      }
    }
    if (cmd === 'dismiss_call_take') {
      if (typeof api.dismissCallTake === 'function') {
        return (await api.dismissCallTake(String(args?.id ?? ''))) as unknown as T;
      }
    }
    if (cmd === 'export_diagnostics') {
      if (typeof api.exportDiagnostics === 'function') {
        return (await api.exportDiagnostics()) as unknown as T;
      }
    }
    if (cmd === 'log_message') {
      if (typeof api.logMessage === 'function') {
        const level = String(args?.level || 'INFO');
        const target = String(args?.target || 'FRONTEND');
        const message = String(args?.message || '');
        return (await api.logMessage(level, target, message, args?.data)) as unknown as T;
      }
    }
  }
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
    if (typeof window.__TAURI_INTERNALS__.invoke === 'function') {
      return window.__TAURI_INTERNALS__.invoke<T>(cmd, args);
    }
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }

  // Graceful browser / test fallbacks
  if (cmd === 'get_start_activated_config') {
    const val = storageGet('clearcore_start_activated');
    return (val === null ? true : val === 'true') as unknown as T;
  }
  if (cmd === 'set_start_activated_config') {
    const enabled = Boolean(args?.enabled);
    storageSet('clearcore_start_activated', String(enabled));
    return enabled as unknown as T;
  }
  if (cmd === 'get_service_running_state') {
    const state = storageGet('clearcore_service_running');
    return { isRunning: state === null ? true : state === 'true' } as unknown as T;
  }
  if (cmd === 'start_audio_service') {
    storageSet('clearcore_service_running', 'true');
    return { success: true, isRunning: true, virtualMic: { present: true, node_id: 1, node_name: 'realtime-noise-source' } } as unknown as T;
  }
  if (cmd === 'stop_audio_service') {
    storageSet('clearcore_service_running', 'false');
    return { success: true, isRunning: false, virtualMic: { present: false, node_id: null, node_name: 'realtime-noise-source' } } as unknown as T;
  }
  if (cmd === 'get_studio_preset') {
    const saved = storageGet('clearcore_studio_preset');
    return (saved || 'Natural') as unknown as T;
  }
  if (cmd === 'set_studio_preset') {
    const preset = String(args?.preset ?? 'Natural');
    storageSet('clearcore_studio_preset', preset);
    return { success: true, preset } as unknown as T;
  }
  if (cmd === 'get_voice_leveler') {
    const saved = storageGet('clearcore_voice_leveler');
    return (saved ? Number(saved) : 0) as unknown as T;
  }
  if (cmd === 'set_voice_leveler') {
    const raw = args?.intensity ?? args;
    const intensity = Math.max(0, Math.min(100, Math.round(Number(raw ?? 0))));
    storageSet('clearcore_voice_leveler', String(intensity));
    return { success: true, intensity, voice_leveler: intensity } as unknown as T;
  }
  if (cmd === 'get_filter_intensity') {
    const saved = storageGet('clearcore_filter_intensity');
    return (saved ? Number(saved) : 50) as unknown as T;
  }
  if (cmd === 'set_filter_intensity') {
    const raw = args?.intensity ?? args;
    const intensity = Math.max(0, Math.min(100, Math.round(Number(raw ?? 50))));
    storageSet('clearcore_filter_intensity', String(intensity));
    return { success: true, filter_intensity: intensity } as unknown as T;
  }
  // No service in a plain browser: the voice profile is neutral and nothing is kept locally
  // (samples, takes and the enrolled flag live only in the service).
  if (cmd === 'get_voice_profile' || cmd === 'get_voice_profile_status' || cmd === 'set_voice_profile') {
    const profile: VoiceProfileStatus = { is_enrolled: false, active_samples_count: 0 };
    return { success: true, profile, ...profile } as unknown as T;
  }
  if (cmd === 'get_call_takes') {
    return [] as unknown as T;
  }
  if (cmd === 'approve_call_take') {
    return { success: true, id: String(args?.id ?? '') } as unknown as T;
  }
  if (cmd === 'dismiss_call_take') {
    return { success: true, id: String(args?.id ?? '') } as unknown as T;
  }
  if (cmd === 'export_diagnostics') {
    return JSON.stringify({ app: 'ClearCore', version: '0.1.0-beta.1' }) as unknown as T;
  }

  return {} as T;
}

export async function getFilterIntensity(): Promise<number> {
  return await invokeBridge<number>('get_filter_intensity');
}

export async function setFilterIntensity(intensity: number): Promise<{ success: boolean; filter_intensity: number }> {
  return await invokeBridge<{ success: boolean; filter_intensity: number }>('set_filter_intensity', { intensity });
}

export async function getVoiceLeveler(): Promise<number> {
  return await invokeBridge<number>('get_voice_leveler');
}

export async function setVoiceLeveler(intensity: number): Promise<{ success: boolean; intensity: number; voice_leveler?: number }> {
  return await invokeBridge<{ success: boolean; intensity: number; voice_leveler?: number }>('set_voice_leveler', { intensity });
}
