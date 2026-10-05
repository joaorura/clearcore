import React, { useState, useRef, useEffect, useCallback } from 'react';
import { useI18n } from './i18n';
import { invokeBridge } from './bridge';
import type { VoiceSample, CallSuggestionTake, VoiceProfileStatus, InputDeviceInfo } from './types';

export interface VoiceProfileCardProps {
  selectedInputId?: string;
  virtualMicPresent?: boolean;
  inputDevices?: InputDeviceInfo[];
}

/**
 * Detect if a media device label represents a virtual microphone, monitor sink, or loopback device.
 */
export const isVirtualOrLoopbackAudioDevice = (label: string): boolean => {
  const l = (label || '').toLowerCase();
  return (
    l.includes('realtime') ||
    l.includes('clearcore') ||
    l.includes('virtual') ||
    l.includes('monitor') ||
    l.includes('loopback')
  );
};

/**
 * Resolves the true physical microphone device from enumerated MediaDeviceInfo list,
 * matching against the PipeWire/system device info and filtering out virtual/loopback devices.
 */
export const resolvePhysicalAudioDevice = (
  audioInputs: MediaDeviceInfo[],
  selectedInputId?: string,
  inputDevices?: InputDeviceInfo[]
): MediaDeviceInfo | undefined => {
  const physicalCandidates = audioInputs.filter(
    (d) => !isVirtualOrLoopbackAudioDevice(d.label)
  );

  // 1. Direct match on Chromium deviceId (if it's not 'default' or 'communications')
  if (selectedInputId && selectedInputId !== 'default' && selectedInputId !== 'communications') {
    const directMatch = physicalCandidates.find((d) => d.deviceId === selectedInputId);
    if (directMatch) return directMatch;
  }

  // 2. Match known physical device from inputDevices by name/label
  const knownPhysical = inputDevices?.find((d) => d.id === selectedInputId);
  if (knownPhysical?.name) {
    const cleanKnownName = knownPhysical.name.toLowerCase().trim();
    const nameMatch = physicalCandidates.find((d) => {
      const devLabel = d.label.toLowerCase().trim();
      return (
        devLabel.length > 0 &&
        (devLabel.includes(cleanKnownName) || cleanKnownName.includes(devLabel))
      );
    });
    if (nameMatch) return nameMatch;
  }

  // 3. Fallback to any physical device that is NOT default or communications and has a label
  const specificPhysical = physicalCandidates.find(
    (d) =>
      d.deviceId !== 'default' &&
      d.deviceId !== 'communications' &&
      d.label.length > 0
  );
  if (specificPhysical) return specificPhysical;

  // 4. Any physical candidate with deviceId not default/communications
  const nonDefault = physicalCandidates.find(
    (d) => d.deviceId !== 'default' && d.deviceId !== 'communications'
  );
  if (nonDefault) return nonDefault;

  // 5. Any candidate in physicalCandidates
  if (physicalCandidates.length > 0) return physicalCandidates[0];

  return undefined;
};

/**
 * Identifies the best supported MediaRecorder MIME type for optimal audio fidelity.
 */
export const getPreferredAudioMimeType = (): string => {
  if (typeof MediaRecorder === 'undefined' || typeof MediaRecorder.isTypeSupported !== 'function') {
    return 'audio/webm';
  }
  const candidates = [
    'audio/webm;codecs=opus',
    'audio/webm',
    'audio/ogg;codecs=opus',
    'audio/mp4',
  ];
  for (const candidate of candidates) {
    if (MediaRecorder.isTypeSupported(candidate)) {
      return candidate;
    }
  }
  return 'audio/webm';
};

/**
 * Standard base audio constraints for voice profile capture.
 * Browser native echo cancellation, noise suppression, and auto gain control are disabled
 * to capture clean, uncolored, and un-clipped raw microphone input.
 */
export const PURE_VOICE_CAPTURE_CONSTRAINTS: MediaTrackConstraints = {
  echoCancellation: false,
  noiseSuppression: false,
  autoGainControl: false,
  channelCount: 1,
};

/**
 * Cleanly acquires a physical microphone MediaStream without virtual loops or browser audio filtering.
 */
export const acquireCleanPhysicalStream = async (
  selectedInputId?: string,
  inputDevices?: InputDeviceInfo[]
): Promise<MediaStream> => {
  if (typeof navigator === 'undefined' || !navigator.mediaDevices?.getUserMedia) {
    throw new Error('navigator.mediaDevices.getUserMedia is unavailable');
  }

  let devs: MediaDeviceInfo[] = [];
  if (navigator.mediaDevices.enumerateDevices) {
    try {
      devs = await navigator.mediaDevices.enumerateDevices();
      if (devs.length > 0 && devs.every((d) => !d.label)) {
        try {
          const probe = await navigator.mediaDevices.getUserMedia({ audio: true });
          probe.getTracks().forEach((t) => t.stop());
          devs = await navigator.mediaDevices.enumerateDevices();
        } catch {}
      }
    } catch {}
  }

  const audioInputs = devs.filter((d) => d.kind === 'audioinput');
  const physicalDev = resolvePhysicalAudioDevice(audioInputs, selectedInputId, inputDevices);

  let stream: MediaStream | null = null;

  if (physicalDev?.deviceId && physicalDev.deviceId !== 'default' && physicalDev.deviceId !== 'communications') {
    try {
      stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          deviceId: { exact: physicalDev.deviceId },
          ...PURE_VOICE_CAPTURE_CONSTRAINTS,
        },
        video: false,
      });
    } catch (err) {
      console.warn('Could not acquire microphone with deviceId.exact, trying ideal:', err);
    }

    if (!stream) {
      try {
        stream = await navigator.mediaDevices.getUserMedia({
          audio: {
            deviceId: physicalDev.deviceId,
            ...PURE_VOICE_CAPTURE_CONSTRAINTS,
          },
          video: false,
        });
      } catch (err) {
        console.warn('Could not acquire microphone with ideal deviceId:', err);
      }
    }
  }

  if (!stream) {
    stream = await navigator.mediaDevices.getUserMedia({
      audio: PURE_VOICE_CAPTURE_CONSTRAINTS,
      video: false,
    });
  }

  return stream;
};

/**
 * Safely stops a MediaRecorder and awaits the final onstop callback to assemble complete audio chunks.
 */
export const stopMediaRecorderAsync = (
  recorder: MediaRecorder | null,
  chunks: Blob[]
): Promise<Blob | null> => {
  return new Promise((resolve) => {
    if (!recorder || recorder.state === 'inactive') {
      if (chunks.length > 0) {
        resolve(new Blob(chunks, { type: recorder?.mimeType || 'audio/webm' }));
      } else {
        resolve(null);
      }
      return;
    }

    recorder.onstop = () => {
      if (chunks.length > 0) {
        resolve(new Blob(chunks, { type: recorder.mimeType || 'audio/webm' }));
      } else {
        resolve(null);
      }
    };

    try {
      if (typeof recorder.requestData === 'function') {
        recorder.requestData();
      }
      recorder.stop();
    } catch {
      if (chunks.length > 0) {
        resolve(new Blob(chunks, { type: recorder.mimeType || 'audio/webm' }));
      } else {
        resolve(null);
      }
    }
  });
};

