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
import { profileStatusLabel } from './voice/panels/profileStatusLabel';
import {
  browserStorage,
  loadVoiceTab,
  parseVoiceTab,
  saveVoiceTab,
  showsJobFeedback,
  tabAfterError,
  voiceTabBadges,
  type VoiceTabId,
} from './voice/panels/voiceTabs';
import { Tabs, type TabDef } from './voice/Tabs';

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

  // Active tab: a UI preference kept in localStorage (validated against the tab ids).
  const [activeTab, setActiveTab] = useState<VoiceTabId>(() => loadVoiceTab(browserStorage()));
  const selectTab = (id: string) => {
    const tab = parseVoiceTab(id);
    setActiveTab(tab);
    saveVoiceTab(tab, browserStorage());
  };

  const labels = useMemo(() => buildEnrollmentLabels(t), [t]);
  const { message: feedbackMessage, flash } = useFlashMessage();
  // ENROLL_BUDGET_EXCEEDED opens the gallery, where the delete action is highlighted.
  const jobs = useJobFeedback(t, labels, () => selectTab(tabAfterError(activeTab, 'ENROLL_BUDGET_EXCEEDED')));
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
  const feedbackFor = (tab: VoiceTabId) => (showsJobFeedback(tab, jobs.origin) ? feedbackView : null);
  const badges = voiceTabBadges(samples.length, callTakes.length);
  // While the microphone records, the other tabs stay closed so the stop button stays in view.
  const tabs: TabDef[] = [
    { id: 'enroll', label: t('voiceProfile.tabEnroll') },
    { id: 'gallery', label: t('voiceProfile.tabGallery'), badge: badges.gallery },
    { id: 'calls', label: t('voiceProfile.tabCalls'), badge: badges.calls },
    { id: 'profile', label: t('voiceProfile.tabProfile') },
  ].map((tab) => ({ ...tab, disabled: isRecording && tab.id !== activeTab }));

  const renderPanel = (id: string) => {
    switch (parseVoiceTab(id)) {
      case 'gallery':
        return (
          <GalleryPanel
            t={t}
            locale={locale}
            labels={labels}
            sampleList={sampleList}
            samplesLoadFailed={samplesLoadFailed}
            deletingId={deletingId}
            budgetError={budgetError}
            feedback={isModalOpen ? null : feedbackFor('gallery')}
            onDelete={(sampleId) => void handleDeleteSample(sampleId)}
            onAddSample={modal.openModal}
          />
        );
      case 'calls':
        return (
          <CallsPanel
            t={t}
            locale={locale}
            takes={callTakes}
            playingAudioId={playingAudioId}
            errorText={jobs.origin === 'calls' ? enrollErrorText : null}
            onPlay={playback.playUrl}
            onApprove={(take) => void handleApproveCallTake(take)}
            onDismiss={(takeId) => void handleDismissCallTake(takeId)}
          />
        );
      case 'profile':
        return (
          <ProfilePanel
            t={t}
            locale={locale}
            labels={labels}
            profileStatus={profileStatus}
            samplesCount={sampleList ? samples.length : profileStatus.active_samples_count}
            canBuild={samples.length > 0}
            busy={jobBusy || isRecording}
            feedback={feedbackFor('profile')}
            onBuildProfile={() => void handleBuildProfile()}
          />
        );
      default:
        return (
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
            feedback={isModalOpen ? null : feedbackFor('enroll')}
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
        );
    }
  };

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

      <Tabs tabs={tabs} activeId={activeTab} onChange={selectTab} renderPanel={renderPanel} ariaLabel={t('voiceProfile.tabsAriaLabel')} idPrefix="voice-profile" />

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
