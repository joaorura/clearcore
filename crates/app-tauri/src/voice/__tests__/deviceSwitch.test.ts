import { describe, it, expect, vi } from 'vitest';
import { needsDeviceSwitchConfirm, confirmDeviceSwitchBeforeSend } from '../deviceSwitch';
import type { CapturedPcm } from '../enrollmentTypes';

const A = 'a'.repeat(64);
const B = 'b'.repeat(64);
const take = (idHash: string): CapturedPcm => ({
  pcm: new Float32Array([0.5, -0.25]), sampleRate: 48000, durationSec: 2, peak: 0.5, rmsDbfs: -9,
  device: { label: 'Mic', idHash },
});

describe('needsDeviceSwitchConfirm', () => {
  it('asks only when there is a current group and the hash differs', () => {
    expect(needsDeviceSwitchConfirm(A, B)).toBe(true);
    expect(needsDeviceSwitchConfirm(A, A)).toBe(false);
    expect(needsDeviceSwitchConfirm(null, B)).toBe(false); // older service / no group yet
    expect(needsDeviceSwitchConfirm(undefined, B)).toBe(false);
    expect(needsDeviceSwitchConfirm('', B)).toBe(false);
  });
});

describe('confirmDeviceSwitchBeforeSend', () => {
  it('sends without asking when the microphone is the same or the service sent no group', async () => {
    const confirm = vi.fn(async () => false);
    expect(await confirmDeviceSwitchBeforeSend(A, take(A), confirm)).toBe(true);
    expect(await confirmDeviceSwitchBeforeSend(null, take(B), confirm)).toBe(true);
    expect(confirm).not.toHaveBeenCalled();
  });
  it('asks on a different microphone and sends when the user confirms', async () => {
    const confirm = vi.fn(async () => true);
    const c = take(B);
    expect(await confirmDeviceSwitchBeforeSend(A, c, confirm)).toBe(true);
    expect(confirm).toHaveBeenCalledTimes(1);
    expect([...c.pcm]).toEqual([0.5, -0.25]);
  });
  it('cancel does not send and wipes the PCM', async () => {
    const c = take(B);
    expect(await confirmDeviceSwitchBeforeSend(A, c, async () => false)).toBe(false);
    expect(c.pcm.every((v) => v === 0)).toBe(true);
  });
});
