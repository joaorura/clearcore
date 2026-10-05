import React, { useEffect, useMemo, useState } from 'react';
import { useI18n } from './i18n';
import type { CallSuggestionTake, InputDeviceInfo } from './types';
import { errorLabel } from './voice/enrollmentErrors';
import { DevModelNotice } from './voice/DevModelNotice';
import { VoiceBudgetMeter } from './voice/VoiceBudgetMeter';
import { VoiceSampleGallery } from './voice/VoiceSampleGallery';
import { BudgetErrorBanner } from './voice/BudgetErrorBanner';
import { EnrollmentJobStatus } from './voice/EnrollmentJobStatus';
import { MAX_RECORD_SECONDS, MIN_RECORD_SECONDS } from './voice/enrollmentTypes';
import {
  buildEnrollmentLabels,
  formatSecondsForLocale,
  interpolateBudgetBody,
  voiceProfileErrorKey,
  voiceProfileStatusLabelKey,
} from './voice/hooks/voiceProfileLogic';
import { STEP_QUESTIONS } from './voice/hooks/guidedSteps';
import { useJobFeedback } from './voice/hooks/useJobFeedback';
import { useVoiceSamples } from './voice/hooks/useVoiceSamples';
import { useEnrollmentRecorder } from './voice/hooks/useEnrollmentRecorder';
import { useAudioPlayback } from './voice/hooks/useAudioPlayback';
import { useProfileBuild } from './voice/hooks/useProfileBuild';
import { useCallTakes } from './voice/hooks/useCallTakes';
import { useGuidedSteps } from './voice/hooks/useGuidedSteps';
import { useVoluntarySample } from './voice/hooks/useVoluntarySample';
import { useFlashMessage } from './voice/hooks/useFlashMessage';

export interface VoiceProfileCardProps {
  selectedInputId?: string;
  virtualMicPresent?: boolean;
  inputDevices?: InputDeviceInfo[];
}

// Capture helpers live in ./voice/captureDevice; re-exported so existing imports keep working.
export { isVirtualOrLoopbackAudioDevice, resolvePhysicalAudioDevice } from './voice/captureDevice';
// Pure card logic lives in ./voice/hooks/voiceProfileLogic; re-exported for the same reason.
export {
  normalizeVoiceProfileStatus,
  voiceProfileStatusLabelKey,
  voiceProfileActivationState,
  mergeVoiceProfileStatus,
  stripServiceVoiceProfileKeys,
  applySetVoiceProfileResult,
  voiceProfileErrorKey,
  buildEnrollmentLabels,
  nextStepAfterJob,
  shouldOpenGalleryOnError,
  shouldShowTakeError,
  formatSecondsForLocale,
  interpolateBudgetBody,
} from './voice/hooks/voiceProfileLogic';
export type {
  VoiceProfileActivationState,
  VoiceProfileStatusLabelKey,
  VoiceProfileErrorKey,
  JobOutcome,
} from './voice/hooks/voiceProfileLogic';

const MIN_RECORDING_SECONDS = MIN_RECORD_SECONDS;
const MAX_RECORDING_SECONDS = MAX_RECORD_SECONDS;

