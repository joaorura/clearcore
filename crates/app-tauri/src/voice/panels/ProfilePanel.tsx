import React, { useState, useEffect, useRef, useCallback } from 'react';
import type { VoiceProfileStatus } from '../../types';
import type { EnrollmentLabels } from '../enrollmentTypes';
import { hasServiceVoiceProfile, voiceProfileErrorKey, voiceProfileUnsupported, type Translate } from '../hooks/voiceProfileLogic';
import { profileStatusLabel } from './profileStatusLabel';
import { JobFeedbackBlock, type JobFeedbackView } from './shared';
import { getVoiceIsolationIntensity, setVoiceIsolationIntensity } from '../../bridge';

export const ISOLATION_PRESETS = [
  { value: 50, key: 'voiceProfile.intensityMild' as const },
  { value: 75, key: 'voiceProfile.intensityStandard' as const },
  { value: 100, key: 'voiceProfile.intensityMaximum' as const },
];

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
  /** True while the neural profile is actively being built and applied. */
  isBuilding?: boolean;
  /** Feedback of the build job. */
  feedback: JobFeedbackView | null;
  onBuildProfile: () => void;
  /** Callback to restart guided enrollment flow. */
  onResetEnrollment?: () => void;
  /** Callback to toggle voice isolation active/inactive in audio engine. */
  onToggleVoiceIsolation?: (enabled: boolean) => void;
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
  const hasProfile = hasServiceVoiceProfile(profileStatus);
  // Only an explicit `false` from the service; an absent field keeps today's behavior.
  const unsupported = voiceProfileUnsupported(profileStatus);
  const unsupportedText = t('voiceProfile.profileUnsupportedNotice');

  const [intensity, setIntensity] = useState<number>(() => {
    if (typeof profileStatus?.voice_isolation_intensity === 'number') {
      return profileStatus.voice_isolation_intensity;
    }
    const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_voice_isolation_intensity') : null;
    return saved ? Number(saved) : 100;
  });

  const [feedback, setFeedback] = useState<string | null>(null);
  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);
  const feedbackTimerRef = useRef<NodeJS.Timeout | null>(null);

  const isControlDisabled = !status.active || p.busy || Boolean(p.isBuilding) || unsupported;

  useEffect(() => {
    if (typeof profileStatus?.voice_isolation_intensity === 'number') {
      setIntensity(profileStatus.voice_isolation_intensity);
    }
  }, [profileStatus?.voice_isolation_intensity]);

  useEffect(() => {
    let isMounted = true;
    getVoiceIsolationIntensity().then((val) => {
      if (isMounted && typeof val === 'number') {
        setIntensity(val);
      }
    }).catch(() => {});
    return () => {
      isMounted = false;
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
      if (feedbackTimerRef.current) clearTimeout(feedbackTimerRef.current);
    };
  }, []);

  const sendIntensityUpdate = useCallback(async (newVal: number) => {
    try {
      await setVoiceIsolationIntensity(newVal);
      setFeedback(t('voiceProfile.intensityFeedback', { intensity: newVal }));
      if (feedbackTimerRef.current) clearTimeout(feedbackTimerRef.current);
      feedbackTimerRef.current = setTimeout(() => setFeedback(null), 2500);
    } catch {
      // ignore
    }
  }, [t]);

  const handleSliderChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = Number(e.target.value);
    setIntensity(val);
    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
    debounceTimerRef.current = setTimeout(() => {
      sendIntensityUpdate(val);
    }, 200);
  };

  const handlePresetClick = (val: number) => {
    if (isControlDisabled) return;
    setIntensity(val);
    if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
    sendIntensityUpdate(val);
  };

  const getIntensityLabel = (val: number) => {
    if (val === 50) return t('voiceProfile.intensityMild');
    if (val === 75) return t('voiceProfile.intensityStandard');
    if (val === 100) return t('voiceProfile.intensityMaximum');
    return `${val}%`;
  };
  return (
    <div>
      {unsupported && (
        <div id="voice-profile-unsupported" role="status" style={{ color: '#fbbf24', fontSize: 13, marginBottom: 8 }}>
          {unsupportedText}
        </div>
      )}
      {status.active && (
        <div
          role="status"
          className="profile-active-banner"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            padding: '12px 16px',
            background: 'rgba(34, 197, 94, 0.12)',
            border: '1px solid rgba(34, 197, 94, 0.35)',
            borderRadius: 8,
            marginBottom: 16,
            color: '#4ade80',
          }}
        >
          <span style={{ fontSize: '1.4rem' }}>✅</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 700, fontSize: '0.95rem' }}>
              {t('voiceProfile.profileCompletedTitle')}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 2 }}>
              {t('voiceProfile.appliedInServiceNote')}
            </div>
          </div>
          {p.onToggleVoiceIsolation && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <label className="toggle-switch-wrapper" title={t('voiceProfile.isolationSwitch')}>
                <input
                  type="checkbox"
                  checked={true}
                  disabled={p.busy || p.isBuilding}
                  onChange={(e) => p.onToggleVoiceIsolation?.(e.target.checked)}
                />
                <span className="toggle-switch-slider" />
              </label>
              <span className="toggle-status-pill pill-on">
                {t('voiceProfile.isolationEnabled')}
              </span>
            </div>
          )}
          {p.onResetEnrollment && (
            <button
              className="action-btn"
              onClick={p.onResetEnrollment}
              style={{ fontSize: '0.8rem', padding: '6px 12px', whiteSpace: 'nowrap' }}
              title={t('voiceProfile.reEnrollHint')}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          )}
        </div>
      )}
      {hasProfile && status.active && (
        <div
          role="alert"
          className="profile-troubleshoot-banner"
          style={{
            display: 'flex',
            alignItems: 'flex-start',
            gap: 12,
            padding: '12px 16px',
            background: 'rgba(234, 179, 8, 0.08)',
            border: '1px solid rgba(234, 179, 8, 0.3)',
            borderRadius: 8,
            marginBottom: 16,
            color: '#eab308',
          }}
        >
          <span style={{ fontSize: '1.3rem', lineHeight: '1.2' }}>💡</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 700, fontSize: '0.9rem', color: '#fde047' }}>
              {t('voiceProfile.troubleshootTitle')}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 3, lineHeight: '1.4' }}>
              {t('voiceProfile.troubleshootDesc')}
            </div>
          </div>
          {p.onResetEnrollment && (
            <button
              className="action-btn"
              onClick={p.onResetEnrollment}
              style={{ fontSize: '0.75rem', padding: '4px 10px', whiteSpace: 'nowrap' }}
              title={t('voiceProfile.reEnrollHint')}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          )}
        </div>
      )}
      {!hasProfile && !status.active && !profileStatus.voice_profile_error && (
        <div
          role="status"
          className="profile-empty-banner"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            padding: '12px 16px',
            background: 'rgba(148, 163, 184, 0.08)',
            border: '1px solid rgba(148, 163, 184, 0.2)',
            borderRadius: 8,
            marginBottom: 16,
            color: 'var(--text-main)',
          }}
        >
          <span style={{ fontSize: '1.4rem' }}>⚪</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 700, fontSize: '0.95rem' }}>
              {t('voiceProfile.noProfileTitle')}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 2 }}>
              {t('voiceProfile.noProfileDesc')}
            </div>
          </div>
          {p.onResetEnrollment && (
            <button
              className="action-btn"
              onClick={p.onResetEnrollment}
              style={{ fontSize: '0.8rem', padding: '6px 12px', whiteSpace: 'nowrap' }}
            >
              {t('voiceProfile.startEnrollBtn')}
            </button>
          )}
        </div>
      )}
      {profileStatus.voice_profile_error && !status.active && (
        <div
          role="alert"
          className="profile-corrupted-banner"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            padding: '12px 16px',
            background: 'rgba(239, 68, 68, 0.12)',
            border: '1px solid rgba(239, 68, 68, 0.35)',
            borderRadius: 8,
            marginBottom: 16,
            color: '#f87171',
          }}
        >
          <span style={{ fontSize: '1.4rem' }}>🔴</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 700, fontSize: '0.95rem', color: '#f87171' }}>
              {t('voiceProfile.profileCorruptedTitle')}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 2 }}>
              {t(`voiceProfile.${voiceProfileErrorKey(profileStatus.voice_profile_error)}`)}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 2 }}>
              {t('voiceProfile.profileCorruptedDesc')}
            </div>
          </div>
          {p.onResetEnrollment && (
            <button
              className="action-btn"
              onClick={p.onResetEnrollment}
              style={{ fontSize: '0.8rem', padding: '6px 12px', whiteSpace: 'nowrap' }}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          )}
        </div>
      )}
      {!status.active && hasProfile && !profileStatus.voice_profile_error && (
        <div
          role="status"
          className="profile-paused-banner"
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 12,
            padding: '12px 16px',
            background: 'rgba(148, 163, 184, 0.1)',
            border: '1px solid rgba(148, 163, 184, 0.25)',
            borderRadius: 8,
            marginBottom: 16,
            color: 'var(--text-main)',
          }}
        >
          <span style={{ fontSize: '1.4rem' }}>⏸️</span>
          <div style={{ flex: 1 }}>
            <div style={{ fontWeight: 700, fontSize: '0.95rem' }}>
              {t('voiceProfile.isolationSwitch')}: {t('voiceProfile.isolationDisabled')}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 2 }}>
              {t('voiceProfile.isolationDisabledFeedback')}
            </div>
          </div>
          {p.onToggleVoiceIsolation && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <label className="toggle-switch-wrapper" title={t('voiceProfile.isolationSwitch')}>
                <input
                  type="checkbox"
                  checked={false}
                  disabled={p.busy || p.isBuilding}
                  onChange={(e) => p.onToggleVoiceIsolation?.(e.target.checked)}
                />
                <span className="toggle-switch-slider" />
              </label>
              <span className="toggle-status-pill pill-off">
                {t('voiceProfile.isolationDisabled')}
              </span>
            </div>
          )}
        </div>
      )}
      <div className="profile-overview-box">
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.statusTitle')}</div>
          <div className="overview-value" style={{ color: status.active ? '#4ade80' : status.corrupted ? '#f87171' : '#fbbf24' }}>
            {status.text}
          </div>
          {status.active && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.75rem', marginTop: 4 }}>
              {t('voiceProfile.appliedInServiceNote')}
            </div>
          )}
          {profileStatus.voice_profile_error && !status.active && (
            <div style={{ color: status.corrupted ? '#f87171' : '#fbbf24', fontSize: '0.75rem', marginTop: 4 }}>
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

      {hasProfile && (
        <div
          className="voice-isolation-intensity-block"
          style={{
            marginTop: 16,
            padding: '16px',
            background: 'var(--bg-secondary)',
            borderRadius: 8,
            border: '1px solid var(--border-color)',
            opacity: isControlDisabled ? 0.6 : 1,
          }}
          data-testid="voice-isolation-intensity"
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              marginBottom: 8,
              flexWrap: 'wrap',
              gap: 8,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <span style={{ fontSize: '1.1rem' }}>🎚️</span>
              <span style={{ fontWeight: 600, fontSize: '0.95rem', color: 'var(--text-main)' }}>
                {t('voiceProfile.intensityTitle')}
              </span>
            </div>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 6,
                background: 'var(--bg-card)',
                padding: '3px 10px',
                borderRadius: 6,
                border: '1px solid var(--border-color)',
              }}
            >
              <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>Nível:</span>
              <span
                style={{
                  fontWeight: 700,
                  fontSize: '0.95rem',
                  color: intensity >= 75 ? '#38bdf8' : intensity >= 50 ? '#4ade80' : '#facc15',
                }}
                data-testid="voice-isolation-intensity-value"
              >
                {intensity}%
              </span>
              <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
                ({getIntensityLabel(intensity)})
              </span>
            </div>
          </div>

          <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 14 }}>
            {t('voiceProfile.intensityDesc')}
          </p>

          {/* Main Slider Control */}
          <div style={{ position: 'relative', padding: '0 4px', marginBottom: 12 }}>
            <input
              type="range"
              min="0"
              max="100"
              step="5"
              value={intensity}
              disabled={isControlDisabled}
              onChange={handleSliderChange}
              aria-label={t('voiceProfile.intensityTitle')}
              data-testid="voice-isolation-intensity-slider"
              style={{
                width: '100%',
                height: 6,
                borderRadius: 3,
                outline: 'none',
                background: 'linear-gradient(to right, #facc15 0%, #4ade80 50%, #38bdf8 75%, #818cf8 100%)',
                cursor: isControlDisabled ? 'not-allowed' : 'pointer',
                opacity: isControlDisabled ? 0.5 : 1,
              }}
            />
          </div>

          {/* Preset Buttons */}
          <div
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(3, 1fr)',
              gap: 6,
            }}
          >
            {ISOLATION_PRESETS.map((preset) => {
              const isSelected = intensity === preset.value;
              return (
                <button
                  key={preset.value}
                  type="button"
                  disabled={isControlDisabled}
                  onClick={() => handlePresetClick(preset.value)}
                  className="action-btn"
                  style={{
                    fontSize: '0.75rem',
                    padding: '5px 4px',
                    textAlign: 'center',
                    background: isSelected ? 'var(--bg-card)' : 'transparent',
                    borderColor: isSelected ? '#3b82f6' : 'var(--border-color)',
                    color: isSelected ? '#93c5fd' : 'var(--text-muted)',
                    fontWeight: isSelected ? 600 : 400,
                    borderRadius: 4,
                    cursor: isControlDisabled ? 'not-allowed' : 'pointer',
                    transition: 'all 0.15s ease',
                  }}
                  data-testid={`isolation-preset-btn-${preset.value}`}
                >
                  {t(preset.key)}
                </button>
              );
            })}
          </div>

          {feedback && (
            <div
              className="feedback-banner success-banner"
              style={{ marginTop: 10, fontSize: '0.8rem', padding: '6px 12px' }}
            >
              ✓ {feedback}
            </div>
          )}
        </div>
      )}

      {p.stale && (
        <div role="status" style={{ color: '#fbbf24', fontSize: 13, marginTop: 8 }}>
          {t('voiceProfile.profileStale')}
        </div>
      )}

      <JobFeedbackBlock feedback={p.feedback} labels={p.labels} t={t} locale={p.locale} />

      {p.canBuild && (
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 12 }}>
          <button
            className="action-btn"
            disabled={p.busy || p.isBuilding || unsupported}
            title={unsupported ? unsupportedText : undefined}
            aria-describedby={unsupported ? 'voice-profile-unsupported' : undefined}
            onClick={p.onBuildProfile}
            style={{ fontSize: '0.8rem', padding: '6px 12px' }}
          >
            {p.isBuilding
              ? `⏳ ${t('voiceProfile.buildingProfile')}`
              : hasServiceVoiceProfile(profileStatus)
                ? t('voiceProfile.rebuildProfileBtn')
                : t('voiceProfile.activateProfileBtn')}
          </button>
        </div>
      )}
    </div>
  );
}
