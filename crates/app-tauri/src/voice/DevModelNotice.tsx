import type { EnrollmentLabels } from './enrollmentTypes';

export function DevModelNotice({ labels }: { labels: EnrollmentLabels }) {
  return (
    <div role="note" className="dev-model-notice" style={{ fontSize: 12, color: '#fbbf24' }}>
      {labels.devModelNotice}
    </div>
  );
}
