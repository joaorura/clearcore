import type { CSSProperties } from 'react';
import type { EnrollmentJob, EnrollmentLabels } from '../enrollmentTypes';
import { EnrollmentJobStatus } from '../EnrollmentJobStatus';
import { formatSecondsLocale } from '../format';
import { MAX_RECORD_SECONDS, MIN_RECORD_SECONDS } from '../enrollmentTypes';
import type { Translate } from '../hooks/voiceProfileLogic';

/** Job feedback a panel shows: sending, job stage/quality, translated error. */
export interface JobFeedbackView {
  jobBusy: boolean;
  currentJob: EnrollmentJob | null;
  enrollErrorText: string | null;
}

export function JobFeedbackBlock({ feedback, labels, t, locale }: {
  feedback: JobFeedbackView | null; labels: EnrollmentLabels; t: Translate; locale: string;
}) {
  if (!feedback || !(feedback.jobBusy || feedback.currentJob || feedback.enrollErrorText)) return null;
  return (
    <div style={{ marginBottom: 12 }}>
      {feedback.jobBusy && !feedback.currentJob && (
        <div role="status" style={{ fontSize: 13, color: 'var(--text-muted)' }}>{t('voiceProfile.sendingSample')}</div>
      )}
      <EnrollmentJobStatus job={feedback.currentJob} labels={labels} lang={locale} />
      {feedback.enrollErrorText && (
        <div role="alert" style={{ color: '#f87171', fontSize: 13, marginTop: 4 }}>{feedback.enrollErrorText}</div>
      )}
    </div>
  );
}

/** Live level from the PCM recorder (0 when idle; never synthesized). */
export function VuMeter({ t, isRecording, level, colorByLevel, style }: {
  t: Translate; isRecording: boolean; level: number; colorByLevel?: boolean; style?: CSSProperties;
}) {
  const shown = isRecording ? level : 0;
  const background = !colorByLevel
    ? '#22c55e'
    : level > 80
      ? 'linear-gradient(90deg, #22c55e, #eab308, #ef4444)'
      : level > 40
        ? 'linear-gradient(90deg, #22c55e, #38bdf8)'
        : '#22c55e';
  return (
    <div className="vu-meter-panel" style={style}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
        <span>{t('voiceProfile.voiceLevel')}</span>
        <span>{`${shown}%`}</span>
      </div>
      <div className="vu-meter-track">
        <div className="vu-meter-bar" style={{ width: `${shown}%`, background }} />
      </div>
    </div>
  );
}

/** "Recording: 1,2 s" plus the stop button, enabled from the minimum duration on. */
export function RecordingIndicator({ t, locale, elapsed, onStop, showHint }: {
  t: Translate; locale: string; elapsed: number; onStop: () => void; showHint?: boolean;
}) {
  const tooShort = elapsed < MIN_RECORD_SECONDS;
  return (
    <div className="recording-active-container">
      <div className="recording-status-group">
        <span className="recording-pulsing-dot" />
        <span style={{ fontWeight: 600, color: '#f87171' }}>
          {t('voiceProfile.recordingStatus', { elapsed: formatSecondsLocale(elapsed, locale), max: String(MAX_RECORD_SECONDS) })}
        </span>
        {showHint && (
          <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
            ({t('voiceProfile.recordingHint')})
          </span>
        )}
      </div>
      <button
        type="button"
        className="stop-record-btn"
        disabled={tooShort}
        onClick={onStop}
        title={
          tooShort
            ? t('voiceProfile.minRecordingTitle', { min: formatSecondsLocale(MIN_RECORD_SECONDS, locale) })
            : t('voiceProfile.stopRecordingBtn')
        }
      >
        ⏹️ {t('voiceProfile.stopRecordingBtn')}
      </button>
    </div>
  );
}