const MIN_RECORDING_SECONDS = 1.5;
const MAX_RECORDING_SECONDS = 30;

const STORAGE_ENROLLED_KEY = 'clearcore_voice_profile_enrolled';
const STORAGE_SAMPLES_KEY = 'clearcore_voice_profile_samples';
const STORAGE_CALL_TAKES_KEY = 'clearcore_voice_intake_takes';
const STORAGE_READING_MODE_KEY = 'clearcore_voice_reading_mode';

const INITIAL_CALL_TAKES: CallSuggestionTake[] = [
  {
    id: 'take-meet-104',
    title: 'Reunião de Alinhamento (Google Meet)',
    timestamp: 'Há 35 min',
    durationSec: 5.2,
    snrDb: 26.4,
  },
  {
    id: 'take-zoom-105',
    title: 'Chamada de Planejamento (Zoom)',
    timestamp: 'Ontem às 16:20',
    durationSec: 4.8,
    snrDb: 24.1,
  },
];

export function normalizeVoiceProfileStatus(res: unknown): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus> };
  const src = r.profile ?? r;
  return {
    is_enrolled: Boolean(src.is_enrolled),
    active_samples_count: src.active_samples_count ?? 0,
    embedding_dim: src.embedding_dim ?? 0,
    neural_eq_calibrated: Boolean(src.neural_eq_calibrated),
    gain_boost_db: src.gain_boost_db,
    is_voice_profile_active: src.is_voice_profile_active,
    stored_voice_profile_id: src.stored_voice_profile_id,
    voice_profile_error: src.voice_profile_error,
    voice_profile_selected: src.voice_profile_selected,
    active_voice_profile_id: src.active_voice_profile_id,
  };
}

export type VoiceProfileActivationState = 'active' | 'stored_not_applied' | 'none';

export type VoiceProfileStatusLabelKey = 'active' | 'storedNotApplied' | 'enrolledUnconfirmed' | 'none';

/** Only 'active' may be shown as active; enrolled without confirmation is neutral. */
export function voiceProfileStatusLabelKey(status: VoiceProfileStatus): VoiceProfileStatusLabelKey {
  const state = voiceProfileActivationState(status);
  if (state === 'active') return 'active';
  if (state === 'stored_not_applied') return 'storedNotApplied';
  return status.is_enrolled ? 'enrolledUnconfirmed' : 'none';
}

/** Stored must never be presented as applied: 'active' needs explicit confirmation. */
export function voiceProfileActivationState(status: VoiceProfileStatus): VoiceProfileActivationState {
  if (status.is_voice_profile_active === true) return 'active';
  const hasStoredId =
    typeof status.stored_voice_profile_id === 'string' && status.stored_voice_profile_id.length > 0;
  if (hasStoredId || (status.is_enrolled && status.is_voice_profile_active === false)) {
    return 'stored_not_applied';
  }
  return 'none';
}

export function mergeVoiceProfileStatus(
  initial: VoiceProfileStatus,
  res: unknown,
  loadedSamplesCount: number,
): VoiceProfileStatus {
  const r = (res ?? {}) as Partial<VoiceProfileStatus> & { profile?: Partial<VoiceProfileStatus> };
  const raw = r.profile ?? r;
  if (!raw || typeof raw !== 'object' || typeof raw.is_enrolled !== 'boolean') return initial;
  const prof = normalizeVoiceProfileStatus(res);
  return {
    ...initial,
    is_enrolled: prof.is_enrolled,
    neural_eq_calibrated: prof.neural_eq_calibrated,
    embedding_dim: raw.embedding_dim ?? initial.embedding_dim,
    gain_boost_db: raw.gain_boost_db ?? initial.gain_boost_db,
    is_voice_profile_active: prof.is_voice_profile_active,
    stored_voice_profile_id: prof.stored_voice_profile_id,
    voice_profile_error: prof.voice_profile_error,
    voice_profile_selected: prof.voice_profile_selected,
    active_voice_profile_id: prof.active_voice_profile_id,
    active_samples_count: loadedSamplesCount > 0 ? loadedSamplesCount : prof.active_samples_count,
  };
}

const SERVICE_VOICE_PROFILE_KEYS = [
  'is_voice_profile_active',
  'stored_voice_profile_id',
  'voice_profile_error',
  'voice_profile_selected',
  'active_voice_profile_id',
] as const;

/** Service-owned fields: the renderer never resends nor inherits them. */
export function stripServiceVoiceProfileKeys(status: VoiceProfileStatus): VoiceProfileStatus {
  const out: VoiceProfileStatus = { ...status };
  for (const key of SERVICE_VOICE_PROFILE_KEYS) delete out[key];
  return out;
}

/**
 * Applies the result of set_voice_profile: local fields come from what the user did, service
 * fields come ONLY from the result (missing/offline result => cleared, never kept as active).
 */
export function applySetVoiceProfileResult(
  prev: VoiceProfileStatus,
  localStatus: VoiceProfileStatus,
  res: unknown,
): VoiceProfileStatus {
  const base = { ...stripServiceVoiceProfileKeys(prev), ...stripServiceVoiceProfileKeys(localStatus) };
  const r = (res ?? {}) as { profile?: unknown };
  const raw = (r.profile ?? res) as Record<string, unknown> | null | undefined;
  if (!raw || typeof raw !== 'object') return base;
  const svc = normalizeVoiceProfileStatus(raw);
  return {
    ...base,
    is_voice_profile_active: typeof svc.is_voice_profile_active === 'boolean' ? svc.is_voice_profile_active : undefined,
    stored_voice_profile_id: svc.stored_voice_profile_id,
    voice_profile_error: svc.voice_profile_error,
    voice_profile_selected: svc.voice_profile_selected,
    active_voice_profile_id: svc.active_voice_profile_id,
  };
}

export type VoiceProfileErrorKey =
  | 'errorServiceUnavailable' | 'errorServiceError' | 'errorServiceRejected' | 'errorNotApplicable'
  | 'errorClearFailed' | 'errorNoProfileStore' | 'errorInvalidProfile' | 'errorPersistFailed'
  | 'errorNotApplied' | 'errorLoadFailed' | 'errorRestoreFailed' | 'errorUnknown';

const VOICE_PROFILE_ERROR_KEYS: Record<string, VoiceProfileErrorKey> = {
  service_unavailable: 'errorServiceUnavailable',
  service_error: 'errorServiceError',
  service_rejected: 'errorServiceRejected',
  VOICE_PROFILE_NOT_APPLICABLE: 'errorNotApplicable',
  VOICE_PROFILE_CLEAR_FAILED: 'errorClearFailed',
  NO_PROFILE_STORE: 'errorNoProfileStore',
  'Invalid voice profile': 'errorInvalidProfile',
  'Failed to persist voice profile': 'errorPersistFailed',
  'The backend could not apply the stored voice profile': 'errorNotApplied',
  'The stored voice profile failed validation or has insecure permissions': 'errorLoadFailed',
  'The previous voice profile could not be restored on the backend': 'errorRestoreFailed',
  'The active backend could not return to the neutral voice': 'errorClearFailed',
  'The active backend cannot apply this voice profile': 'errorNotApplicable',
  'Voice profile storage is not configured': 'errorNoProfileStore',
};

