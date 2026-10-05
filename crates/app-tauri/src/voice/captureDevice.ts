import type { DeviceInfo } from './enrollmentTypes';
import { CAPTURE_SAMPLE_RATE } from './enrollmentTypes';

/** Thrown when no physical microphone could be opened. There is deliberately no fallback (spec D1). */
export class PhysicalMicUnavailableError extends Error {
  constructor(message = 'No physical microphone could be opened') {
    super(message);
    this.name = 'PhysicalMicUnavailableError';
  }
}

/** Virtual microphone, monitor sink or loopback device (never used for enrollment). */
export function isVirtualOrLoopbackAudioDevice(label: string): boolean {
  const l = (label || '').toLowerCase();
  return (
    l.includes('realtime') ||
    l.includes('clearcore') ||
    l.includes('virtual') ||
    l.includes('monitor') ||
    l.includes('loopback')
  );
}

const isAlias = (id: string) => id === 'default' || id === 'communications';

/** Resolve the physical microphone from enumerated devices, filtering virtual/loopback ones. */
export function resolvePhysicalAudioDevice(
  audioInputs: MediaDeviceInfo[],
  selectedInputId?: string,
  inputDevices?: Array<{ id?: string; name?: string }>,
): MediaDeviceInfo | undefined {
  const candidates = audioInputs.filter((d) => !isVirtualOrLoopbackAudioDevice(d.label));

  if (selectedInputId && !isAlias(selectedInputId)) {
    const direct = candidates.find((d) => d.deviceId === selectedInputId);
    if (direct) return direct;
  }

  const known = inputDevices?.find((d) => d.id === selectedInputId);
  if (known?.name) {
    const name = known.name.toLowerCase().trim();
    const byName = candidates.find((d) => {
      const label = d.label.toLowerCase().trim();
      return label.length > 0 && (label.includes(name) || name.includes(label));
    });
    if (byName) return byName;
  }

  const specific = candidates.find((d) => !isAlias(d.deviceId) && d.label.length > 0);
  if (specific) return specific;
  const nonDefault = candidates.find((d) => !isAlias(d.deviceId));
  if (nonDefault) return nonDefault;
  return candidates[0];
}

export async function sha256Hex(text: string): Promise<string> {
  const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(text));
  return [...new Uint8Array(digest)].map((b) => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Open the raw physical microphone: all browser processing off, mono, 48 kHz ideal.
 * Throws PhysicalMicUnavailableError instead of ever falling back to an unconstrained getUserMedia.
 */
export async function acquireRawPhysicalStream(
  selectedInputId?: string,
  inputDevices?: Array<{ id?: string; name?: string }>,
): Promise<{ stream: MediaStream; device: DeviceInfo }> {
  const md = typeof navigator === 'undefined' ? undefined : navigator.mediaDevices;
  if (!md?.getUserMedia || !md.enumerateDevices) {
    throw new PhysicalMicUnavailableError('navigator.mediaDevices is unavailable');
  }

  let devs: MediaDeviceInfo[] = await md.enumerateDevices();
  if (devs.length > 0 && devs.every((d) => !d.label)) {
    // Label-only probe (labels are hidden before the first grant); tracks are stopped at once.
    try {
      const probe = await md.getUserMedia({ audio: true });
      probe.getTracks().forEach((t) => t.stop());
      devs = await md.enumerateDevices();
    } catch {
      // permission denied: handled below as "unavailable"
    }
  }

  const physical = resolvePhysicalAudioDevice(
    devs.filter((d) => d.kind === 'audioinput'),
    selectedInputId,
    inputDevices,
  );
  if (!physical || !physical.deviceId || isAlias(physical.deviceId)) {
    throw new PhysicalMicUnavailableError();
  }

  let stream: MediaStream;
  try {
    stream = await md.getUserMedia({
      audio: {
        deviceId: { exact: physical.deviceId },
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
        channelCount: 1,
        sampleRate: { ideal: CAPTURE_SAMPLE_RATE },
      },
      video: false,
    });
  } catch (err) {
    throw new PhysicalMicUnavailableError(err instanceof Error ? err.message : String(err));
  }

  return {
    stream,
    device: {
      label: (physical.label || '').slice(0, 128),
      idHash: await sha256Hex(`${physical.deviceId}|${physical.groupId}`),
    },
  };
}
