import type { EnrollmentLabels, SampleList } from '../enrollmentTypes';
import { VoiceBudgetMeter } from '../VoiceBudgetMeter';
import { VoiceSampleGallery } from '../VoiceSampleGallery';
import { BudgetErrorBanner } from '../BudgetErrorBanner';
import { interpolateBudgetBody, type Translate } from '../hooks/voiceProfileLogic';
import { JobFeedbackBlock, type JobFeedbackView } from './shared';

export interface GalleryPanelProps {
  t: Translate;
  locale: string;
  labels: EnrollmentLabels;
  sampleList: SampleList | null;
  samplesLoadFailed: boolean;
  deletingId: string | null;
  /** A profile build is running: delete is locked. */
  busy?: boolean;
  /** Set on ENROLL_BUDGET_EXCEEDED: shows the banner and highlights delete (spec §4.4). */
  budgetError: { remainingSeconds: number | null } | null;
  /** Feedback of the voluntary sample job once its modal closed. */
  feedback: JobFeedbackView | null;
  onDelete: (id: string) => void;
  onAddSample: () => void;
}

/** Speech budget, the samples stored in the service and the delete action on each one. */
export function GalleryPanel(p: GalleryPanelProps) {
  const { t, labels, sampleList, budgetError } = p;
  const samples = sampleList?.samples ?? [];
  const bannerLabels = { ...labels, budgetExceededBody: interpolateBudgetBody(labels.budgetExceededBody, budgetError?.remainingSeconds ?? null, p.locale) };
  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0 }}>
          {t('voiceProfile.sampleGalleryDesc')}
        </p>
        <button className="action-btn add-sample-accent-btn" onClick={p.onAddSample}>
          {t('voiceProfile.addNewSampleBtn')}
        </button>
      </div>

      <JobFeedbackBlock feedback={p.feedback} labels={labels} t={t} locale={p.locale} />

      {sampleList && sampleList.serviceOutdated && (
        <div role="alert" style={{ color: '#fbbf24', fontSize: 13, marginBottom: 12 }}>
          {labels.errors.SERVICE_OUTDATED}
        </div>
      )}

      {sampleList && !sampleList.serviceOutdated && (
        <div style={{ marginBottom: 12 }}>
          <VoiceBudgetMeter budget={sampleList.budget} labels={labels} lang={p.locale} />
        </div>
      )}

      {budgetError && (
        <div style={{ marginBottom: 12 }}>
          <BudgetErrorBanner remainingSeconds={budgetError.remainingSeconds} labels={bannerLabels} />
        </div>
      )}

      {p.samplesLoadFailed && (
        <div role="alert" style={{ color: '#fbbf24', fontSize: 13, marginBottom: 12 }}>
          {t('voiceProfile.samplesLoadFailed')}
        </div>
      )}

      {sampleList && samples.length > 0 ? (
        <VoiceSampleGallery
          samples={samples}
          budget={sampleList.budget}
          labels={labels}
          onDelete={p.onDelete}
          deletingId={p.deletingId}
          busy={p.busy}
          highlightDelete={budgetError !== null}
          lang={p.locale}
        />
      ) : (
        <div className="empty-state-card">{t('voiceProfile.emptyGallery')}</div>
      )}
    </div>
  );
}
