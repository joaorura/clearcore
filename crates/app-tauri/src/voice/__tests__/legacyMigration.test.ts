import { describe, it, expect } from 'vitest';
import { purgeLegacyVoiceKeys, takeLegacyNotice, LEGACY_VOICE_KEYS, LEGACY_NOTICE_SHOWN_KEY } from '../legacyMigration';

class FakeStorage {
  data = new Map<string, string>();
  getItem(k: string) { return this.data.has(k) ? this.data.get(k)! : null; }
  setItem(k: string, v: string) { this.data.set(k, v); }
  removeItem(k: string) { this.data.delete(k); }
}
class ThrowingStorage {
  getItem(): string | null { throw new Error('denied'); }
  setItem(): void { throw new Error('denied'); }
  removeItem(): void { throw new Error('denied'); }
}

describe('purgeLegacyVoiceKeys', () => {
  it('removes every legacy key and reports that local samples existed', () => {
    const s = new FakeStorage();
    s.setItem('clearcore_voice_profile_samples', JSON.stringify([{ id: 'x' }]));
    s.setItem('clearcore_voice_profile_enrolled', 'true');
    s.setItem('clearcore_voice_intake_takes', '[]');
    s.setItem('clearcore_voice_sample_count', '1');
    s.setItem('clearcore_voice_reading_mode', 'true'); // UI preference: kept
    expect(purgeLegacyVoiceKeys(s)).toBe(true);
    for (const k of LEGACY_VOICE_KEYS) expect(s.getItem(k), k).toBeNull();
    expect(s.getItem('clearcore_voice_reading_mode')).toBe('true');
  });
  it('reports false when there were no local samples (keys still removed)', () => {
    const s = new FakeStorage();
    s.setItem('clearcore_voice_profile_enrolled', 'false');
    s.setItem('clearcore_voice_profile_samples', '[]');
    expect(purgeLegacyVoiceKeys(s)).toBe(false);
    expect(s.getItem('clearcore_voice_profile_enrolled')).toBeNull();
    expect(purgeLegacyVoiceKeys(new FakeStorage())).toBe(false);
  });
  it('counts a positive legacy sample count or corrupt sample JSON as local samples', () => {
    const a = new FakeStorage(); a.setItem('clearcore_voice_sample_count', '3');
    expect(purgeLegacyVoiceKeys(a)).toBe(true);
    const b = new FakeStorage(); b.setItem('clearcore_voice_profile_samples', '{oops');
    expect(purgeLegacyVoiceKeys(b)).toBe(true);
  });
  it('never throws (blocked storage or none)', () => {
    expect(purgeLegacyVoiceKeys(new ThrowingStorage())).toBe(false);
    expect(purgeLegacyVoiceKeys(undefined)).toBe(false);
  });
});

describe('takeLegacyNotice', () => {
  it('shows the notice once', () => {
    const s = new FakeStorage();
    expect(takeLegacyNotice(true, s)).toBe(true);
    expect(s.getItem(LEGACY_NOTICE_SHOWN_KEY)).toBe('true');
    expect(takeLegacyNotice(true, s)).toBe(false);
    expect(takeLegacyNotice(false, new FakeStorage())).toBe(false);
  });
});
