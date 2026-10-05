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
  /** The microphone is being opened: recording cannot be started again. */
  isStarting?: boolean;
  playingAudioId: string | null;
  /** Feedback of the sample job started here (null when another tab started the job). */
  feedback: JobFeedbackView | null;
  onSelectStep: (step: number) => void;
  onToggleReadingMode: () => void;
  onStartStep: (step: number) => void;
  onFinishStep: (step: number) => void;
  onRedoStep: (step: number) => void;
  onNextStep: () => void;
  onPlayStep: (step: number) => void;
  onBuildProfile: () => void;
  onResetEnrollment: () => void;
}

/** Guided 5-step enrollment: question, live level, record/stop, take preview and job feedback. */
export function EnrollPanel(p: EnrollPanelProps) {
  const { t, currentStep, completedSteps, isReadingMode, isRecording } = p;
  const completedCount = Object.keys(completedSteps).length;
  const isAllStepsCompleted = completedCount >= GUIDED_STEP_COUNT;
  const currentQ = STEP_QUESTIONS[currentStep - 1] ?? STEP_QUESTIONS[0];
  const currentTake = completedSteps[currentStep];
  const showFlow = !p.isEnrolled || completedCount < GUIDED_STEP_COUNT;

  return (
    <div>
      {p.isEnrolled && (
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 12 }}>
          <button className="action-btn" onClick={p.onResetEnrollment} style={{ fontSize: '0.8rem', padding: '6px 12px' }}>
            {t('voiceProfile.reEnrollBtn')}
          </button>
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
                  disabled={isRecording}
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

          <div className="stepper-action-row">
            {isRecording ? (
              <RecordingIndicator t={t} locale={p.locale} elapsed={p.recordingElapsedSeconds} onStop={() => p.onFinishStep(currentStep)} showHint />
            ) : currentTake ? (
              <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
                <button
                  className="action-btn play-sample-btn"
                  disabled={!currentTake.captured}
                  onClick={() => p.onPlayStep(currentStep)}
                >
                  {p.playingAudioId === `step-${currentStep}` ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                </button>
                <button className="action-btn" disabled={p.jobBusy || p.isStarting} onClick={() => p.onRedoStep(currentStep)}>
                  {t('voiceProfile.redoSample')}
                </button>
                {currentStep < GUIDED_STEP_COUNT && (
                  <button className="action-btn primary-next-btn" onClick={p.onNextStep}>
                    {t('voiceProfile.nextStep')}
                  </button>
                )}
              </div>
            ) : (
              <button className="record-btn-trigger" disabled={p.jobBusy || p.isStarting} onClick={() => p.onStartStep(currentStep)}>
                🎙️ {t('voiceProfile.recordSample')}
              </button>
            )}

            {isAllStepsCompleted && (
              <button className="activate-profile-master-btn" disabled={p.jobBusy || isRecording} onClick={p.onBuildProfile}>
                {t('voiceProfile.activateProfileBtn')}
              </button>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