export const VoiceProfileCard: React.FC<VoiceProfileCardProps> = ({
  selectedInputId,
  virtualMicPresent: _virtualMicPresent,
  inputDevices,
}) => {
  const { t, locale } = useI18n();

  // Active Tab: 'samples' (Galeria Cumulativa) | 'intake' (Sugestões de Chamadas)
  const [activeTab, setActiveTab] = useState<'samples' | 'intake'>('samples');

  const labels = useMemo(() => buildEnrollmentLabels(t), [t]);
  const { message: feedbackMessage, flash } = useFlashMessage();
  const jobs = useJobFeedback(t, labels, () => setActiveTab('samples'));
  const { jobBusy, currentJob, enrollErrorText, budgetError } = jobs;
  const { sampleList, samples, samplesLoadFailed, deletingId, refreshSamples, removeSample } = useVoiceSamples();
  const rec = useEnrollmentRecorder({ selectedInputId, inputDevices, t, jobs, refreshSamples });
  const { isRecording, recordingElapsedSeconds, liveVoiceLevel, captureError } = rec;
  const playback = useAudioPlayback();
  const { playingAudioId, playCaptured } = playback;
  const profile = useProfileBuild({ t, labels, jobs, refreshSamples });
  const { profileStatus } = profile;
  const takes = useCallTakes();
  const { callTakes, refreshCallTakes } = takes;
  const steps = useGuidedSteps({ recorder: rec, t, flash });
  const { currentStep, setCurrentStep, isReadingMode, completedSteps } = steps;
  const modal = useVoluntarySample({ recorder: rec, playback, jobs, t, samplesCount: samples.length, flash });
  const { isModalOpen, modalSampleName, setModalSampleName, modalCaptured } = modal;

  const bannerLabels = useMemo(
    () => ({ ...labels, budgetExceededBody: interpolateBudgetBody(labels.budgetExceededBody, budgetError?.remainingSeconds ?? null, locale) }),
    [labels, budgetError, locale],
  );

  // Initial state: samples, takes and profile status from the service.
  useEffect(() => {
    const initVoiceData = async () => {
      const list = await refreshSamples();
      await refreshCallTakes();
      const initialProfile = await profile.loadInitialStatus(list?.samples.length ?? 0);
      if (initialProfile.is_enrolled) setCurrentStep(5);
    };
    void initVoiceData();
  }, [refreshSamples, refreshCallTakes]);

  const handleStartStepRecording = (stepNum: number) => void steps.startStep(stepNum);
  const finishStepRecording = (stepNum: number) => void steps.finishStep(stepNum);
  const handleRedoStep = steps.redoStep;
  const handleNextStep = steps.nextStep;
  const handleToggleReadingMode = steps.toggleReadingMode;
  const handleResetEnrollment = steps.resetSteps;
  const handlePlayAudio = playback.playUrl;
  const handleStartModalRecording = () => void modal.startRecording();
  const finishModalRecording = () => void modal.finishRecording();
  const handleSaveModalSample = modal.save;

  const handleBuildProfile = async () => {
    if (await profile.buildProfile()) flash(t('voiceProfile.profileActivatedSuccess'), 5000);
  };

  const handleDeleteSample = async (id: string) => {
    const list = await removeSample(id);
    if (list) jobs.setBudgetError(null);
  };

  const handleApproveCallTake = async (take: CallSuggestionTake) => {
    jobs.setBudgetError(null);
    jobs.setEnrollErrorText(null);
    const outcome = await takes.approveTake(take);
    if (outcome.kind === 'budget') jobs.showBudgetError(outcome.remainingSeconds);
    else if (outcome.kind === 'error') jobs.showError('calls', errorLabel(outcome.code, labels));
    else flash(t('voiceProfile.takeApprovedFeedback'), 4000);
    await Promise.all([refreshSamples(), refreshCallTakes()]);
  };

  const handleDismissCallTake = async (id: string) => {
    await takes.dismissTake(id);
    flash(t('voiceProfile.takeDismissedFeedback'), 3000);
  };

  const completedCount = Object.keys(completedSteps).length;
  const isAllStepsCompleted = completedCount >= 5;
  const currentQ = STEP_QUESTIONS[currentStep - 1];


  const statusLabelKey = voiceProfileStatusLabelKey(profileStatus);
  const statusLabelActive = statusLabelKey === 'active';
  const statusLabel =
    statusLabelKey === 'active'
      ? t('voiceProfile.statusActive')
      : statusLabelKey === 'storedNotApplied'
        ? t('voiceProfile.storedNotApplied')
        : statusLabelKey === 'enrolledUnconfirmed'
          ? t('voiceProfile.enrolledUnconfirmed')
          : t('voiceProfile.statusPending');

  return (
    <div className="card voice-profile-card">
      {/* Header & Status Section */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 12, marginBottom: 16 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <h2 className="card-title" style={{ margin: 0 }}>{t('voiceProfile.title')}</h2>
            <span className="profile-badge-ecapa">
              {t('voiceProfile.badge')}
            </span>
            <span className={`status-pill ${statusLabelActive ? 'pill-active' : 'pill-pending'}`}>
              {statusLabel}
            </span>
          </div>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 6, lineHeight: 1.45 }}>
            {t('voiceProfile.description')}
          </p>
        </div>

        <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
          {samples.length > 0 && (
            <button
              className="action-btn"
              disabled={jobBusy || isRecording}
              onClick={() => void handleBuildProfile()}
              style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            >
              {t('voiceProfile.rebuildProfileBtn')}
            </button>
          )}
          {profileStatus.is_enrolled && (
            <button
              className="action-btn"
              onClick={handleResetEnrollment}
              style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            >
              {t('voiceProfile.reEnrollBtn')}
            </button>
          )}
        </div>
      </div>

      <div style={{ marginBottom: 12 }}>
        <DevModelNotice labels={labels} />
      </div>

      {feedbackMessage && (
        <div className="feedback-banner success-banner">
          {feedbackMessage}
        </div>
      )}

      {/* Service job feedback (outside the modal) */}
      {!isModalOpen && (jobBusy || currentJob || enrollErrorText) && (
        <div style={{ marginBottom: 12 }}>
          {jobBusy && !currentJob && (
            <div role="status" style={{ fontSize: 13, color: 'var(--text-muted)' }}>{t('voiceProfile.sendingSample')}</div>
          )}
          <EnrollmentJobStatus job={currentJob} labels={labels} lang={locale} />
          {enrollErrorText && (
            <div role="alert" style={{ color: '#f87171', fontSize: 13, marginTop: 4 }}>{enrollErrorText}</div>
          )}
        </div>
      )}

      {/* Profile Overview Bar */}
      <div className="profile-overview-box">
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.statusTitle')}</div>
          <div className="overview-value" style={{ color: statusLabelActive ? '#4ade80' : '#fbbf24' }}>
            {statusLabel}
          </div>
          {statusLabelKey === 'active' && (
            <div style={{ color: 'var(--text-muted)', fontSize: '0.75rem', marginTop: 4 }}>
              {t('voiceProfile.appliedInServiceNote')}
            </div>
          )}
          {profileStatus.voice_profile_error && statusLabelKey !== 'active' && (
            <div style={{ color: '#fbbf24', fontSize: '0.75rem', marginTop: 4 }}>
              {t(`voiceProfile.${voiceProfileErrorKey(profileStatus.voice_profile_error)}`)}
            </div>
          )}
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.samplesRegisteredTitle')}</div>
          <div className="overview-value">
            {t('voiceProfile.statusSamplesPill', { count: String(sampleList ? samples.length : profileStatus.active_samples_count) })}
          </div>
        </div>
        <div className="overview-metric">
          <div className="overview-label">{t('voiceProfile.neuralEqStatusTitle')}</div>
          <div className="overview-value" style={{ color: profileStatus.neural_eq_calibrated ? '#4ade80' : 'var(--text-muted)' }}>
            {profileStatus.neural_eq_calibrated ? t('voiceProfile.neuralEqCalibrated') : t('voiceProfile.neuralEqPending')}
          </div>
        </div>
      </div>

      {/* Guided Multi-Sampling Stepper (If not enrolled or re-enrolling) */}
      {(!profileStatus.is_enrolled || completedCount < 5) && (
        <div className="guided-sampling-section">
          {/* Progress Header */}
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 10, flexWrap: 'wrap', gap: 8 }}>
            <div style={{ fontWeight: 600, fontSize: '0.95rem', color: '#93c5fd' }}>
              {t('voiceProfile.stepProgress', { current: String(currentStep), total: '5' })}: {t(currentQ.categoryKey)}
            </div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
              <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {t('voiceProfile.stepPercent', { percent: String(Math.round((completedCount / 5) * 100)) })}
              </span>
              <button
                className="mode-toggle-link"
                onClick={handleToggleReadingMode}
                title={isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              >
                {isReadingMode ? t('voiceProfile.toggleModeBack') : t('voiceProfile.toggleModePrompt')}
              </button>
            </div>
          </div>

          {/* Stepper Progress Bar (1/5 to 5/5) */}
          <div className="stepper-bar-container">
            {[1, 2, 3, 4, 5].map((stepIdx) => {
              const isDone = Boolean(completedSteps[stepIdx]);
              const isCur = currentStep === stepIdx;
              return (
                <div
                  key={stepIdx}
                  className={`stepper-segment ${isDone ? 'done' : isCur ? 'current' : 'pending'}`}
                  onClick={() => setCurrentStep(stepIdx)}
                  title={t('voiceProfile.stepTitle', { n: String(stepIdx) })}
                >
                  <div className="segment-number">{isDone ? '✓' : stepIdx}</div>
                  <div className="segment-fill" />
                </div>
              );
            })}
          </div>

          {/* Prompt Box */}
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

          {/* Live Dynamic VU Meter */}
          <div className="vu-meter-panel">
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
              <span>{t('voiceProfile.voiceLevel')}</span>
              <span>{isRecording ? `${liveVoiceLevel}%` : '0%'}</span>
            </div>
            <div className="vu-meter-track">
              <div
                className="vu-meter-bar"
                style={{
                  width: `${isRecording ? liveVoiceLevel : 0}%`,
                  background:
                    liveVoiceLevel > 80
                      ? 'linear-gradient(90deg, #22c55e, #eab308, #ef4444)'
                      : liveVoiceLevel > 40
                      ? 'linear-gradient(90deg, #22c55e, #38bdf8)'
                      : '#22c55e',
                }}
              />
            </div>
          </div>

          {captureError && !isModalOpen && (
            <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 10 }}>
              {captureError}
            </div>
          )}

          {/* Action Row for the Step */}
          <div className="stepper-action-row">
            {!isRecording ? (
              completedSteps[currentStep] ? (
                <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
                  <button
                    className="action-btn play-sample-btn"
                    disabled={!completedSteps[currentStep].captured}
                    onClick={() => {
                      const c = completedSteps[currentStep].captured;
                      if (c) playCaptured(`step-${currentStep}`, c);
                    }}
                  >
                    {playingAudioId === `step-${currentStep}` ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                  </button>
                  <button
                    className="action-btn"
                    disabled={jobBusy}
                    onClick={() => handleRedoStep(currentStep)}
                  >
                    {t('voiceProfile.redoSample')}
                  </button>
                  {currentStep < 5 && (
                    <button
                      className="action-btn primary-next-btn"
                      onClick={handleNextStep}
                    >
                      {t('voiceProfile.nextStep')}
                    </button>
                  )}
                </div>
              ) : (
                <button
                  className="record-btn-trigger"
                  disabled={jobBusy}
                  onClick={() => handleStartStepRecording(currentStep)}
                >
                  🎙️ {t('voiceProfile.recordSample')}
                </button>
              )
            ) : (
              <div className="recording-active-container">
                <div className="recording-status-group">
                  <span className="recording-pulsing-dot" />
                  <span style={{ fontWeight: 600, color: '#f87171' }}>
                    {t('voiceProfile.recordingStatus', {
                      elapsed: formatSecondsForLocale(recordingElapsedSeconds, locale),
                      max: String(MAX_RECORDING_SECONDS),
                    })}
                  </span>
                  <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                    ({t('voiceProfile.recordingHint')})
                  </span>
                </div>
                <button
                  type="button"
                  className="stop-record-btn"
                  disabled={recordingElapsedSeconds < MIN_RECORDING_SECONDS}
                  onClick={() => finishStepRecording(currentStep)}
                  title={
                    recordingElapsedSeconds < MIN_RECORDING_SECONDS
                      ? t('voiceProfile.minRecordingTitle', { min: formatSecondsForLocale(MIN_RECORDING_SECONDS, locale) })
                      : t('voiceProfile.stopRecordingBtn')
                  }
                >
                  ⏹️ {t('voiceProfile.stopRecordingBtn')}
                </button>
              </div>
            )}

            {isAllStepsCompleted && (
              <button
                className="activate-profile-master-btn"
                disabled={jobBusy || isRecording}
                onClick={() => void handleBuildProfile()}
              >
                {t('voiceProfile.activateProfileBtn')}
              </button>
            )}
          </div>
        </div>
      )}

      {/* Tabs Header: Cumulative Gallery vs Call Suggestions */}
      <div className="profile-subtabs-nav">
        <button
          className={`subtab-btn ${activeTab === 'samples' ? 'subtab-active' : ''}`}
          onClick={() => setActiveTab('samples')}
        >
          {t('voiceProfile.tabSamples', { count: String(samples.length) })}
        </button>
        <button
          className={`subtab-btn ${activeTab === 'intake' ? 'subtab-active' : ''}`}
          onClick={() => setActiveTab('intake')}
        >
          {t('voiceProfile.tabCallSuggestions', { count: String(callTakes.length) })}
          {callTakes.length > 0 && <span className="tab-count-badge">{callTakes.length}</span>}
        </button>
      </div>

      {/* Tab 1: Cumulative Samples Gallery */}
      {activeTab === 'samples' && (
        <div className="tab-pane-content">
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0 }}>
              {t('voiceProfile.sampleGalleryDesc')}
            </p>
            <button
              className="action-btn add-sample-accent-btn"
              onClick={modal.openModal}
            >
              {t('voiceProfile.addNewSampleBtn')}
            </button>
          </div>

          {sampleList && (
            <div style={{ marginBottom: 12 }}>
              <VoiceBudgetMeter budget={sampleList.budget} labels={labels} lang={locale} />
            </div>
          )}

          {budgetError && (
            <div style={{ marginBottom: 12 }}>
              <BudgetErrorBanner remainingSeconds={budgetError.remainingSeconds} labels={bannerLabels} />
            </div>
          )}

          {samplesLoadFailed && (
            <div role="alert" style={{ color: '#fbbf24', fontSize: 13, marginBottom: 12 }}>
              {t('voiceProfile.samplesLoadFailed')}
            </div>
          )}

          {sampleList && samples.length > 0 ? (
            <VoiceSampleGallery
              samples={samples}
              budget={sampleList.budget}
              labels={labels}
              onDelete={(id) => void handleDeleteSample(id)}
              deletingId={deletingId}
              highlightDelete={budgetError !== null}
              lang={locale}
            />
          ) : (
            <div className="empty-state-card">
              {t('voiceProfile.emptyGallery')}
            </div>
          )}
        </div>
      )}

      {/* Tab 2: Call Suggestions (Voice Intake Engine) */}
      {activeTab === 'intake' && (
        <div className="tab-pane-content">
          <div style={{ marginBottom: 14 }}>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0, lineHeight: 1.45 }}>
              {t('voiceProfile.callSuggestionsDesc')}
            </p>
          </div>

          {callTakes.length > 0 ? (
            <div className="intake-takes-list">
              {callTakes.map((take) => (
                <div key={take.id} className="intake-take-card">
                  <div className="intake-take-info">
                    <div className="take-title-row">
                      <span className="take-badge-live">{t('voiceProfile.takeBadge')}</span>
                      {take.title && <strong style={{ fontSize: '0.95rem' }}>{take.title}</strong>}
                    </div>
                    <div className="take-meta-row">
                      <span>🕒 {take.timestamp}</span>
                      {typeof take.speech_seconds === 'number' && (
                        <span>⏱ {t('voiceProfile.takeSpeech', { sec: formatSecondsForLocale(take.speech_seconds, locale) })}</span>
                      )}
                      {typeof take.speech_seconds !== 'number' && typeof take.durationSec === 'number' && (
                        <span>⏱ {t('voiceProfile.takeDuration', { sec: formatSecondsForLocale(take.durationSec, locale) })}</span>
                      )}
                      {typeof take.snrDb === 'number' && (
                        <span className="take-snr-badge">{t('voiceProfile.takeSnr', { snr: formatSecondsForLocale(take.snrDb, locale) })}</span>
                      )}
                      {take.device_label && <span>🎙️ {take.device_label}</span>}
                    </div>
                  </div>

                  <div className="intake-take-actions">
                    {take.audioUrl && (
                      <button
                        className="action-btn take-play-btn"
                        onClick={() => handlePlayAudio(take.id, take.audioUrl)}
                      >
                        {playingAudioId === take.id ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                      </button>
                    )}
                    <button
                      className="action-btn take-approve-btn"
                      onClick={() => void handleApproveCallTake(take)}
                    >
                      {t('voiceProfile.approveTake')}
                    </button>
                    <button
                      className="action-btn take-dismiss-btn"
                      onClick={() => void handleDismissCallTake(take.id)}
                    >
                      {t('voiceProfile.dismissTake')}
                    </button>
                  </div>
                </div>
              ))}
            </div>
          ) : (
            <div className="empty-state-card">
              {t('voiceProfile.emptyCallSuggestions')}
            </div>
          )}
        </div>
      )}

      {/* Modal: "+ Adicionar Nova Amostra" */}
      {isModalOpen && (
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
                value={modalSampleName}
                onChange={(e) => setModalSampleName(e.target.value)}
              />
            </div>

            {/* VU Meter inside modal */}
            <div className="vu-meter-panel" style={{ marginBottom: 16 }}>
              <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 4, color: 'var(--text-muted)' }}>
                <span>{t('voiceProfile.voiceLevel')}</span>
                <span>{isRecording ? `${liveVoiceLevel}%` : '0%'}</span>
              </div>
              <div className="vu-meter-track">
                <div
                  className="vu-meter-bar"
                  style={{ width: `${isRecording ? liveVoiceLevel : 0}%`, background: '#22c55e' }}
                />
              </div>
            </div>

            {captureError && (
              <div role="alert" className="feedback-banner" style={{ color: '#f87171', marginBottom: 12 }}>
                {captureError}
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'center', marginBottom: 16 }}>
              {!isRecording ? (
                <button
                  className="record-btn-trigger"
                  disabled={jobBusy}
                  onClick={handleStartModalRecording}
                >
                  🎙️ {modalCaptured ? t('voiceProfile.redoSample') : t('voiceProfile.recordSample')}
                </button>
              ) : (
                <div className="recording-active-container">
                  <div className="recording-status-group">
                    <span className="recording-pulsing-dot" />
                    <span style={{ fontWeight: 600, color: '#f87171' }}>
                      {t('voiceProfile.recordingStatus', {
                        elapsed: formatSecondsForLocale(recordingElapsedSeconds, locale),
                        max: String(MAX_RECORDING_SECONDS),
                      })}
                    </span>
                  </div>
                  <button
                    type="button"
                    className="stop-record-btn"
                    disabled={recordingElapsedSeconds < MIN_RECORDING_SECONDS}
                    onClick={finishModalRecording}
                    title={
                      recordingElapsedSeconds < MIN_RECORDING_SECONDS
                        ? t('voiceProfile.minRecordingTitle', { min: formatSecondsForLocale(MIN_RECORDING_SECONDS, locale) })
                        : t('voiceProfile.stopRecordingBtn')
                    }
                  >
                    ⏹️ {t('voiceProfile.stopRecordingBtn')}
                  </button>
                </div>
              )}
            </div>

            {modalCaptured && !isRecording && (
              <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 10, marginBottom: 16 }}>
                <button
                  className="action-btn"
                  onClick={() => playCaptured('modal-preview', modalCaptured)}
                >
                  {playingAudioId === 'modal-preview' ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                </button>
                <span style={{ fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                  {t('voiceProfile.recordedDuration', { sec: formatSecondsForLocale(modalCaptured.durationSec, locale) })}
                </span>
              </div>
            )}

            {(jobBusy || currentJob || enrollErrorText) && (
              <div style={{ marginBottom: 12 }}>
                {jobBusy && !currentJob && (
                  <div role="status" style={{ fontSize: 13, color: 'var(--text-muted)' }}>{t('voiceProfile.sendingSample')}</div>
                )}
                <EnrollmentJobStatus job={currentJob} labels={labels} lang={locale} />
                {enrollErrorText && (
                  <div role="alert" style={{ color: '#f87171', fontSize: 13, marginTop: 4 }}>{enrollErrorText}</div>
                )}
              </div>
            )}

            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 12 }}>
              <button
                className="action-btn"
                onClick={modal.cancelModal}
              >
                {t('voiceProfile.modalCancel')}
              </button>
              <button
                className="action-btn primary-next-btn"
                disabled={!modalCaptured || jobBusy || isRecording}
                onClick={() => void handleSaveModalSample()}
              >
                {t('voiceProfile.modalSave')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
