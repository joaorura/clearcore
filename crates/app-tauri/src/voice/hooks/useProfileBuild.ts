import { useEffect, useRef, useState } from 'react';
import { JobAbortScope, isAbortError } from '../jobAbort';
import { invokeBridge } from '../../bridge';
import type { VoiceProfileStatus } from '../../types';
import type { EnrollmentLabels, SampleList } from '../enrollmentTypes';
import { enrollmentErrorCode } from '../enrollmentErrors';
import { buildProfile, waitForJob } from '../enrollmentClient';
import {
  applySetVoiceProfileResult,
  profileSampleIds,
  errorLabelForJob,
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
  /** Activates or deactivates the voice profile in the audio engine without deleting it. */
  setVoiceIsolation: (enabled: boolean) => Promise<boolean>;
  /** Ids of the samples the last successful build in this session used (null: none yet). */
  idsAtBuild: string[] | null;
  /** True while the neural profile is actively being built and applied. */
  isBuilding: boolean;
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
  const [idsAtBuild, setIdsAtBuild] = useState<string[] | null>(null);
  const [isBuilding, setIsBuilding] = useState<boolean>(false);
  const buildingRef = useRef<boolean>(false);
  // The build poller stops when the card unmounts.
  const jobScopeRef = useRef<JobAbortScope | null>(null);
  if (jobScopeRef.current === null) jobScopeRef.current = new JobAbortScope();
  const jobScope = jobScopeRef.current;
  useEffect(() => () => jobScope.abortAll(), [jobScope]);

  // Main-process pushes (same merged payload as the set_voice_profile reply).
  useEffect(() => {
    const api = typeof window !== 'undefined' ? window.clearcoreApi : undefined;
    if (!api?.onVoiceProfileUpdate) return;
    return api.onVoiceProfileUpdate((profile) => {
      setProfileStatus((prev) => {
        const next = applySetVoiceProfileResult(prev, prev, profile);
        if (typeof window !== 'undefined') {
          window.dispatchEvent(new CustomEvent('clearcore_profile_updated', { detail: next }));
        }
        return next;
      });
    });
  }, []);

  // Window-level reactive updates from custom events
  useEffect(() => {
    if (typeof window === 'undefined') return;
    const handleEvent = (e: Event) => {
      const custom = e as CustomEvent<VoiceProfileStatus>;
      if (custom.detail) {
        setProfileStatus((prev) => applySetVoiceProfileResult(prev, prev, custom.detail));
      }
    };
    window.addEventListener('clearcore_profile_updated', handleEvent);
    return () => window.removeEventListener('clearcore_profile_updated', handleEvent);
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
    if (buildingRef.current) return false;
    buildingRef.current = true;
    setIsBuilding(true);
    jobs.begin(origin, 'build');
    let done = false;
    try {
      const start = await buildProfile(t('voiceProfile.defaultProfileName'));
      const startError = enrollmentErrorCode(start);
      if (startError !== null || !('jobId' in start)) {
        jobs.setEnrollErrorText(errorLabelForJob(startError ?? 'ENROLL_FAILED', 'build', labels, t));
      } else {
        const signal = jobScope.signal();
        let job;
        try {
          job = await waitForJob(start.jobId, { onUpdate: jobs.setCurrentJob, signal });
        } finally {
          jobScope.release(signal);
        }
        jobs.setCurrentJob(job);
        const outcome = nextStepAfterJob(job);
        if (outcome.kind === 'done') done = true;
        else jobs.applyOutcome(outcome, 'build');
      }

      const list = await refreshSamples();
      if (done && list) setIdsAtBuild(profileSampleIds(list.samples));
      let serviceStatus: VoiceProfileStatus | null = null;
      try {
        let res = await invokeBridge<unknown>('get_voice_profile');
        serviceStatus = normalizeVoiceProfileStatus(res);
        // If done, retry briefly (up to ~300ms) if neural_eq_calibrated has not yet updated
        // in the service supervisor state
        if (done && !serviceStatus.neural_eq_calibrated) {
          for (let attempt = 0; attempt < 5; attempt++) {
            await new Promise((r) => setTimeout(r, 60));
            res = await invokeBridge<unknown>('get_voice_profile');
            serviceStatus = normalizeVoiceProfileStatus(res);
            if (serviceStatus.neural_eq_calibrated) break;
          }
        }
        setProfileStatus((prev) => mergeVoiceProfileStatus(prev, res, list?.samples.length ?? 0));
      } catch {
        // the status keeps what the service last reported
      }
      if (done && typeof window !== 'undefined' && serviceStatus) {
        window.dispatchEvent(new CustomEvent('clearcore_profile_updated', { detail: serviceStatus }));
      }
    } catch (err) {
      if (isAbortError(err)) return false; // unmounted: nothing more to show
      jobs.failWith(err);
    } finally {
      jobs.setJobBusy(false);
      buildingRef.current = false;
      setIsBuilding(false);
    }

    return done;
  };

  const setVoiceIsolation = async (enabled: boolean): Promise<boolean> => {
    try {
      await invokeBridge('set_voice_isolation', { enabled });
      const res = await invokeBridge<unknown>('get_voice_profile');
      const updated = normalizeVoiceProfileStatus(res);
      setProfileStatus((prev) => mergeVoiceProfileStatus(prev, res, prev.active_samples_count));
      if (typeof window !== 'undefined') {
        window.dispatchEvent(new CustomEvent('clearcore_profile_updated', { detail: updated }));
      }
      return true;
    } catch {
      return false;
    }
  };

  return { profileStatus, loadInitialStatus, buildProfile: runBuild, setVoiceIsolation, idsAtBuild, isBuilding };
}
