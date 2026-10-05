import type { EnrollmentLabels, ServiceSample, SpeechBudget } from './enrollmentTypes';
import { formatSecondsLocale } from './format';

const badge = { fontSize: 11, padding: '2px 8px', borderRadius: 10, border: '1px solid var(--border-color)' } as const;

export function VoiceSampleGallery(p: {
  samples: ServiceSample[]; budget: SpeechBudget; labels: EnrollmentLabels;
  onDelete: (id: string) => void; deletingId?: string | null; highlightDelete?: boolean; lang?: string;
  /** A profile build is running: deleting would change its input, so delete is locked. */
  busy?: boolean;
}) {
  const { samples, budget, labels, onDelete, deletingId, highlightDelete, lang = 'en-US', busy = false } = p;
  return (
    <ul className="voice-sample-gallery" style={{ listStyle: 'none', margin: 0, padding: 0, display: 'flex', flexDirection: 'column', gap: 8 }}>
      {samples.map((s) => {
        const deleting = deletingId === s.id;
        const pct = budget.maxSeconds > 0 ? Math.min(100, Math.max(0, (s.speechSeconds / budget.maxSeconds) * 100)) : 0;
        return (
          <li
            key={s.id}
            data-testid={`sample-${s.id}`}
            style={{ display: 'flex', alignItems: 'center', gap: 12, padding: '8px 12px', border: '1px solid var(--border-color)', borderRadius: 8 }}
          >
            <div style={{ flex: 1, minWidth: 0 }}>
              <div style={{ fontWeight: 600 }}>{s.name}</div>
              <div style={{ fontSize: 12, color: 'var(--text-muted)' }}>
                {formatSecondsLocale(s.speechSeconds, lang)} {labels.seconds} · {s.deviceLabel}
              </div>
              {!s.needsReenroll && (
                <div className="sample-usage-bar" style={{ height: 4, marginTop: 4, borderRadius: 2, background: 'var(--border-color)' }}>
                  <div style={{ width: `${pct}%`, height: '100%', background: '#4ade80' }} />
                </div>
              )}
            </div>
            <div style={{ display: 'flex', gap: 6, flexWrap: 'wrap' }}>
              {s.otherMicrophone && <span style={{ ...badge, color: '#fbbf24' }}>{labels.otherMicrophone}</span>}
              {s.needsReenroll && <span style={{ ...badge, color: '#f87171' }}>{labels.needsReenroll}</span>}
              <span style={{ ...badge, color: 'var(--text-muted)' }}>{s.usedInProfile ? labels.usedInProfile : labels.notUsed}</span>
            </div>
            <button
              type="button"
              className="action-btn"
              data-testid={`delete-${s.id}`}
              data-highlight={highlightDelete ? 'true' : undefined}
              aria-label={`${labels.deleteAction}: ${s.name}`}
              disabled={deleting || busy}
              onClick={() => onDelete(s.id)}
              style={highlightDelete ? { borderColor: '#f87171', color: '#f87171', boxShadow: '0 0 0 2px rgba(248,113,113,0.35)' } : undefined}
            >
              {deleting ? labels.deleting : labels.deleteAction}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
