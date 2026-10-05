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

export type StudioPreset = 'Natural' | 'Podcast' | 'Broadcast' | 'Off';

export interface VoiceSample {
  id: string;
  title: string;
  category?: string;
  timestamp: string;
  durationSec: number;
  audioUrl?: string;
  isInitialStep?: boolean;
}

export interface CallSuggestionTake {
  id: string;
  title: string;
  timestamp: string;
  durationSec: number;
  snrDb: number;
  audioUrl?: string;
}

export interface VoiceProfileStatus {
  is_enrolled: boolean;
  active_samples_count: number;
  embedding_dim: number;
  neural_eq_calibrated: boolean;
  gain_boost_db?: number;
  is_voice_profile_active?: boolean;
  stored_voice_profile_id?: string | null;
  voice_profile_error?: string | null;
  voice_profile_selected?: boolean;
  active_voice_profile_id?: string | null;
}

