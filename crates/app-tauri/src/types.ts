export type DenoiseMode = 'Active' | 'Bypass' | 'Mute';

export interface EngineStatus {
  state: string;
  is_terminal: boolean;
  can_restart: boolean;
  mode: DenoiseMode;
  crash_count_15m: number;
  total_crashes: number;
}

export interface VirtualMicStatus {
  platform?: 'linux' | 'windows' | 'macos' | string;
  platform_label?: string;
  present: boolean;
  node_id: number | string | null;
  node_name: string;
  node_description: string;
  driver_status?: string;
  driver_bundle?: string;
  driver_installed?: boolean;
  install_inf?: string;
  is_default: boolean;
  format?: string;
  rate?: number;
  channels?: number;
  quantum?: number;
  error?: string;
}

export interface InputDeviceInfo {
  id: string;
  name: string;
  is_default?: boolean;
}
