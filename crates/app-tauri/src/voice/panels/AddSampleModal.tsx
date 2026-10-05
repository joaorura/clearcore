import type { CapturedPcm, EnrollmentLabels } from '../enrollmentTypes';
import { formatSecondsLocale } from '../format';
import type { Translate } from '../hooks/voiceProfileLogic';
import { JobFeedbackBlock, RecordingIndicator, VuMeter, type JobFeedbackView } from './shared';

export interface AddSampleModalProps {
  t: Translate;
  locale: string;
  labels: EnrollmentLabels;
  name: string;
  onNameChange: (name: string) => void;
  captured: CapturedPcm | null;
  isRecording: boolean;
  liveVoiceLevel: number;
  recordingElapsedSeconds: number;
  captureError: string | null;
  jobBusy: boolean;
  feedback: JobFeedbackView | null;
  playingAudioId: string | null;
  onStart: () => void;
  onStop: () => void;
  onPlayPreview: () => void;
  onCancel: () => void;
  onSave: () => void;
}

/** "+ add sample": records one voluntary sample, previews it from memory and sends it. */
export function AddSampleModal(p: AddSampleModalProps) {
  const { t } = p;
  return (
    <div className="modal-overlay">
      <div className="modal-content profile-add-modal">
        <h3 style={{ fontSize: '1.2rem', marginBottom: 8, color: 'var(--text-main)' }}>
          {t('voiceProfile.modalAddSampleTitle')}
        </h3>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginBottom: 16 }}>
          {t('voiceProfile.modalAddSampleDesc')}
        </p>

        <div style={{ marginBottom: 14 }}>
          <label style={{ display: 'block', fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 6 }}>
            {t('voiceProfile.sampleNameLabel')}
          </label>
          <input
            type="text"
            className="device-select"
            placeholder={t('voiceProfile.sampleNamePlaceholder')}
            value={p.name}
            onChange={(e) => p.onNameChange(e.target.value)}
          />
        </div>

        <VuMeter t={t} isRecording={p.isRecording} level={p.liveVoiceLevel} style={{ marginBottom: 16 }} />

        {p.captureError && (
          <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 12 }}>
            {p.captureError}
          </div>
        )}

        <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 16 }}>
          {p.isRecording ? (
            <RecordingIndicator t={t} locale={p.locale} elapsed={p.recordingElapsedSeconds} onStop={p.onStop} />
          ) : (
            <button className="record-btn-trigger" disabled={p.jobBusy} onClick={p.onStart}>
              🎙️ {p.captured ? t('voiceProfile.redoSample') : t('voiceProfile.recordSample')}
            </button>
          )}
        </div>

        {p.captured && !p.isRecording && (
          <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 10, marginBottom: 16 }}>
            <button className="action-btn" onClick={p.onPlayPreview}>
              {p.playingAudioId === 'modal-preview' ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
            </button>
            <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
              {t('voiceProfile.recordedDuration', { sec: formatSecondsLocale(p.captured.durationSec, p.locale) })}
            </span>
          </div>
        )}

        <JobFeedbackBlock feedback={p.feedback} labels={p.labels} t={t} locale={p.locale} />

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 12 }}>
          <button className="action-btn" onClick={p.onCancel}>
            {t('voiceProfile.modalCancel')}
          </button>
          <button className="action-btn primary-next-btn" disabled={!p.captured || p.jobBusy || p.isRecording} onClick={p.onSave}>
            {t('voiceProfile.modalSave')}
          </button>
        </div>
      </div>
    </div>
  );
}
