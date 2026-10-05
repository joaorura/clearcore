import type { EnrollmentLabels, SpeechBudget } from './enrollmentTypes';
import { formatSecondsLocale } from './format';

function pctOf(used: number, max: number): number {
  if (!Number.isFinite(used) || !Number.isFinite(max) || max <= 0) return 0;
  return Math.min(100, Math.max(0, (used / max) * 100));
}

export function VoiceBudgetMeter({ budget, labels, lang = 'en-US' }: { budget: SpeechBudget; labels: EnrollmentLabels; lang?: string }) {
  const fmt = (n: number) => formatSecondsLocale(n, lang);
  const pct = pctOf(budget.usedSeconds, budget.maxSeconds);
  const full = pct >= 100;
  return (
    <div className="voice-budget-meter" style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: 13, color: 'var(--text-muted)' }}>
        <span>{labels.budgetTitle}</span>
        <span>{fmt(budget.usedSeconds)} / {fmt(budget.maxSeconds)} {labels.seconds}</span>
      </div>
      <div
        role="progressbar"
        aria-label={labels.budgetTitle}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={Math.round(pct)}
        style={{ height: 8, borderRadius: 4, background: 'var(--border-color)', overflow: 'hidden' }}
      >
        <div style={{ width: `${pct}%`, height: '100%', background: full ? '#f87171' : '#4ade80' }} />
      </div>
      <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
        {labels.budgetRemaining}: {fmt(budget.remainingSeconds)} {labels.seconds}
      </div>
    </div>
  );
}
