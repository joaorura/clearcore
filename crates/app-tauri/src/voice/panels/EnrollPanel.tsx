import type { EnrollmentLabels } from '../enrollmentTypes';
import { GUIDED_STEP_COUNT, STEP_QUESTIONS, type CompletedSteps } from '../hooks/guidedSteps';
import type { Translate } from '../hooks/voiceProfileLogic';
import { JobFeedbackBlock, RecordingIndicator, VuMeter, type JobFeedbackView } from './shared';

export interface EnrollPanelProps {
  t: Translate;
  locale: string;
  labels: EnrollmentLabels;
  isEnrolled: boolean;
  currentStep: number;
  isReadingMode: boolean;
  completedSteps: CompletedSteps;
  isRecording: boolean;
  liveVoiceLevel: number;
  recordingElapsedSeconds: number;
  /** Null while the voluntary sample modal shows it instead. */
  captureError: string | null;
  jobBusy: boolean;
  /** True while the neural profile is actively being built and applied. */
  isBuilding?: boolean;
  /** True while the recorded sample is being submitted/processed in the guided flow. */
  isSubmitting?: boolean;
  /** The microphone is being opened: recording cannot be started again. */
  isStarting?: boolean;
  /** Feedback of the sample job started here (null when another tab started the job). */
  feedback: JobFeedbackView | null;
  onSelectStep: (step: number) => void;
  onToggleReadingMode: () => void;
  onStartStep: (step: number) => void;
  onFinishStep: (step: number) => void;
  onRedoStep: (step: number) => void;
  onNextStep: () => void;
  onBuildProfile: () => void;
  onResetEnrollment: () => void;
}

