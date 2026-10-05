import type { CapturedPcm } from './enrollmentTypes';

/**
 * Spec §4.4 / D7: a sample from a different microphone switches the device group, and the earlier
 * samples stop being used. Ask only when the service reported a current group AND the new take's
 * device differs; an older service that sends no group is never asked about.
 */
export function normalizeDeviceLabel(label: string | null | undefined): string {
  if (!label) return '';
  return label
    .trim()
    .toLowerCase()
    .replace(/^(default|padrão)\s*[-:]\s*/i, '')
    .trim();
}

export function needsDeviceSwitchConfirm(
  currentGroupHash: string | null | undefined,
  newDeviceIdHash: string,
  currentGroupLabel?: string | null,
  newDeviceLabel?: string | null,
): boolean {
  if (!currentGroupHash || currentGroupHash.length === 0 || currentGroupHash === newDeviceIdHash) {
    return false;
  }
  const normCurrent = normalizeDeviceLabel(currentGroupLabel);
  const normNew = normalizeDeviceLabel(newDeviceLabel);
  if (
    normCurrent.length > 0 &&
    normNew.length > 0 &&
    (normCurrent === normNew || normCurrent.includes(normNew) || normNew.includes(normCurrent))
  ) {
    return false;
  }
  return true;
}

/**
 * Resolves true when the take may be sent. On a device switch it asks `confirm`; if the user
 * cancels, nothing is sent and the raw PCM is wiped from memory.
 */
export async function confirmDeviceSwitchBeforeSend(
  currentGroupHash: string | null | undefined,
  captured: CapturedPcm,
  confirm: () => Promise<boolean>,
  currentGroupLabel?: string | null,
): Promise<boolean> {
  if (!needsDeviceSwitchConfirm(currentGroupHash, captured.device.idHash, currentGroupLabel, captured.device.label)) {
    return true;
  }
  if (await confirm()) return true;
  captured.pcm.fill(0);
  return false;
}
