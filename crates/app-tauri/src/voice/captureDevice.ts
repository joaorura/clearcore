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

/** The user denied microphone permission (distinguishable so the UI can say so). */
export class MicPermissionDeniedError extends PhysicalMicUnavailableError {
  constructor(message = 'Microphone permission was denied') {
    super(message);
    this.name = 'MicPermissionDeniedError';
  }
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

  const known = selectedInputId ? inputDevices?.find((d) => d.id === selectedInputId) : undefined;
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

const stopAll = (stream: MediaStream | null | undefined) =>
  stream?.getTracks().forEach((t) => {
    try { t.stop(); } catch { /* already stopped */ }
  });

/**
 * Open the raw physical microphone: all browser processing off, mono, 48 kHz ideal.
 * Throws PhysicalMicUnavailableError instead of ever falling back to an unconstrained getUserMedia.
 * Any failure after a stream was opened stops its tracks before rethrowing.
 *
 * Two controlled exceptions to D1 (both close the device again before anything is recorded):
 *  - Label probe: browsers hide device labels until a first grant, so when every enumerated label is
 *    empty one getUserMedia({audio:true}) is issued. It may open the system default (possibly the
 *    virtual mic); its tracks are stopped in the same tick and its audio is never read. Skipped when
 *    labels are already available.
 *  - Alias device: if the only physical candidate is the 'default'/'communications' alias (and no
 *    non-alias with the same group exists), the alias is opened with an exact deviceId and the
 *    resulting track label ('Default - <name>') is validated AFTER opening; a virtual/loopback
 *    label stops the track and raises PhysicalMicUnavailableError.
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
    let probe: MediaStream;
    try {
      probe = await md.getUserMedia({ audio: true });
    } catch (err) {
      const name = (err as { name?: string })?.name;
      const message = err instanceof Error ? err.message : String(err);
      throw name === 'NotAllowedError' || name === 'SecurityError'
        ? new MicPermissionDeniedError(message)
        : new PhysicalMicUnavailableError(message);
    }
    stopAll(probe);
    devs = await md.enumerateDevices();
  }

  const audioInputs = devs.filter((d) => d.kind === 'audioinput');
  let physical = resolvePhysicalAudioDevice(audioInputs, selectedInputId, inputDevices);
  if (!physical || !physical.deviceId) throw new PhysicalMicUnavailableError();
  if (isAlias(physical.deviceId)) {
    const alias = physical;
    const sameGroup = audioInputs.find(
      (d) => !isAlias(d.deviceId) && d.groupId === alias.groupId && !isVirtualOrLoopbackAudioDevice(d.label),
    );
    if (sameGroup) physical = sameGroup;
  }
  const viaAlias = isAlias(physical.deviceId);

  // Computed before opening so a failure here cannot leave an open stream behind.
  const cleanPreLabel = (physical.label || '').trim().toLowerCase();
  const preHash = await sha256Hex(cleanPreLabel.length > 0 ? `mic:${cleanPreLabel}` : `${physical.deviceId}|${physical.groupId}`);

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

  try {
    const track = stream.getTracks()[0] as MediaStreamTrack | undefined;
    let label = physical.label || '';
    if (viaAlias) {
      const settings = (track?.getSettings?.() ?? {}) as { label?: string };
      const labels = [track?.label ?? '', settings.label ?? '', label];
      if (labels.some((l) => isVirtualOrLoopbackAudioDevice(l))) {
        throw new PhysicalMicUnavailableError('Default device resolves to a virtual or loopback source');
      }
      label = track?.label || label;
    } else if (!label && track?.label) {
      label = track.label;
    }
    const cleanLabel = (label || physical.label || '').trim().toLowerCase();
    // A stable hash of the physical device identifier: the normalized OS hardware label is persistent
    // across Chromium reloads and sessions, unlike ephemeral WebRTC deviceId/groupId.
    const idHash = cleanLabel.length > 0 && cleanLabel !== cleanPreLabel
      ? await sha256Hex(`mic:${cleanLabel}`)
      : preHash;
    return { stream, device: { label: label.slice(0, 128), idHash } };
  } catch (err) {
    stopAll(stream);
    throw err;
  }
}
