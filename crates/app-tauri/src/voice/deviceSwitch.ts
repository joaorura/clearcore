import type { CapturedPcm } from './enrollmentTypes';

/**
 * Spec §4.4 / D7: a sample from a different microphone switches the device group, and the earlier
 * samples stop being used. Ask only when the service reported a current group AND the new take's
 * device differs; an older service that sends no group is never asked about.
 */
export function needsDeviceSwitchConfirm(
  currentGroupHash: string | null | undefined,
  newDeviceIdHash: string,
  currentGroupLabel?: string | null,
  newDeviceLabel?: string | null,
): boolean {
  if (!currentGroupHash || currentGroupHash.length === 0 || currentGroupHash === newDeviceIdHash) {
    return false;
  }
  // If both labels are non-empty and match case-insensitively, it is the EXACT same microphone.
  // Never prompt the user to switch microphones when the label is identical!
  if (
    typeof currentGroupLabel === 'string' &&
    typeof newDeviceLabel === 'string' &&
    currentGroupLabel.trim().length > 0 &&
    newDeviceLabel.trim().length > 0 &&
    currentGroupLabel.trim().toLowerCase() === newDeviceLabel.trim().toLowerCase()
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
