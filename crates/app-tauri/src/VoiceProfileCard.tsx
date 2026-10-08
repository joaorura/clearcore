import React, { useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from './i18n';
import { logger } from './utils/logger';
import type { CallSuggestionTake, InputDeviceInfo } from './types';
import { errorLabel } from './voice/enrollmentErrors';
import { buildEnrollmentLabels, hasServiceVoiceProfile, profileSampleIds, samplesUsedInProfile, showStaleProfileNotice } from './voice/hooks/voiceProfileLogic';
import { stepAfterSelect } from './voice/hooks/guidedSteps';
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
import { DeviceSwitchDialog, type DeviceSwitchInfo } from './voice/panels/DeviceSwitchDialog';
import { useConfirmDialog } from './voice/hooks/useConfirmDialog';
import { confirmDeviceSwitchBeforeSend } from './voice/deviceSwitch';
import { purgeLegacyVoiceKeys, takeLegacyNotice } from './voice/legacyMigration';
import { profileStatusLabel } from './voice/panels/profileStatusLabel';
import {
  browserStorage,
  loadVoiceTab,
  jobFeedbackFor,
  parseVoiceTab,
  resolveTabChange,
  saveVoiceTab,
  voiceTabBadges,
  type TabChange,
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
  hasServiceVoiceProfile,
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
  const changeTab = (change: TabChange) => {
    const { tab, persist } = resolveTabChange(activeTab, change);
    logger.info('VOICE_UI', `Voice profile tab changed to '${tab}' (reason: ${change.by})`);
    setActiveTab(tab);
    if (persist) saveVoiceTab(tab, browserStorage());
  };

  const labels = useMemo(() => buildEnrollmentLabels(t), [t]);
  const { message: feedbackMessage, flash } = useFlashMessage();
  // ENROLL_BUDGET_EXCEEDED opens the gallery, where the delete action is highlighted.
  const jobs = useJobFeedback(t, labels, () => changeTab({ by: 'budget-error' }), locale);
  const { jobBusy, currentJob, enrollErrorText, budgetError } = jobs;
  const { sampleList, samples, samplesLoadFailed, deletingId, refreshSamples, removeSample } = useVoiceSamples();
  // Device switch (spec §4.4): asked in the card before a take from another microphone is sent.
  const deviceSwitch = useConfirmDialog<DeviceSwitchInfo>();
  const sampleListRef = useRef(sampleList);
  sampleListRef.current = sampleList;
  const beforeSend = (captured: Parameters<typeof confirmDeviceSwitchBeforeSend>[1]) => {
    const list = sampleListRef.current;
    return confirmDeviceSwitchBeforeSend(
      list?.selectedDeviceIdHash ?? null,
      captured,
      () => deviceSwitch.ask({ newLabel: captured.device.label, oldLabel: list?.selectedDeviceLabel ?? '' }),
      list?.selectedDeviceLabel ?? null,
    );
  };
  const rec = useEnrollmentRecorder({ selectedInputId, inputDevices, t, jobs, refreshSamples, beforeSend });
  const { isRecording, isStarting, recordingElapsedSeconds, liveVoiceLevel, captureError } = rec;
  const playback = useAudioPlayback();
  const { playingAudioId, playCaptured } = playback;
  const profile = useProfileBuild({ t, labels, jobs, refreshSamples });
  const { profileStatus } = profile;
  const takes = useCallTakes();
  const { callTakes, refreshCallTakes } = takes;
  const steps = useGuidedSteps({
    recorder: rec,
    t,
    flash,
    deleteReplacedSample: removeSample,
    samples,
  });
  const { currentStep, setCurrentStep, isReadingMode, completedSteps, isSubmitting } = steps;
  const modal = useVoluntarySample({ recorder: rec, playback, jobs, t, samplesCount: samples.length, flash });
  const { isModalOpen, modalSampleName, setModalSampleName, modalCaptured } = modal;

  const [legacyNotice, setLegacyNotice] = useState<boolean>(false);
  const [isReenrolling, setIsReenrolling] = useState<boolean>(false);
  useEffect(() => {
    const storage = browserStorage();
    if (takeLegacyNotice(purgeLegacyVoiceKeys(storage), storage)) setLegacyNotice(true);
  }, []);

  // Initial state: samples, takes and profile status from the service.
  useEffect(() => {
    let cancelled = false;
    let retryTimer: ReturnType<typeof setTimeout> | null = null;

    const initVoiceData = async (attempt = 1) => {
      try {
        const list = await refreshSamples();
        if (cancelled) return;
        await refreshCallTakes();
        if (cancelled) return;
        const initialProfile = await profile.loadInitialStatus(list?.samples?.length ?? 0);
        if (cancelled) return;

        const enrolled = hasServiceVoiceProfile(initialProfile) || initialProfile.is_enrolled === true;
        if (list?.samples && list.samples.length > 0) {
          steps.syncFromSamples(list.samples, true, enrolled);
        } else if (enrolled) {
          steps.syncFromSamples([], true, true);
        }
        if (enrolled) {
          setCurrentStep(5);
        }

        // If list failed to load on initial mount (daemon was starting up), retry up to 4 times
        if (!list && attempt < 5 && !cancelled) {
          retryTimer = setTimeout(() => {
            if (!cancelled) void initVoiceData(attempt + 1);
          }, attempt * 800);
        }
      } catch {
        if (!cancelled && attempt < 5) {
          retryTimer = setTimeout(() => {
            if (!cancelled) void initVoiceData(attempt + 1);
          }, attempt * 800);
        }
      }
    };

    void initVoiceData();

    return () => {
      cancelled = true;
      if (retryTimer) clearTimeout(retryTimer);
    };
  }, [refreshSamples, refreshCallTakes]);

  // Re-fetch samples whenever a profile update arrives from the daemon
  useEffect(() => {
    const handleProfileUpdate = () => {
      if (samples.length === 0) {
        void refreshSamples().then((list) => {
          if (list?.samples && list.samples.length > 0) {
            const enrolled = hasServiceVoiceProfile(profileStatus) || profileStatus.is_enrolled === true;
            steps.syncFromSamples(list.samples, true, enrolled);
          }
        });
      }
    };
    window.addEventListener('clearcore_profile_updated', handleProfileUpdate);
    const api = typeof window !== 'undefined' ? window.clearcoreApi : undefined;
    const cleanup = api?.onVoiceProfileUpdate?.(() => handleProfileUpdate());
    return () => {
      window.removeEventListener('clearcore_profile_updated', handleProfileUpdate);
      if (cleanup) cleanup();
    };
  }, [samples.length, refreshSamples, profileStatus, steps]);

  const handleResetEnrollment = () => {
    setIsReenrolling(true);
    steps.resetSteps();
    changeTab({ by: 'user', id: 'enroll' });
  };

  const handleBuildProfile = async (origin: 'enroll' | 'profile') => {
    if (profile.isBuilding) return;
    if (await profile.buildProfile(origin)) {
      setIsReenrolling(false);
      flash(t('voiceProfile.profileActivatedSuccess'), 6000);
      changeTab({ by: 'user', id: 'profile' });
      await refreshSamples();
    }
  };

  const handleDeleteSample = async (id: string) => {
    const list = await removeSample(id);
    if (list) jobs.setBudgetError(null);
  };

  const handleApproveCallTake = async (take: CallSuggestionTake) => {
    jobs.setBudgetError(null);
    jobs.setEnrollErrorText(null);
    const outcome = await takes.approveTake(take);
    if (outcome === null) return; // already being approved
    if (outcome.kind === 'budget') jobs.showBudgetError(outcome.remainingSeconds);
    else if (outcome.kind === 'error') jobs.showError('calls', errorLabel(outcome.code, labels));
    else if (outcome.kind === 'approved') flash(t('voiceProfile.takeApprovedFeedback'), 4000);
    // 'not-recorded' (no margin left): silent by design (spec §4.4).
    await Promise.all([refreshSamples(), refreshCallTakes()]);
  };

  const handleDismissCallTake = async (id: string) => {
    await takes.dismissTake(id);
    flash(t('voiceProfile.takeDismissedFeedback'), 3000);
  };

  const status = profileStatusLabel(profileStatus, t);
  const feedbackView = { jobBusy, currentJob, enrollErrorText };
  const feedbackFor = (tab: VoiceTabId) => jobFeedbackFor(tab, jobs.source, feedbackView);
  const badges = voiceTabBadges(samples.length, callTakes.length);
  // While the microphone records, sample is being submitted, or profile is building, the other tabs stay closed.
  const tabs: TabDef[] = [
    { id: 'enroll', label: t('voiceProfile.tabEnroll') },
    { id: 'gallery', label: t('voiceProfile.tabGallery'), badge: badges.gallery },
    { id: 'calls', label: t('voiceProfile.tabCalls'), badge: badges.calls },
    { id: 'profile', label: t('voiceProfile.tabProfile') },
  ].map((tab) => ({ ...tab, disabled: (isRecording || isSubmitting || profile.isBuilding) && tab.id !== activeTab }));

  const isVoiceIsolationActive = profileStatus.is_voice_profile_active === true;
  const hasProfile = hasServiceVoiceProfile(profileStatus) || profileStatus.is_enrolled === true;

  const handleToggleVoiceIsolation = async (enabled: boolean) => {
    logger.info('VoiceProfileCard: toggling voice isolation', { enabled });
    const success = await profile.setVoiceIsolation(enabled);
    if (success) {
      flash(
        enabled
          ? t('voiceProfile.isolationEnabledFeedback')
          : t('voiceProfile.isolationDisabledFeedback'),
        4000,
      );
    }
  };

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
            busy={jobBusy && jobs.source?.kind === 'build'}
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
            errorText={feedbackFor('calls')?.enrollErrorText ?? null}
            approvingIds={takes.approvingIds}
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
            samplesCount={sampleList ? samplesUsedInProfile(samples) : profileStatus.active_samples_count}
            canBuild={samples.length > 0}
            busy={jobBusy || isRecording}
            isBuilding={profile.isBuilding}
            feedback={feedbackFor('profile')}
            onBuildProfile={() => void handleBuildProfile('profile')}
            onResetEnrollment={handleResetEnrollment}
            onToggleVoiceIsolation={(enabled) => void handleToggleVoiceIsolation(enabled)}
            stale={showStaleProfileNotice(profileStatus, sampleList ? profileSampleIds(samples) : null, profile.idsAtBuild)}
          />
        );
      default:
        return (
          <EnrollPanel
            t={t}
            locale={locale}
            labels={labels}
            isEnrolled={(hasServiceVoiceProfile(profileStatus) || profileStatus.is_enrolled === true) && !isReenrolling}
            currentStep={currentStep}
            isReadingMode={isReadingMode}
            completedSteps={completedSteps}
            isRecording={isRecording}
            isSubmitting={isSubmitting}
            liveVoiceLevel={liveVoiceLevel}
            recordingElapsedSeconds={recordingElapsedSeconds}
            captureError={isModalOpen ? null : captureError}
            jobBusy={jobBusy}
            isBuilding={profile.isBuilding}
            isStarting={isStarting}
            feedback={isModalOpen ? null : feedbackFor('enroll')}
            onSelectStep={(step) => setCurrentStep(stepAfterSelect(currentStep, step, isRecording || isSubmitting))}
            onToggleReadingMode={steps.toggleReadingMode}
            onStartStep={(step) => void steps.startStep(step)}
            onFinishStep={(step) => void steps.finishStep(step)}
            onRedoStep={steps.redoStep}
            onNextStep={steps.nextStep}
            onBuildProfile={() => void handleBuildProfile('enroll')}
            onResetEnrollment={handleResetEnrollment}
          />
        );
    }
  };

  return (
    <div className="card voice-profile-card">
      <div style={{ marginBottom: 16 }}>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, flexWrap: 'wrap' }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <h2 className="card-title" style={{ margin: 0 }}>{t('voiceProfile.title')}</h2>
            <span className={`status-pill ${status.active ? 'pill-active' : 'pill-pending'}`}>{status.text}</span>
          </div>
          {hasProfile && (
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <label
                className="toggle-switch-wrapper"
                title={isVoiceIsolationActive ? t('voiceProfile.isolationDisabled') : t('voiceProfile.isolationEnabled')}
              >
                <input
                  type="checkbox"
                  checked={isVoiceIsolationActive}
                  disabled={profile.isBuilding || jobBusy}
                  onChange={(e) => void handleToggleVoiceIsolation(e.target.checked)}
                />
                <span className="toggle-switch-slider" />
              </label>
              <span className={`toggle-status-pill ${isVoiceIsolationActive ? 'pill-on' : 'pill-off'}`}>
                {isVoiceIsolationActive ? t('voiceProfile.isolationEnabled') : t('voiceProfile.isolationDisabled')}
              </span>
            </div>
          )}
        </div>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 6, lineHeight: 1.45 }}>
          {t('voiceProfile.description')}
        </p>
      </div>

      {legacyNotice && (
        <div style={{ marginBottom: 12 }}>
          <div role="note" style={{ fontSize: 12, color: 'var(--text-muted)', marginTop: 4 }}>
            {t('voiceProfile.legacySamplesNotice')}
          </div>
        </div>
      )}

      {feedbackMessage && <div className="feedback-banner success-banner">{feedbackMessage}</div>}

      <Tabs tabs={tabs} activeId={activeTab} onChange={(id) => changeTab({ by: 'user', id })} renderPanel={renderPanel} ariaLabel={t('voiceProfile.tabsAriaLabel')} idPrefix="voice-profile" />

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
          isStarting={isStarting}
          feedback={feedbackView}
          playingAudioId={playingAudioId}
          onStart={() => void modal.startRecording()}
          onStop={() => void modal.finishRecording()}
          onPlayPreview={() => { if (modalCaptured) playCaptured('modal-preview', modalCaptured); }}
          onCancel={modal.cancelModal}
          onSave={() => void modal.save()}
        />
      )}

      {deviceSwitch.pending && <DeviceSwitchDialog t={t} info={deviceSwitch.pending} onAnswer={deviceSwitch.answer} />}
    </div>
  );
};
