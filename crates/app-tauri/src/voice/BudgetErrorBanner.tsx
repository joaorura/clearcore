import type { EnrollmentLabels } from './enrollmentTypes';

export function BudgetErrorBanner({ remainingSeconds, labels }: { remainingSeconds: number | null; labels: EnrollmentLabels }) {
  let body: string;
  if (remainingSeconds === null) {
    // No figure available: drop the sentence that carries the placeholder, keep the rest.
    body = labels.budgetExceededBody
      .split(/(?<=[.!?])\s+/)
      .filter((part) => !part.includes('{remaining}'))
      .join(' ')
      .trim();
    if (body === '') body = labels.budgetExceededBody.replace(/\{remaining\}/g, '').trim();
  } else {
    body = labels.budgetExceededBody.replace(/\{remaining\}/g, String(remainingSeconds));
  }
  return (
    <div
      role="alert"
      className="feedback-banner"
      style={{ padding: '10px 14px', borderRadius: 8, border: '1px solid #f87171', background: 'rgba(248,113,113,0.1)', color: '#fca5a5' }}
    >
      <strong>{labels.budgetExceededTitle}</strong>
      <div style={{ fontSize: 13 }}>{body}</div>
    </div>
  );
}
