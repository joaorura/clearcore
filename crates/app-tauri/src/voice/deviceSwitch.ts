import type { CapturedPcm } from './enrollmentTypes';

/**
 * Spec §4.4 / D7: a sample from a different microphone switches the device group, and the earlier
 * samples stop being used. Ask only when the service reported a current group AND the new take's
 * device differs; an older service that sends no group is never asked about.
 */
export function needsDeviceSwitchConfirm(currentGroupHash: string | null | undefined, newDeviceIdHash: string): boolean {
  return typeof currentGroupHash === 'string' && currentGroupHash.length > 0 && currentGroupHash !== newDeviceIdHash;
}

/**
 * Resolves true when the take may be sent. On a device switch it asks `confirm`; if the user
 * cancels, nothing is sent and the raw PCM is wiped from memory.
 */
export async function confirmDeviceSwitchBeforeSend(
  currentGroupHash: string | null | undefined,
  captured: CapturedPcm,
  confirm: () => Promise<boolean>,
): Promise<boolean> {
  if (!needsDeviceSwitchConfirm(currentGroupHash, captured.device.idHash)) return true;
  if (await confirm()) return true;
  captured.pcm.fill(0);
  return false;
}
