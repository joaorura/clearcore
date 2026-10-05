import { it, expect } from 'vitest';
import { budgetPercent, wouldExceed, canRecordTake, formatSeconds } from '../speechBudget';
import type { SpeechBudget } from '../enrollmentTypes';

const b = (used: number): SpeechBudget => ({ usedSeconds: used, maxSeconds: 90, remainingSeconds: Math.max(0, 90 - used) });
it('wouldExceed is exact at the boundary', () => { expect(wouldExceed(b(80), 10)).toBe(false); expect(wouldExceed(b(80), 10.01)).toBe(true); });
it('canRecordTake needs 5 s of margin and must fit', () => {
  expect(canRecordTake(b(86), 1)).toBe(false); expect(canRecordTake(b(80), 6)).toBe(true); expect(canRecordTake(b(80), 11)).toBe(false);
});
it('budgetPercent clamps', () => { expect(budgetPercent(b(45))).toBe(50); expect(budgetPercent(b(120))).toBe(100); expect(budgetPercent(b(-3))).toBe(0); });
it('formatSeconds uses one decimal', () => { expect(formatSeconds(84.26)).toBe('84.3'); expect(formatSeconds(0)).toBe('0.0'); });
