import React, { useEffect, useMemo, useState } from 'react';
import { useI18n } from './i18n';
import type { CallSuggestionTake, InputDeviceInfo } from './types';
import { errorLabel } from './voice/enrollmentErrors';
import { buildEnrollmentLabels } from './voice/hooks/voiceProfileLogic';
import { useJobFeedback } from './voice/hooks/useJobFeedback';
import { useVoiceSamples } from './voice/hooks/useVoiceSamples';
import { useEnrollmentRecorder } from './voice/hooks/useEnrollmentRecorder';
import { useAudioPlayback } from './voice/hooks/useAudioPlayback';
import { useProfileBuild } from './voice/hooks/useProfileBuild';
import { useCallTakes } from './voice/hooks/useCallTakes';
import { useGuidedSteps } from './voice/hooks/useGuidedSteps';
import { useVoluntarySample } from './voice/hooks/useVoluntarySample';
import { useFlashMessage } from './voice/hooks/useFlashMessage';
import { EnrollPanel } from './voice/panels/EnrollPanel';
import { GalleryPanel } from './voice/panels/GalleryPanel';
import { CallsPanel } from './voice/panels/CallsPanel';
import { ProfilePanel } from './voice/panels/ProfilePanel';
import { AddSampleModal } from './voice/panels/AddSampleModal';
import { JobFeedbackBlock } from './voice/panels/shared';
import { profileStatusLabel } from './voice/panels/profileStatusLabel';

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

  const status = profileStatusLabel(profileStatus, t);
  const feedbackView = { jobBusy, currentJob, enrollErrorText };

  return (
    <div className="card voice-profile-card">
      <div style={{ marginBottom: 16 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
          <h2 className="card-title" style={{ margin: 0 }}>{t('voiceProfile.title')}</h2>
          <span className="profile-badge-ecapa">{t('voiceProfile.badge')}</span>
          <span className={`status-pill ${status.active ? 'pill-active' : 'pill-pending'}`}>{status.text}</span>
        </div>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 6, lineHeight: 1.45 }}>
          {t('voiceProfile.description')}
        </p>
      </div>

      {feedbackMessage && <div className="feedback-banner success-banner">{feedbackMessage}</div>}

      {!isModalOpen && <JobFeedbackBlock feedback={feedbackView} labels={labels} t={t} locale={locale} />}

      <ProfilePanel
        t={t}
        locale={locale}
        labels={labels}
        profileStatus={profileStatus}
        samplesCount={sampleList ? samples.length : profileStatus.active_samples_count}
        canBuild={samples.length > 0}
        busy={jobBusy || isRecording}
        feedback={null}
        onBuildProfile={() => void handleBuildProfile()}
      />

      <EnrollPanel
        t={t}
        locale={locale}
        labels={labels}
        isEnrolled={profileStatus.is_enrolled}
        currentStep={currentStep}
        isReadingMode={isReadingMode}
        completedSteps={completedSteps}
        isRecording={isRecording}
        liveVoiceLevel={liveVoiceLevel}
        recordingElapsedSeconds={recordingElapsedSeconds}
        captureError={isModalOpen ? null : captureError}
        jobBusy={jobBusy}
        playingAudioId={playingAudioId}
        feedback={null}
        onSelectStep={setCurrentStep}
        onToggleReadingMode={steps.toggleReadingMode}
        onStartStep={(step) => void steps.startStep(step)}
        onFinishStep={(step) => void steps.finishStep(step)}
        onRedoStep={steps.redoStep}
        onNextStep={steps.nextStep}
        onPlayStep={(step) => {
          const c = completedSteps[step]?.captured;
          if (c) playCaptured(`step-${step}`, c);
        }}
        onBuildProfile={() => void handleBuildProfile()}
        onResetEnrollment={steps.resetSteps}
      />

      <div className="profile-subtabs-nav">
        <button className={`subtab-btn ${activeTab === 'samples' ? 'subtab-active' : ''}`} onClick={() => setActiveTab('samples')}>
          {t('voiceProfile.tabSamples', { count: String(samples.length) })}
        </button>
        <button className={`subtab-btn ${activeTab === 'intake' ? 'subtab-active' : ''}`} onClick={() => setActiveTab('intake')}>
          {t('voiceProfile.tabCallSuggestions', { count: String(callTakes.length) })}
          {callTakes.length > 0 && <span className="tab-count-badge">{callTakes.length}</span>}
        </button>
      </div>

      <div className="tab-pane-content">
        {activeTab === 'samples' ? (
          <GalleryPanel
            t={t}
            locale={locale}
            labels={labels}
            sampleList={sampleList}
            samplesLoadFailed={samplesLoadFailed}
            deletingId={deletingId}
            budgetError={budgetError}
            feedback={null}
            onDelete={(id) => void handleDeleteSample(id)}
            onAddSample={modal.openModal}
          />
        ) : (
          <CallsPanel
            t={t}
            locale={locale}
            takes={callTakes}
            playingAudioId={playingAudioId}
            errorText={null}
            onPlay={playback.playUrl}
            onApprove={(take) => void handleApproveCallTake(take)}
            onDismiss={(id) => void handleDismissCallTake(id)}
          />
        )}
      </div>

      {isModalOpen && (
        <AddSampleModal
          t={t}
          locale={locale}
          labels={labels}
          name={modalSampleName}
          onNameChange={setModalSampleName}
          captured={modalCaptured}
          isRecording={isRecording}
          liveVoiceLevel={liveVoiceLevel}
          recordingElapsedSeconds={recordingElapsedSeconds}
          captureError={captureError}
          jobBusy={jobBusy}
          feedback={feedbackView}
          playingAudioId={playingAudioId}
          onStart={() => void modal.startRecording()}
          onStop={() => void modal.finishRecording()}
          onPlayPreview={() => { if (modalCaptured) playCaptured('modal-preview', modalCaptured); }}
          onCancel={modal.cancelModal}
          onSave={() => void modal.save()}
        />
      )}
    </div>
  );
};
