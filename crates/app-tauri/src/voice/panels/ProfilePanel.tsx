import type { VoiceProfileStatus } from '../../types';
import type { EnrollmentLabels } from '../enrollmentTypes';
import { hasServiceVoiceProfile, voiceProfileErrorKey, type Translate } from '../hooks/voiceProfileLogic';
import { profileStatusLabel } from './profileStatusLabel';
import { JobFeedbackBlock, type JobFeedbackView } from './shared';

export interface ProfilePanelProps {
  t: Translate;
  locale: string;
  labels: EnrollmentLabels;
  /** Merged service status; "active" only when the service confirmed it (spec §9). */
  profileStatus: VoiceProfileStatus;
  samplesCount: number;
  /** There are samples in the service to build from. */
  canBuild: boolean;
  /** A job or a recording is running. */
  busy: boolean;
  /** Feedback of the build job. */
  feedback: JobFeedbackView | null;
  onBuildProfile: () => void;
  /** Samples changed since the last build in this session: ask to rebuild (never automatic). */
  stale?: boolean;
}

/**
 * Profile status as the service reports it, the build/rebuild action and its job stage.
 * The development model notice is shown by the card above the tabs (spec §9).
 */
export function ProfilePanel(p: ProfilePanelProps) {
  const { t, profileStatus } = p;
  const status = profileStatusLabel(profileStatus, t);
  return (
    <div>
      <div className="profile-overview-box">
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.statusTitle')}</div>
          <div className="overview-value" style={{ color: status.active ? '#4ade80' : '#fbbf24' }}>
            {status.text}
          </div>
          {status.active && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.75rem', marginTop: 4 }}>
              {t('voiceProfile.appliedInServiceNote')}
            </div>
          )}
          {profileStatus.voice_profile_error && !status.active && (
            <div style={{ color: '#fbbf24', fontSize: '0.75rem', marginTop: 4 }}>
              {t(`voiceProfile.${voiceProfileErrorKey(profileStatus.voice_profile_error)}`)}
            </div>
          )}
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.samplesRegisteredTitle')}</div>
          <div className="overview-value">
            {t('voiceProfile.statusSamplesPill', { count: String(p.samplesCount) })}
          </div>
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.neuralEqStatusTitle')}</div>
          <div className="overview-value" style={{ color: profileStatus.neural_eq_calibrated ? '#4ade80' : 'var(--text-muted)' }}>
            {profileStatus.neural_eq_calibrated ? t('voiceProfile.neuralEqCalibrated') : t('voiceProfile.neuralEqPending')}
          </div>
        </div>
      </div>

      {p.stale && (
        <div role="status" style={{ color: '#fbbf24', fontSize: 13, marginTop: 8 }}>
          {t('voiceProfile.profileStale')}
        </div>
      )}

      <JobFeedbackBlock feedback={p.feedback} labels={p.labels} t={t} locale={p.locale} />

      {p.canBuild && (
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 12 }}>
          <button className="action-btn" disabled={p.busy} onClick={p.onBuildProfile} style={{ fontSize: '0.8rem', padding: '6px 12px' }}>
            {hasServiceVoiceProfile(profileStatus) ? t('voiceProfile.rebuildProfileBtn') : t('voiceProfile.activateProfileBtn')}
          </button>
        </div>
      )}
    </div>
  );
}
