import { describe, it, expect } from 'vitest';

describe('Auto Updater version comparison logic', () => {
  function compareVersions(vA: string, vB: string): number {
    const cleanA = vA.replace(/^v/, '').split(/[-_+]/)[0];
    const cleanB = vB.replace(/^v/, '').split(/[-_+]/)[0];
    const partsA = cleanA.split('.').map(Number);
    const partsB = cleanB.split('.').map(Number);

    for (let i = 0; i < 3; i++) {
      const a = partsA[i] || 0;
      const b = partsB[i] || 0;
      if (a > b) return 1;
      if (a < b) return -1;
    }
    const normA = vA.replace(/^v/, '');
    const normB = vB.replace(/^v/, '');
    return normA.localeCompare(normB);
  }

  it('correctly detects newer versions', () => {
    expect(compareVersions('v0.1.0-beta.3', '0.1.0-beta.2')).toBeGreaterThan(0);
    expect(compareVersions('v0.2.0', '0.1.0-beta.2')).toBeGreaterThan(0);
    expect(compareVersions('v1.0.0', '0.9.9')).toBeGreaterThan(0);
  });

  it('detects older or equal versions', () => {
    expect(compareVersions('0.1.0-beta.2', '0.1.0-beta.2')).toBe(0);
    expect(compareVersions('v0.1.0-beta.1', '0.1.0-beta.2')).toBeLessThan(0);
  });
});