/** Guided 5-step enrollment: question, live level, record/stop, take preview and job feedback. */
export function EnrollPanel(p: EnrollPanelProps) {
  const { t, currentStep, completedSteps, isReadingMode, isRecording } = p;
  const isBuilding = Boolean(p.isBuilding || (p.jobBusy && p.feedback?.jobBusy && !p.isSubmitting));
  const isProcessing = Boolean(p.jobBusy || p.isSubmitting || p.isBuilding || isBuilding);
  const completedCount = Object.keys(completedSteps).length;
  const isAllStepsCompleted = completedCount >= GUIDED_STEP_COUNT;
  const currentQ = STEP_QUESTIONS[currentStep - 1] ?? STEP_QUESTIONS[0];
  const currentTake = completedSteps[currentStep];
  const showFlow = !p.isEnrolled || completedCount < GUIDED_STEP_COUNT;

  return (
    <div>
      {p.isEnrolled && showFlow && (
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
          <button className="action-btn" onClick={p.onResetEnrollment} title={t('voiceProfile.reEnrollHint')} style={{ fontSize: '0.8rem', padding: '6px 12px' }}>
            {t('voiceProfile.reEnrollBtn')}
          </button>
        </div>
      )}

      {p.isEnrolled && showFlow && (
        <p style={{ color: 'var(--text-muted)', fontSize: '0.75rem', margin: '0 0 12px', textAlign: 'right' }}>
          {t('voiceProfile.reEnrollHint')}
        </p>
      )}

      {!showFlow && (
        <div
          className="enroll-completed-card"
          style={{
            background: 'linear-gradient(135deg, rgba(34, 197, 94, 0.12) 0%, rgba(16, 185, 129, 0.08) 100%)',
            border: '1px solid rgba(34, 197, 94, 0.35)',
            borderRadius: 12,
            padding: '28px 24px',
            textAlign: 'center',
            marginTop: 8,
            marginBottom: 16,
          }}
        >
          <div style={{ fontSize: '2.5rem', marginBottom: 12 }}>✅</div>
          <h3 style={{ margin: '0 0 8px', color: '#4ade80', fontSize: '1.25rem', fontWeight: 700 }}>
            {t('voiceProfile.profileCompletedTitle')}
          </h3>
          <p style={{ margin: '0 auto 20px', color: 'var(--text-muted)', fontSize: '0.9rem', maxWidth: 480, lineHeight: 1.5 }}>
            {t('voiceProfile.profileCompletedDesc')}
          </p>
          <div style={{ display: 'flex', justifyContent: 'center', gap: 12, flexWrap: 'wrap' }}>
            <button
              className="action-btn"
              onClick={p.onResetEnrollment}
              title={t('voiceProfile.reEnrollHint')}
              style={{
                fontWeight: 600,
                fontSize: '0.9rem',
                padding: '10px 22px',
                background: 'rgba(255, 255, 255, 0.08)',
                border: '1px solid rgba(255, 255, 255, 0.2)',
                borderRadius: 8,
                cursor: 'pointer',
              }}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          </div>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.75rem', margin: '16px auto 0', maxWidth: 440 }}>
            {t('voiceProfile.reEnrollHint')}
          </p>
        </div>
      )}

      <JobFeedbackBlock feedback={p.feedback} labels={p.labels} t={t} locale={p.locale} />

      {showFlow && (
        <div className="guided-sampling-section">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10, flexWrap: 'wrap', gap: 8 }}>
            <div style={{ fontWeight: 600, fontSize: '0.95rem', color: '#93c5fd' }}>
              {t('voiceProfile.stepProgress', { current: String(currentStep), total: String(GUIDED_STEP_COUNT) })}: {t(currentQ.categoryKey)}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {t('voiceProfile.stepPercent', { percent: String(Math.round((completedCount / GUIDED_STEP_COUNT) * 100)) })}
              </span>
              <button
                className="mode-toggle-link"
                onClick={p.onToggleReadingMode}
                title={isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              >
                {isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              </button>
            </div>
          </div>

          <div className="stepper-bar-container">
            {STEP_QUESTIONS.map(({ step }) => {
              const isDone = Boolean(completedSteps[step]);
              const isCur = currentStep === step;
              return (
                <button
                  type="button"
                  key={step}
                  className={`stepper-segment ${isDone ? 'done' : isCur ? 'current' : 'pending'}`}
                  aria-label={t('voiceProfile.stepTitle', { n: String(step) })}
                  aria-current={isCur ? 'step' : undefined}
                  disabled={isRecording || isProcessing}
                  onClick={() => p.onSelectStep(step)}
                  title={t('voiceProfile.stepTitle', { n: String(step) })}
                  style={{ background: 'transparent', border: 'none', padding: 0, font: 'inherit', color: 'inherit' }}
                >
                  <span className="segment-number">{isDone ? '✓' : step}</span>
                  <span className="segment-fill" style={{ display: 'block' }} />
                </button>
              );
            })}
          </div>

          <div className="prompt-display-card">
            <div className="prompt-mode-tag">
              {isReadingMode ? `📖 ${t('voiceProfile.readingMode')}` : `💬 ${t('voiceProfile.openQuestionsMode')}`}
            </div>
            <div className="prompt-main-text">
              {isReadingMode ? `"${t(currentQ.fallbackKey)}"` : `"${t(currentQ.textKey)}"`}
            </div>
            <div className="prompt-hint-sub">
              {isReadingMode ? t('voiceProfile.readingHint') : t('voiceProfile.openHint')}
            </div>
          </div>

          <VuMeter t={t} isRecording={isRecording} level={p.liveVoiceLevel} colorByLevel />

          {p.captureError && (
            <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 10 }}>
              {p.captureError}
            </div>
          )}
          {!isRecording && !isProcessing && p.feedback?.enrollErrorText && (
            <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 10, padding: '8px 12px', background: 'rgba(239, 68, 68, 0.12)', borderRadius: 6 }}>
              ⚠️ {p.feedback.enrollErrorText}
            </div>
          )}

          {!isRecording && !isProcessing && currentTake && (
            <div role="status" style={{ display: 'flex', alignItems: 'center', gap: 8, color: '#4ade80', background: 'rgba(34, 197, 94, 0.12)', border: '1px solid rgba(34, 197, 94, 0.3)', padding: '8px 14px', borderRadius: 6, marginBottom: 14, fontSize: '0.9rem', fontWeight: 600 }}>
              <span>✓</span>
              <span>{t('voiceProfile.sampleCompleted')}</span>
            </div>
          )}

          <div className="stepper-action-row">
            {isRecording ? (
              <RecordingIndicator t={t} locale={p.locale} elapsed={p.recordingElapsedSeconds} onStop={() => p.onFinishStep(currentStep)} showHint />
            ) : isProcessing ? (
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '8px 16px', background: 'rgba(59, 130, 246, 0.15)', borderRadius: 8, color: '#93c5fd' }}>
                <span style={{ fontSize: '1.1rem' }}>⏳</span>
                <span style={{ fontSize: '0.9rem', fontWeight: 500 }}>
                  {isBuilding ? t('voiceProfile.buildingProfile') : t('voiceProfile.sendingSample')}
                </span>
              </div>
            ) : currentTake ? (
              <div style={{ display: 'flex', gap: 12, alignItems: 'center', flexWrap: 'wrap' }}>
                <button className="action-btn" disabled={isProcessing || p.isStarting} onClick={() => p.onRedoStep(currentStep)}>
                  {t('voiceProfile.redoSample')}
                </button>
                {currentStep < GUIDED_STEP_COUNT && (
                  <button className="action-btn primary-next-btn" style={{ fontWeight: 700, padding: '10px 24px', fontSize: '0.95rem' }} disabled={isProcessing} onClick={p.onNextStep}>
                    {t('voiceProfile.nextStep')} ➔
                  </button>
                )}
              </div>
            ) : (
              <button className="record-btn-trigger" disabled={isProcessing || p.isStarting} onClick={() => p.onStartStep(currentStep)}>
                🎙️ {t('voiceProfile.recordSample')}
              </button>
            )}

            {isAllStepsCompleted && (
              <button
                className="activate-profile-master-btn"
                disabled={isProcessing || isRecording || isBuilding}
                onClick={p.onBuildProfile}
              >
                {isBuilding ? `⏳ ${t('voiceProfile.buildingProfile')}` : t('voiceProfile.activateProfileBtn')}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
