import { MIN_TAKE_MARGIN_SECONDS, type SpeechBudget } from './enrollmentTypes';

const TOLERANCE = 1e-4;

export function budgetPercent(b: SpeechBudget): number {
  if (!(b.maxSeconds > 0)) return 100;
  return Math.min(100, Math.max(0, (b.usedSeconds / b.maxSeconds) * 100));
}

export function wouldExceed(b: SpeechBudget, speechSeconds: number): boolean {
  return b.usedSeconds + speechSeconds > b.maxSeconds + TOLERANCE;
}

export function canRecordTake(b: SpeechBudget, takeSeconds: number): boolean {
  return b.remainingSeconds >= MIN_TAKE_MARGIN_SECONDS && takeSeconds <= b.remainingSeconds;
}

export function formatSeconds(s: number): string {
  return s.toFixed(1);
}