/** Never returns service free text: unknown input maps to errorUnknown. */
export function voiceProfileErrorKey(code: unknown): VoiceProfileErrorKey {
  if (typeof code !== 'string') return 'errorUnknown';
  return Object.prototype.hasOwnProperty.call(VOICE_PROFILE_ERROR_KEYS, code)
    ? VOICE_PROFILE_ERROR_KEYS[code]
    : 'errorUnknown';
}

export const VoiceProfileCard: React.FC<VoiceProfileCardProps> = ({
  selectedInputId,
  virtualMicPresent: _virtualMicPresent,
  inputDevices,
}) => {
  const { t } = useI18n();

  // Profile Status
  const [profileStatus, setProfileStatus] = useState<VoiceProfileStatus>({
    is_enrolled: false,
    active_samples_count: 0,
    embedding_dim: 192,
    neural_eq_calibrated: false,
    gain_boost_db: 1.8,
  });

  // Guided Multi-Sampling State (5 Steps)
  const [currentStep, setCurrentStep] = useState<number>(1);
  const [isReadingMode, setIsReadingMode] = useState<boolean>(false);
  const [completedSteps, setCompletedSteps] = useState<{ [step: number]: { audioUrl?: string; duration: number } }>({});
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null);

  // Active Tab: 'samples' (Galeria Cumulativa) | 'intake' (Sugestões de Chamadas)
  const [activeTab, setActiveTab] = useState<'samples' | 'intake'>('samples');

  // Cumulative Samples Gallery & Call Suggestions
  const [samples, setSamples] = useState<VoiceSample[]>([]);
  const [callTakes, setCallTakes] = useState<CallSuggestionTake[]>(INITIAL_CALL_TAKES);

  // Recording State (both for guided steps and modal voluntary sample)
  const [isRecording, setIsRecording] = useState<boolean>(false);
  const [recordingElapsedSeconds, setRecordingElapsedSeconds] = useState<number>(0);
  const [liveVoiceLevel, setLiveVoiceLevel] = useState<number>(0);

  // Modal for "+ Adicionar Nova Amostra"
  const [isModalOpen, setIsModalOpen] = useState<boolean>(false);
  const [modalSampleName, setModalSampleName] = useState<string>('');
  const [modalRecordedUrl, setModalRecordedUrl] = useState<string | null>(null);
  const [modalDuration, setModalDuration] = useState<number>(0);

  // Audio Playback State
  const [playingAudioId, setPlayingAudioId] = useState<string | null>(null);
  const audioElementRef = useRef<HTMLAudioElement | null>(null);

  // Audio Recording & Web Audio references
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const analyserRef = useRef<AnalyserNode | null>(null);
  const animationFrameRef = useRef<number | null>(null);
  const timerRef = useRef<NodeJS.Timeout | null>(null);
  const vuIntervalRef = useRef<NodeJS.Timeout | null>(null);
  const recordingStartTimeRef = useRef<number>(0);
  const recordedChunksRef = useRef<Blob[]>([]);

  // Step Question metadata definition
  const stepQuestions = [
    {
      step: 1,
      categoryKey: 'voiceProfile.question1Category',
      textKey: 'voiceProfile.question1Text',
      fallbackKey: 'voiceProfile.question1Fallback',
    },
    {
      step: 2,
      categoryKey: 'voiceProfile.question2Category',
      textKey: 'voiceProfile.question2Text',
      fallbackKey: 'voiceProfile.question2Fallback',
    },
    {
      step: 3,
      categoryKey: 'voiceProfile.question3Category',
      textKey: 'voiceProfile.question3Text',
      fallbackKey: 'voiceProfile.question3Fallback',
    },
    {
      step: 4,
      categoryKey: 'voiceProfile.question4Category',
      textKey: 'voiceProfile.question4Text',
      fallbackKey: 'voiceProfile.question4Fallback',
    },
    {
      step: 5,
      categoryKey: 'voiceProfile.question5Category',
      textKey: 'voiceProfile.question5Text',
      fallbackKey: 'voiceProfile.question5Fallback',
    },
  ];

  // Load initial persistent state
  useEffect(() => {
    const initVoiceData = async () => {
      try {
        const savedEnrolled = localStorage.getItem(STORAGE_ENROLLED_KEY) === 'true';
        const savedReading = localStorage.getItem(STORAGE_READING_MODE_KEY) === 'true';
        setIsReadingMode(savedReading);

        let loadedSamples: VoiceSample[] = [];
        try {
          const res = await invokeBridge<{ success?: boolean; samples?: VoiceSample[] } | VoiceSample[]>('get_voice_samples');
          if (Array.isArray(res) && res.length > 0) {
            loadedSamples = res;
          } else if (res && typeof res === 'object' && 'samples' in res && Array.isArray(res.samples) && res.samples.length > 0) {
            loadedSamples = res.samples;
          }
        } catch {}

        if (loadedSamples.length === 0) {
          const savedSamplesJson = localStorage.getItem(STORAGE_SAMPLES_KEY);
          if (savedSamplesJson) {
            loadedSamples = JSON.parse(savedSamplesJson);
          } else if (savedEnrolled) {
            // Generate initial 5 samples if marked enrolled
            loadedSamples = stepQuestions.map((q, idx) => ({
              id: `sample-${idx + 1}`,
              title: `Amostra ${idx + 1}: ${t(q.categoryKey)}`,
              category: t(q.categoryKey),
              timestamp: new Date().toLocaleDateString(),
              durationSec: 5.0,
              isInitialStep: true,
            }));
          }
        }
        setSamples(loadedSamples);

        let loadedTakes: CallSuggestionTake[] = [];
        try {
          const res = await invokeBridge<{ success?: boolean; takes?: CallSuggestionTake[] } | CallSuggestionTake[]>('get_call_takes');
          if (Array.isArray(res) && res.length > 0) {
            loadedTakes = res;
          } else if (res && typeof res === 'object' && 'takes' in res && Array.isArray(res.takes) && res.takes.length > 0) {
            loadedTakes = res.takes;
          }
        } catch {}

        if (loadedTakes.length === 0) {
          const savedTakesJson = localStorage.getItem(STORAGE_CALL_TAKES_KEY);
          if (savedTakesJson) {
            loadedTakes = JSON.parse(savedTakesJson);
          }
        }
        setCallTakes(loadedTakes);

        let initialProfile: VoiceProfileStatus = {
          is_enrolled: savedEnrolled && loadedSamples.length > 0,
          active_samples_count: loadedSamples.length,
          embedding_dim: 192,
          neural_eq_calibrated: savedEnrolled && loadedSamples.length > 0,
          gain_boost_db: 1.8,
        };

        try {
          const profileRes = await invokeBridge<unknown>('get_voice_profile');
          initialProfile = mergeVoiceProfileStatus(initialProfile, profileRes, loadedSamples.length);
        } catch {}

        setProfileStatus(initialProfile);
        if (initialProfile.is_enrolled) {
          setCurrentStep(5);
        }
      } catch {
        // Ignore local storage / bridge initialization errors
      }
    };

    initVoiceData();
  }, []);

  // Cleanup Web Audio & Recorders
  const cleanupRecording = useCallback(() => {
    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current);
      animationFrameRef.current = null;
    }
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    if (vuIntervalRef.current) {
      clearInterval(vuIntervalRef.current);
      vuIntervalRef.current = null;
    }
    if (mediaStreamRef.current) {
      mediaStreamRef.current.getTracks().forEach((track) => track.stop());
      mediaStreamRef.current = null;
    }
    if (audioContextRef.current && audioContextRef.current.state !== 'closed') {
      audioContextRef.current.close().catch(() => {});
      audioContextRef.current = null;
    }
    setIsRecording(false);
    setLiveVoiceLevel(0);
  }, []);

  useEffect(() => {
    return () => {
      cleanupRecording();
      if (audioElementRef.current) {
        audioElementRef.current.pause();
      }
    };
  }, [cleanupRecording]);

  // Create synthetic preview beep/tone if no native audio recorded
  const createSyntheticAudioUrl = useCallback((durationSec: number = 5.0): string => {
    try {
      const ctx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
      const sampleRate = ctx.sampleRate;
      const duration = Math.max(MIN_RECORDING_SECONDS, durationSec);
      const totalFrames = Math.floor(sampleRate * duration);
      const buffer = ctx.createBuffer(1, totalFrames, sampleRate);
      const data = buffer.getChannelData(0);
      for (let i = 0; i < totalFrames; i++) {
        // Harmonic voice-like simulation (fundamental ~150Hz with harmonics)
        const tSec = i / sampleRate;
        const envelope = Math.sin((Math.PI * tSec) / duration);
        const wave =
          0.5 * Math.sin(2 * Math.PI * 150 * tSec) +
          0.3 * Math.sin(2 * Math.PI * 300 * tSec) +
          0.2 * Math.sin(2 * Math.PI * 450 * tSec);
        data[i] = wave * envelope * 0.4;
      }
      ctx.close();

      // Encode minimal WAV
      const wavBytes = encodeWav(data, sampleRate);
      const blob = new Blob([wavBytes], { type: 'audio/wav' });
      return URL.createObjectURL(blob);
    } catch {
      return '';
    }
  }, []);

  // Helper WAV encoder
  function encodeWav(samples: Float32Array, sampleRate: number): Uint8Array {
    const buffer = new ArrayBuffer(44 + samples.length * 2);
    const view = new DataView(buffer);
    const writeString = (offset: number, string: string) => {
      for (let i = 0; i < string.length; i++) {
        view.setUint8(offset + i, string.charCodeAt(i));
      }
    };
    writeString(0, 'RIFF');
    view.setUint32(4, 36 + samples.length * 2, true);
    writeString(8, 'WAVE');
    writeString(12, 'fmt ');
    view.setUint32(16, 16, true);
    view.setUint16(20, 1, true); // PCM
    view.setUint16(22, 1, true); // Mono
    view.setUint32(24, sampleRate, true);
    view.setUint32(28, sampleRate * 2, true);
    view.setUint16(32, 2, true);
    view.setUint16(34, 16, true);
    writeString(36, 'data');
    view.setUint32(40, samples.length * 2, true);
    let offset = 44;
    for (let i = 0; i < samples.length; i++, offset += 2) {
      const s = Math.max(-1, Math.min(1, samples[i]));
      view.setInt16(offset, s < 0 ? s * 0x8000 : s * 0x7fff, true);
    }
    return new Uint8Array(buffer);
  }

  // Start Guided Step Recording (up to MAX_RECORDING_SECONDS with manual stop)
  const handleStartStepRecording = async (stepNum: number) => {
    cleanupRecording();
    recordedChunksRef.current = [];
    setIsRecording(true);
    setRecordingElapsedSeconds(0);
    recordingStartTimeRef.current = Date.now();

    let stream: MediaStream | null = null;
    let audioCtx: AudioContext | null = null;

    try {
      stream = await acquireCleanPhysicalStream(selectedInputId, inputDevices);
      mediaStreamRef.current = stream;

      audioCtx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
      audioContextRef.current = audioCtx;
      const source = audioCtx.createMediaStreamSource(stream);
      const analyser = audioCtx.createAnalyser();
      analyser.fftSize = 256;
      source.connect(analyser);
      analyserRef.current = analyser;

      const updateVuMeter = () => {
        if (!analyserRef.current) return;
        const dataArray = new Uint8Array(analyserRef.current.frequencyBinCount);
        analyserRef.current.getByteFrequencyData(dataArray);
        let sum = 0;
        for (let i = 0; i < dataArray.length; i++) {
          sum += dataArray[i];
        }
        const avg = sum / dataArray.length;
        const percent = Math.min(100, Math.round((avg / 128) * 100));
        setLiveVoiceLevel(percent);
        animationFrameRef.current = requestAnimationFrame(updateVuMeter);
      };
      updateVuMeter();

      const preferredMime = getPreferredAudioMimeType();
      const recorderOptions = preferredMime ? { mimeType: preferredMime } : undefined;
      const recorder = new MediaRecorder(stream, recorderOptions);
      mediaRecorderRef.current = recorder;
      recorder.ondataavailable = (e) => {
        if (e.data && e.data.size > 0) {
          recordedChunksRef.current.push(e.data);
        }
      };
      recorder.start(100);
    } catch (err) {
      console.warn('Could not acquire physical microphone, using simulated VU meter fallback:', err);
      // Fallback simulated VU meter for environments without mic permissions
      let simLevel = 35;
      const simInterval = setInterval(() => {
        simLevel = Math.max(15, Math.min(95, simLevel + (Math.random() * 30 - 15)));
        setLiveVoiceLevel(Math.round(simLevel));
      }, 100);
      vuIntervalRef.current = simInterval;
    }

    // Elapsed timer up to MAX_RECORDING_SECONDS
    const timer = setInterval(() => {
      const elapsed = (Date.now() - recordingStartTimeRef.current) / 1000;
      const rounded = Math.round(elapsed * 10) / 10;
      setRecordingElapsedSeconds(rounded);
      if (elapsed >= MAX_RECORDING_SECONDS) {
        clearInterval(timer);
        finishStepRecording(stepNum);
      }
    }, 100);
    timerRef.current = timer;
  };

  const finishStepRecording = async (stepNum: number) => {
    const rawElapsed = (Date.now() - recordingStartTimeRef.current) / 1000;
    const exactDuration = Math.min(
      MAX_RECORDING_SECONDS,
      Math.max(MIN_RECORDING_SECONDS, Math.round(rawElapsed * 10) / 10)
    );

    let finalUrl = '';
    const blob = await stopMediaRecorderAsync(mediaRecorderRef.current, recordedChunksRef.current);
    if (blob && blob.size > 0) {
      finalUrl = URL.createObjectURL(blob);
    }
    if (!finalUrl) {
      finalUrl = createSyntheticAudioUrl(exactDuration);
    }

    cleanupRecording();

    setCompletedSteps((prev) => ({
      ...prev,
      [stepNum]: { audioUrl: finalUrl, duration: exactDuration },
    }));

    setFeedbackMessage(t('voiceProfile.sampleCompleted'));
    setTimeout(() => setFeedbackMessage(null), 3500);
  };

  // Advance to next step
  const handleNextStep = () => {
    if (currentStep < 5) {
      setCurrentStep(currentStep + 1);
    }
  };

  // Re-record step
  const handleRedoStep = (stepNum: number) => {
    setCompletedSteps((prev) => {
      const copy = { ...prev };
      delete copy[stepNum];
      return copy;
    });
    handleStartStepRecording(stepNum);
  };

  // Toggle mode: Open questions vs Neutral reading
  const handleToggleReadingMode = () => {
    const nextVal = !isReadingMode;
    setIsReadingMode(nextVal);
    try {
      localStorage.setItem(STORAGE_READING_MODE_KEY, String(nextVal));
    } catch {}
  };

  // Activate Profile (Once 5/5 are complete)
  const handleActivateProfile = async () => {
    const newSamples: VoiceSample[] = stepQuestions.map((q, idx) => ({
      id: `sample-init-${idx + 1}-${Date.now()}`,
      title: `${t(q.categoryKey)}`,
      category: t(q.categoryKey),
      timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      durationSec: completedSteps[idx + 1]?.duration || 5.0,
      audioUrl: completedSteps[idx + 1]?.audioUrl,
      isInitialStep: true,
    }));

    setSamples(newSamples);
    try {
      localStorage.setItem(STORAGE_SAMPLES_KEY, JSON.stringify(newSamples));
      localStorage.setItem(STORAGE_ENROLLED_KEY, 'true');
    } catch {}

    const newStatus: VoiceProfileStatus = {
      is_enrolled: true,
      active_samples_count: newSamples.length,
      embedding_dim: 192,
      neural_eq_calibrated: true,
      gain_boost_db: 1.8,
    };
    setProfileStatus((prev) => applySetVoiceProfileResult(prev, newStatus, undefined));

    const setRes = await invokeBridge<unknown>('set_voice_profile', { profile: stripServiceVoiceProfileKeys(newStatus) }).catch(() => undefined);
    setProfileStatus((prev) => applySetVoiceProfileResult(prev, newStatus, setRes));
    for (const sample of newSamples) {
      await invokeBridge('add_voice_sample', { sample }).catch(() => {});
    }
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new CustomEvent('clearcore_profile_updated', { detail: newStatus }));
    }
    setFeedbackMessage(t('voiceProfile.profileActivatedSuccess'));
    setTimeout(() => setFeedbackMessage(null), 5000);
  };

  // Full Re-enrollment reset
  const handleResetEnrollment = () => {
    setCompletedSteps({});
    setCurrentStep(1);
  };

  // Playback handling
  const handlePlayAudio = (id: string, audioUrl?: string) => {
    if (playingAudioId === id) {
      if (audioElementRef.current) {
        audioElementRef.current.pause();
      }
      setPlayingAudioId(null);
      return;
    }

    if (audioElementRef.current) {
      audioElementRef.current.pause();
    }

    const urlToPlay = audioUrl || createSyntheticAudioUrl();
    const audio = new Audio(urlToPlay);
    audioElementRef.current = audio;
    setPlayingAudioId(id);

    audio.onended = () => {
      setPlayingAudioId(null);
    };
    audio.onerror = () => {
      setPlayingAudioId(null);
    };
    audio.play().catch(() => {
      setPlayingAudioId(null);
    });
  };

  // Sends the local profile (never service keys) and applies the fresh service status from the reply.
  const pushProfileStatus = (local: VoiceProfileStatus) => {
    invokeBridge<unknown>('set_voice_profile', { profile: stripServiceVoiceProfileKeys(local) })
      .catch(() => undefined)
      .then((res) => setProfileStatus((prev) => applySetVoiceProfileResult(prev, local, res)));
  };

  // Main-process pushes (same merged payload as the set_voice_profile reply).
  useEffect(() => {
    const api = typeof window !== 'undefined' ? window.clearcoreApi : undefined;
    if (!api?.onVoiceProfileUpdate) return;
    return api.onVoiceProfileUpdate((profile) => {
      setProfileStatus((prev) => applySetVoiceProfileResult(prev, prev, profile));
    });
  }, []);

  // Delete Sample from Cumulative Gallery
  const handleDeleteSample = (id: string) => {
    const updated = samples.filter((s) => s.id !== id);
    setSamples(updated);
    try {
      localStorage.setItem(STORAGE_SAMPLES_KEY, JSON.stringify(updated));
      localStorage.setItem(STORAGE_ENROLLED_KEY, String(updated.length > 0));
    } catch {}

    const newStatus: VoiceProfileStatus = {
      ...stripServiceVoiceProfileKeys(profileStatus),
      is_enrolled: updated.length > 0,
      active_samples_count: updated.length,
      neural_eq_calibrated: updated.length > 0,
    };
    setProfileStatus((prev) => applySetVoiceProfileResult(prev, newStatus, undefined));
    invokeBridge('delete_voice_sample', { id }).catch(() => {});
    pushProfileStatus(newStatus);
  };

  // Approve Call Suggestion Take (Voice Intake Engine)
  const handleApproveCallTake = (take: CallSuggestionTake) => {
    // Add to cumulative samples gallery
    const newSample: VoiceSample = {
      id: `take-approved-${take.id}-${Date.now()}`,
      title: take.title,
      category: 'Chamada Real (Intake)',
      timestamp: take.timestamp,
      durationSec: take.durationSec,
      audioUrl: take.audioUrl,
    };
    const updatedSamples = [newSample, ...samples];
    setSamples(updatedSamples);

    // Remove from suggestions
    const updatedTakes = callTakes.filter((tItem) => tItem.id !== take.id);
    setCallTakes(updatedTakes);

    try {
      localStorage.setItem(STORAGE_SAMPLES_KEY, JSON.stringify(updatedSamples));
      localStorage.setItem(STORAGE_CALL_TAKES_KEY, JSON.stringify(updatedTakes));
      localStorage.setItem(STORAGE_ENROLLED_KEY, 'true');
    } catch {}

    const newStatus: VoiceProfileStatus = {
      ...stripServiceVoiceProfileKeys(profileStatus),
      is_enrolled: true,
      active_samples_count: updatedSamples.length,
      neural_eq_calibrated: true,
    };
    setProfileStatus((prev) => applySetVoiceProfileResult(prev, newStatus, undefined));
    invokeBridge('approve_call_take', { id: take.id, name: take.title, take }).catch(() => {});
    pushProfileStatus(newStatus);

    setFeedbackMessage(t('voiceProfile.takeApprovedFeedback'));
    setTimeout(() => setFeedbackMessage(null), 4000);
  };

  // Dismiss Call Suggestion Take
  const handleDismissCallTake = (id: string) => {
    const updatedTakes = callTakes.filter((tItem) => tItem.id !== id);
    setCallTakes(updatedTakes);
    try {
      localStorage.setItem(STORAGE_CALL_TAKES_KEY, JSON.stringify(updatedTakes));
    } catch {}
    invokeBridge('dismiss_call_take', { id }).catch(() => {});
    setFeedbackMessage(t('voiceProfile.takeDismissedFeedback'));
    setTimeout(() => setFeedbackMessage(null), 3000);
  };

  const finishModalRecording = async () => {
    const rawElapsed = (Date.now() - recordingStartTimeRef.current) / 1000;
    const exactDuration = Math.min(
      MAX_RECORDING_SECONDS,
      Math.max(MIN_RECORDING_SECONDS, Math.round(rawElapsed * 10) / 10)
    );

    let finalUrl = '';
    const blob = await stopMediaRecorderAsync(mediaRecorderRef.current, recordedChunksRef.current);
    if (blob && blob.size > 0) {
      finalUrl = URL.createObjectURL(blob);
    }
    if (!finalUrl) {
      finalUrl = createSyntheticAudioUrl(exactDuration);
    }

    cleanupRecording();
    setModalRecordedUrl(finalUrl);
    setModalDuration(exactDuration);
  };

  // Modal: Start Recording voluntary sample
  const handleStartModalRecording = async () => {
    cleanupRecording();
    recordedChunksRef.current = [];
    setIsRecording(true);
    setRecordingElapsedSeconds(0);
    recordingStartTimeRef.current = Date.now();

    try {
      const stream = await acquireCleanPhysicalStream(selectedInputId, inputDevices);
      mediaStreamRef.current = stream;

      const audioCtx = new (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext)();
      audioContextRef.current = audioCtx;
      const source = audioCtx.createMediaStreamSource(stream);
      const analyser = audioCtx.createAnalyser();
      analyser.fftSize = 256;
      source.connect(analyser);
      analyserRef.current = analyser;

      const updateVuMeter = () => {
        if (!analyserRef.current) return;
        const dataArray = new Uint8Array(analyserRef.current.frequencyBinCount);
        analyserRef.current.getByteFrequencyData(dataArray);
        let sum = 0;
        for (let i = 0; i < dataArray.length; i++) sum += dataArray[i];
        const avg = sum / dataArray.length;
        setLiveVoiceLevel(Math.min(100, Math.round((avg / 128) * 100)));
        animationFrameRef.current = requestAnimationFrame(updateVuMeter);
      };
      updateVuMeter();

      const preferredMime = getPreferredAudioMimeType();
      const recorderOptions = preferredMime ? { mimeType: preferredMime } : undefined;
      const recorder = new MediaRecorder(stream, recorderOptions);
      mediaRecorderRef.current = recorder;
      recorder.ondataavailable = (e) => {
        if (e.data && e.data.size > 0) recordedChunksRef.current.push(e.data);
      };
      recorder.start(100);
    } catch (err) {
      console.warn('Could not acquire physical microphone for modal recording, using simulated fallback:', err);
      let simLevel = 35;
      const simInterval = setInterval(() => {
        simLevel = Math.max(15, Math.min(95, simLevel + (Math.random() * 30 - 15)));
        setLiveVoiceLevel(Math.round(simLevel));
      }, 100);
      vuIntervalRef.current = simInterval;
    }

    const timer = setInterval(() => {
      const elapsed = (Date.now() - recordingStartTimeRef.current) / 1000;
      const rounded = Math.round(elapsed * 10) / 10;
      setRecordingElapsedSeconds(rounded);
      if (elapsed >= MAX_RECORDING_SECONDS) {
        clearInterval(timer);
        finishModalRecording();
      }
    }, 100);
    timerRef.current = timer;
  };

  // Modal: Save Voluntary Sample to Gallery
  const handleSaveModalSample = () => {
    const sampleTitle = modalSampleName.trim() || `Amostra Adicional #${samples.length + 1}`;
    const newSample: VoiceSample = {
      id: `sample-vol-${Date.now()}`,
      title: sampleTitle,
      category: 'Adição Voluntária',
      timestamp: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
      durationSec: modalDuration,
      audioUrl: modalRecordedUrl || undefined,
    };

    const updated = [newSample, ...samples];
    setSamples(updated);
    try {
      localStorage.setItem(STORAGE_SAMPLES_KEY, JSON.stringify(updated));
      localStorage.setItem(STORAGE_ENROLLED_KEY, 'true');
    } catch {}

    const newStatus: VoiceProfileStatus = {
      ...stripServiceVoiceProfileKeys(profileStatus),
      is_enrolled: true,
      active_samples_count: updated.length,
      neural_eq_calibrated: true,
    };
    setProfileStatus((prev) => applySetVoiceProfileResult(prev, newStatus, undefined));
    invokeBridge('add_voice_sample', { sample: newSample }).catch(() => {});
    pushProfileStatus(newStatus);

    setIsModalOpen(false);
    setModalSampleName('');
    setModalRecordedUrl(null);
    setFeedbackMessage(t('voiceProfile.sampleCompleted'));
    setTimeout(() => setFeedbackMessage(null), 3500);
  };

  const completedCount = Object.keys(completedSteps).length;
  const isAllStepsCompleted = completedCount >= 5;
  const currentQ = stepQuestions[currentStep - 1];


  const statusLabelKey = voiceProfileStatusLabelKey(profileStatus);
  const statusLabelActive = statusLabelKey === 'active';
  const statusLabel =
    statusLabelKey === 'active'
      ? t('voiceProfile.statusActive')
      : statusLabelKey === 'storedNotApplied'
        ? t('voiceProfile.storedNotApplied')
        : statusLabelKey === 'enrolledUnconfirmed'
          ? t('voiceProfile.enrolledUnconfirmed')
          : t('voiceProfile.statusPending');

  return (
    <div className="card voice-profile-card">
      {/* Header & Status Section */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 12, marginBottom: 16 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <h2 className="card-title" style={{ margin: 0 }}>{t('voiceProfile.title')}</h2>
            <span className="profile-badge-ecapa">
              {t('voiceProfile.badge')}
            </span>
            <span className={`status-pill ${statusLabelActive ? 'pill-active' : 'pill-pending'}`}>
              {statusLabel}
            </span>
          </div>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 6, lineHeight: 1.45 }}>
            {t('voiceProfile.description')}
          </p>
        </div>

        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {profileStatus.is_enrolled && (
            <button
              className="action-btn"
              onClick={handleResetEnrollment}
              style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          )}
        </div>
      </div>

      {feedbackMessage && (
        <div className="feedback-banner success-banner">
          {feedbackMessage}
        </div>
      )}

      {/* Profile Overview Bar */}
      <div className="profile-overview-box">
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.statusTitle')}</div>
          <div className="overview-value" style={{ color: statusLabelActive ? '#4ade80' : '#fbbf24' }}>
            {statusLabel}
          </div>
          {statusLabelKey === 'active' && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.75rem', marginTop: 4 }}>
              {t('voiceProfile.appliedInServiceNote')}
            </div>
          )}
          {profileStatus.voice_profile_error && statusLabelKey !== 'active' && (
            <div style={{ color: '#fbbf24', fontSize: '0.75rem', marginTop: 4 }}>
              {t(`voiceProfile.${voiceProfileErrorKey(profileStatus.voice_profile_error)}`)}
            </div>
          )}
        </div>
        <div className="overview-metric">
          <div className="overview-label">Amostras Registradas</div>
          <div className="overview-value">
            {profileStatus.active_samples_count} {t('voiceProfile.statusSamplesPill', { count: String(profileStatus.active_samples_count) })}
          </div>
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.neuralEqStatusTitle')}</div>
          <div className="overview-value" style={{ color: profileStatus.neural_eq_calibrated ? '#4ade80' : 'var(--text-muted)' }}>
            {profileStatus.neural_eq_calibrated ? t('voiceProfile.neuralEqCalibrated') : t('voiceProfile.neuralEqPending')}
          </div>
        </div>
      </div>

      {/* Guided Multi-Sampling Stepper (If not enrolled or re-enrolling) */}
      {(!profileStatus.is_enrolled || completedCount < 5) && (
        <div className="guided-sampling-section">
          {/* Progress Header */}
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10, flexWrap: 'wrap', gap: 8 }}>
            <div style={{ fontWeight: 600, fontSize: '0.95rem', color: '#93c5fd' }}>
              {t('voiceProfile.stepProgress', { current: String(currentStep), total: '5' })}: {t(currentQ.categoryKey)}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {t('voiceProfile.stepPercent', { percent: String(Math.round((completedCount / 5) * 100)) })}
              </span>
              <button
                className="mode-toggle-link"
                onClick={handleToggleReadingMode}
                title={isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              >
                {isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              </button>
            </div>
          </div>

          {/* Stepper Progress Bar (1/5 to 5/5) */}
          <div className="stepper-bar-container">
            {[1, 2, 3, 4, 5].map((stepIdx) => {
              const isDone = Boolean(completedSteps[stepIdx]);
              const isCur = currentStep === stepIdx;
              return (
                <div
                  key={stepIdx}
                  className={`stepper-segment ${isDone ? 'done' : isCur ? 'current' : 'pending'}`}
                  onClick={() => setCurrentStep(stepIdx)}
                  title={`Etapa ${stepIdx}`}
                >
                  <div className="segment-number">{isDone ? '✓' : stepIdx}</div>
                  <div className="segment-fill" />
                </div>
              );
            })}
          </div>

          {/* Prompt Box */}
          <div className="prompt-display-card">
            <div className="prompt-mode-tag">
              {isReadingMode ? `📖 ${t('voiceProfile.readingMode')}` : `💬 ${t('voiceProfile.openQuestionsMode')}`}
            </div>
            <div className="prompt-main-text">
              {isReadingMode ? `"${t(currentQ.fallbackKey)}"` : `"${t(currentQ.textKey)}"`}
            </div>
            <div className="prompt-hint-sub">
              {isReadingMode
                ? 'Leia a frase em voz alta com seu ritmo natural de fala.'
                : 'Responda espontaneamente, sem pensar muito, exatamente como falaria com um colega em uma chamada.'}
            </div>
          </div>

          {/* Live Dynamic VU Meter */}
          <div className="vu-meter-panel">
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
              <span>{t('voiceProfile.voiceLevel')}</span>
              <span>{isRecording ? `${liveVoiceLevel}%` : '0%'}</span>
            </div>
            <div className="vu-meter-track">
              <div
                className="vu-meter-bar"
                style={{
                  width: `${isRecording ? liveVoiceLevel : 0}%`,
                  background:
                    liveVoiceLevel > 80
                      ? 'linear-gradient(90deg, #22c55e, #eab308, #ef4444)'
                      : liveVoiceLevel > 40
                      ? 'linear-gradient(90deg, #22c55e, #38bdf8)'
                      : '#22c55e',
                }}
              />
            </div>
          </div>

          {/* Action Row for the Step */}
          <div className="stepper-action-row">
            {!isRecording ? (
              completedSteps[currentStep] ? (
                <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
                  <button
                    className="action-btn play-sample-btn"
                    onClick={() => handlePlayAudio(`step-${currentStep}`, completedSteps[currentStep].audioUrl)}
                  >
                    {playingAudioId === `step-${currentStep}` ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                  </button>
                  <button
                    className="action-btn"
                    onClick={() => handleRedoStep(currentStep)}
                  >
                    {t('voiceProfile.redoSample')}
                  </button>
                  {currentStep < 5 && (
                    <button
                      className="action-btn primary-next-btn"
                      onClick={handleNextStep}
                    >
                      {t('voiceProfile.nextStep')}
                    </button>
                  )}
                </div>
              ) : (
                <button
                  className="record-btn-trigger"
                  onClick={() => handleStartStepRecording(currentStep)}
                >
                  🎙️ {t('voiceProfile.recordSample')}
                </button>
              )
            ) : (
              <div className="recording-active-container">
                <div className="recording-status-group">
                  <span className="recording-pulsing-dot" />
                  <span style={{ fontWeight: 600, color: '#f87171' }}>
                    {t('voiceProfile.recordingStatus', {
                      elapsed: recordingElapsedSeconds.toFixed(1),
                      max: String(MAX_RECORDING_SECONDS),
                    })}
                  </span>
                  <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    ({t('voiceProfile.recordingHint')})
                  </span>
                </div>
                <button
                  type="button"
                  className="stop-record-btn"
                  disabled={recordingElapsedSeconds < MIN_RECORDING_SECONDS}
                  onClick={() => finishStepRecording(currentStep)}
                  title={
                    recordingElapsedSeconds < MIN_RECORDING_SECONDS
                      ? `Mínimo de ${MIN_RECORDING_SECONDS}s`
                      : t('voiceProfile.stopRecordingBtn')
                  }
                >
                  ⏹️ {t('voiceProfile.stopRecordingBtn')}
                </button>
              </div>
            )}

            {isAllStepsCompleted && (
              <button
                className="activate-profile-master-btn"
                onClick={handleActivateProfile}
              >
                {t('voiceProfile.activateProfileBtn')}
              </button>
            )}
          </div>
        </div>
      )}

      {/* Tabs Header: Cumulative Gallery vs Call Suggestions */}
      <div className="profile-subtabs-nav">
        <button
          className={`subtab-btn ${activeTab === 'samples' ? 'subtab-active' : ''}`}
          onClick={() => setActiveTab('samples')}
        >
          {t('voiceProfile.tabSamples', { count: String(samples.length) })}
        </button>
        <button
          className={`subtab-btn ${activeTab === 'intake' ? 'subtab-active' : ''}`}
          onClick={() => setActiveTab('intake')}
        >
          {t('voiceProfile.tabCallSuggestions', { count: String(callTakes.length) })}
          {callTakes.length > 0 && <span className="tab-count-badge">{callTakes.length}</span>}
        </button>
      </div>

      {/* Tab 1: Cumulative Samples Gallery */}
      {activeTab === 'samples' && (
        <div className="tab-pane-content">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0 }}>
              {t('voiceProfile.sampleGalleryDesc')}
            </p>
            <button
              className="action-btn add-sample-accent-btn"
              onClick={() => setIsModalOpen(true)}
            >
              {t('voiceProfile.addNewSampleBtn')}
            </button>
          </div>

          {samples.length > 0 ? (
            <div className="samples-list-grid">
              {samples.map((s, idx) => (
                <div key={s.id} className="sample-card-item">
                  <div className="sample-card-header">
                    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                      <span className="sample-mic-icon">🎙️</span>
                      <div>
                        <div className="sample-card-title">{s.title || `Amostra #${idx + 1}`}</div>
                        <div className="sample-card-meta">
                          {s.timestamp} • {s.durationSec.toFixed(1)}s {s.category ? `• ${s.category}` : ''}
                        </div>
                      </div>
                    </div>
                    <button
                      className="delete-sample-icon-btn"
                      onClick={() => handleDeleteSample(s.id)}
                      title={t('voiceProfile.deleteSample')}
                    >
                      🗑
                    </button>
                  </div>

                  <div className="sample-card-actions">
                    <button
                      className={`action-btn sample-play-toggle-btn ${playingAudioId === s.id ? 'btn-playing' : ''}`}
                      onClick={() => handlePlayAudio(s.id, s.audioUrl)}
                    >
                      {playingAudioId === s.id ? `⏹ ${t('voiceProfile.stopSample')}` : `▶ ${t('voiceProfile.playSample')}`}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state-card">
              {t('voiceProfile.emptyGallery')}
            </div>
          )}
        </div>
      )}

      {/* Tab 2: Call Suggestions (Voice Intake Engine) */}
      {activeTab === 'intake' && (
        <div className="tab-pane-content">
          <div style={{ marginBottom: 14 }}>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0, lineHeight: 1.45 }}>
              {t('voiceProfile.callSuggestionsDesc')}
            </p>
          </div>

          {callTakes.length > 0 ? (
            <div className="intake-takes-list">
              {callTakes.map((take) => (
                <div key={take.id} className="intake-take-card">
                  <div className="intake-take-info">
                    <div className="take-title-row">
                      <span className="take-badge-live">Take Reunião</span>
                      <strong style={{ fontSize: '0.95rem' }}>{take.title}</strong>
                    </div>
                    <div className="take-meta-row">
                      <span>🕒 {take.timestamp}</span>
                      <span>⏱ {t('voiceProfile.takeDuration', { sec: take.durationSec.toFixed(1) })}</span>
                      <span className="take-snr-badge">🟢 {t('voiceProfile.takeSnr', { snr: take.snrDb.toFixed(1) })}</span>
                    </div>
                  </div>

                  <div className="intake-take-actions">
                    <button
                      className="action-btn take-play-btn"
                      onClick={() => handlePlayAudio(take.id, take.audioUrl)}
                    >
                      {playingAudioId === take.id ? `⏹ ${t('voiceProfile.stopSample')}` : `▶ ${t('voiceProfile.playSample')}`}
                    </button>
                    <button
                      className="action-btn take-approve-btn"
                      onClick={() => handleApproveCallTake(take)}
                    >
                      {t('voiceProfile.approveTake')}
                    </button>
                    <button
                      className="action-btn take-dismiss-btn"
                      onClick={() => handleDismissCallTake(take.id)}
                    >
                      {t('voiceProfile.dismissTake')}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state-card">
              {t('voiceProfile.emptyCallSuggestions')}
            </div>
          )}
        </div>
      )}

      {/* Modal: "+ Adicionar Nova Amostra" */}
      {isModalOpen && (
        <div className="modal-overlay">
          <div className="modal-content profile-add-modal">
            <h3 style={{ fontSize: '1.2rem', marginBottom: 8, color: 'var(--text-main)' }}>
              {t('voiceProfile.modalAddSampleTitle')}
            </h3>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginBottom: 16 }}>
              {t('voiceProfile.modalAddSampleDesc')}
            </p>

            <div style={{ marginBottom: 14 }}>
              <label style={{ display: 'block', fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 6 }}>
                Identificação da Amostra (Opcional):
              </label>
              <input
                type="text"
                className="device-select"
                placeholder={t('voiceProfile.sampleNamePlaceholder')}
                value={modalSampleName}
                onChange={(e) => setModalSampleName(e.target.value)}
              />
            </div>

            {/* VU Meter inside modal */}
            <div className="vu-meter-panel" style={{ marginBottom: 16 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
                <span>{t('voiceProfile.voiceLevel')}</span>
                <span>{isRecording ? `${liveVoiceLevel}%` : '0%'}</span>
              </div>
              <div className="vu-meter-track">
                <div
                  className="vu-meter-bar"
                  style={{ width: `${isRecording ? liveVoiceLevel : 0}%`, background: '#22c55e' }}
                />
              </div>
            </div>

            <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 16 }}>
              {!isRecording ? (
                <button
                  className="record-btn-trigger"
                  onClick={handleStartModalRecording}
                >
                  🎙️ {modalRecordedUrl ? t('voiceProfile.redoSample') : t('voiceProfile.recordSample')}
                </button>
              ) : (
                <div className="recording-active-container">
                  <div className="recording-status-group">
                    <span className="recording-pulsing-dot" />
                    <span style={{ fontWeight: 600, color: '#f87171' }}>
                      {t('voiceProfile.recordingStatus', {
                        elapsed: recordingElapsedSeconds.toFixed(1),
                        max: String(MAX_RECORDING_SECONDS),
                      })}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="stop-record-btn"
                    disabled={recordingElapsedSeconds < MIN_RECORDING_SECONDS}
                    onClick={finishModalRecording}
                    title={
                      recordingElapsedSeconds < MIN_RECORDING_SECONDS
                        ? `Mínimo de ${MIN_RECORDING_SECONDS}s`
                        : t('voiceProfile.stopRecordingBtn')
                    }
                  >
                    ⏹️ {t('voiceProfile.stopRecordingBtn')}
                  </button>
                </div>
              )}
            </div>

            {modalRecordedUrl && !isRecording && (
              <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 10, marginBottom: 16 }}>
                <button
                  className="action-btn"
                  onClick={() => handlePlayAudio('modal-preview', modalRecordedUrl)}
                >
                  {playingAudioId === 'modal-preview' ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                </button>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                  ({modalDuration.toFixed(1)}s)
                </span>
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 12 }}>
              <button
                className="action-btn"
                onClick={() => {
                  cleanupRecording();
                  setIsModalOpen(false);
                  setModalRecordedUrl(null);
                }}
              >
                {t('voiceProfile.modalCancel')}
              </button>
              <button
                className="action-btn primary-next-btn"
                disabled={!modalRecordedUrl}
                onClick={handleSaveModalSample}
              >
                {t('voiceProfile.modalSave')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
