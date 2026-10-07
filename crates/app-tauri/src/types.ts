export type DenoiseMode = 'Active' | 'Bypass' | 'Mute';

export interface EngineStatus {
  state: string;
  is_terminal: boolean;
  can_restart: boolean;
  mode: DenoiseMode;
  crash_count_15m: number;
  total_crashes: number;
  filter_intensity?: number;
  voice_leveler?: number;
  voice_leveler_intensity?: number;
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

/** A call take as listed by the service (fields it does not send stay absent, never invented). */
export interface CallSuggestionTake {
  id: string;
  title?: string;
  timestamp: string;
  durationSec?: number;
  snrDb?: number;
  audioUrl?: string;
  speech_seconds?: number;
  device_label?: string;
}

export interface VoiceProfileStatus {
  is_enrolled: boolean;
  active_samples_count: number;
  /** Service-reported only; never invented by the renderer. */
  embedding_dim?: number;
  /** Service-reported only; never invented by the renderer. */
  neural_eq_calibrated?: boolean;
  gain_boost_db?: number;
  is_voice_profile_active?: boolean;
  stored_voice_profile_id?: string | null;
  voice_profile_error?: string | null;
  voice_profile_selected?: boolean;
  active_voice_profile_id?: string | null;
  /** Service-reported (GetStatus): the service holds a stored profile. */
  has_voice_profile?: boolean;
  /**
   * Service-reported (GetStatus): the active isolation model can apply a voice profile.
   * `undefined` (older service) means unknown and keeps the build available.
   */
  voice_profile_supported?: boolean;
  /**
   * Service-reported (GetStatus), development only: 'pdfnet3-dev' while the unsigned pDFNet3
   * (M2 NO-GO checkpoint) is the isolation model, 'base' otherwise. `undefined` = older service.
   */
  dev_base_model?: 'pdfnet3-dev' | 'base';
  /** Service-reported fixed code (`DEV_MODEL_*`) when the configured development model is not in use. */
  dev_base_model_error?: string | null;
  /** Service-reported: whether voice isolation is enabled in settings. */
  voice_isolation_enabled?: boolean;
}

