import { useEffect, useState } from 'react';
import { invokeBridge } from '../../bridge';
import type { VoiceProfileStatus } from '../../types';
import type { EnrollmentLabels, SampleList } from '../enrollmentTypes';
import { enrollmentErrorCode, errorLabel } from '../enrollmentErrors';
import { buildProfile, waitForJob } from '../enrollmentClient';
import {
  applySetVoiceProfileResult,
  mergeVoiceProfileStatus,
  nextStepAfterJob,
  normalizeVoiceProfileStatus,
  type Translate,
} from './voiceProfileLogic';
import type { JobFeedback, JobOrigin } from './useJobFeedback';

export interface ProfileBuild {
  profileStatus: VoiceProfileStatus;
  /** Reads the service status once at start (neutral when the service is unreachable). */
  loadInitialStatus: (loadedSamplesCount: number) => Promise<VoiceProfileStatus>;
  /**
   * Builds the profile in the service and follows the job; resolves true when it finished.
   * `origin` is the tab whose button was clicked: progress and errors show there (and on Profile).
   */
  buildProfile: (origin: Extract<JobOrigin, 'enroll' | 'profile'>) => Promise<boolean>;
}

/**
 * Profile status and build. The status shown is always the merged service status, never a local
 * guess (no invented EQ, gain or embedding).
 */
export function useProfileBuild(opts: {
  t: Translate;
  labels: EnrollmentLabels;
  jobs: JobFeedback;
  refreshSamples: () => Promise<SampleList | null>;
}): ProfileBuild {
  const { t, labels, jobs, refreshSamples } = opts;
  const [profileStatus, setProfileStatus] = useState<VoiceProfileStatus>({ is_enrolled: false, active_samples_count: 0 });

  // Main-process pushes (same merged payload as the set_voice_profile reply).
  useEffect(() => {
    const api = typeof window !== 'undefined' ? window.clearcoreApi : undefined;
    if (!api?.onVoiceProfileUpdate) return;
    return api.onVoiceProfileUpdate((profile) => {
      setProfileStatus((prev) => applySetVoiceProfileResult(prev, prev, profile));
    });
  }, []);

  const loadInitialStatus = async (loadedSamplesCount: number): Promise<VoiceProfileStatus> => {
    let initialProfile: VoiceProfileStatus = { is_enrolled: false, active_samples_count: loadedSamplesCount };
    try {
      const profileRes = await invokeBridge<unknown>('get_voice_profile');
      initialProfile = mergeVoiceProfileStatus(initialProfile, profileRes, loadedSamplesCount);
    } catch {
      // service unreachable: status stays neutral
    }
    setProfileStatus(initialProfile);
    return initialProfile;
  };

  const runBuild = async (origin: Extract<JobOrigin, 'enroll' | 'profile'>): Promise<boolean> => {
    jobs.begin(origin, 'build');
    let done = false;
    try {
      const start = await buildProfile(t('voiceProfile.defaultProfileName'));
      const startError = enrollmentErrorCode(start);
      if (startError !== null || !('jobId' in start)) {
        jobs.setEnrollErrorText(errorLabel(startError ?? 'ENROLL_FAILED', labels));
      } else {
        const job = await waitForJob(start.jobId, { onUpdate: jobs.setCurrentJob });
        jobs.setCurrentJob(job);
        const outcome = nextStepAfterJob(job);
        if (outcome.kind === 'done') done = true;
        else jobs.applyOutcome(outcome);
      }
    } catch (err) {
      jobs.failWith(err);
    } finally {
      jobs.setJobBusy(false);
    }

    const list = await refreshSamples();
    let serviceStatus: VoiceProfileStatus | null = null;
    try {
      const res = await invokeBridge<unknown>('get_voice_profile');
      serviceStatus = normalizeVoiceProfileStatus(res);
      setProfileStatus((prev) => mergeVoiceProfileStatus(prev, res, list?.samples.length ?? 0));
    } catch {
      // the status keeps what the service last reported
    }
    if (done && typeof window !== 'undefined' && serviceStatus) {
      window.dispatchEvent(new CustomEvent('clearcore_profile_updated', { detail: serviceStatus }));
    }
    return done;
  };

  return { profileStatus, loadInitialStatus, buildProfile: runBuild };
}
