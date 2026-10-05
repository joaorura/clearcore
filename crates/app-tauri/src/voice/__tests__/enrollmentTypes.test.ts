import { it, expect } from 'vitest';
import { CMD, MAX_SPEECH_SECONDS, MIN_TAKE_MARGIN_SECONDS } from '../enrollmentTypes';

it('exposes the agreed constants and unique command names', () => {
  expect(MAX_SPEECH_SECONDS).toBe(90);
  expect(MIN_TAKE_MARGIN_SECONDS).toBe(5);
  const names = Object.values(CMD);
  expect(new Set(names).size).toBe(names.length);
  expect(names.every((n) => n.startsWith('enrollment_'))).toBe(true);
});
