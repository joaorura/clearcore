import { useEffect, useRef, useState } from 'react';
import { JobAbortScope, ensureOpenScope, isAbortError } from '../jobAbort';
import { invokeBridge } from '../../bridge';
import type { VoiceProfileStatus } from '../../types';
import type { EnrollmentJob, EnrollmentLabels, SampleList } from '../enrollmentTypes';
import { buildProfile, getActiveEnrollmentJobs, waitForJob } from '../enrollmentClient';
import { resolveBuildJob } from '../profileBuildFlow';
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
  /** True while a build waits for a sample job the daemon is still processing (ENROLL_BUSY). */
  waitingForSample: boolean;
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
  const [waitingForSample, setWaitingForSample] = useState<boolean>(false);
  // The build poller stops when the card unmounts. abortAll() closes a scope for good and React
  // StrictMode runs that cleanup on a live card, so the scope is re-created whenever it is found
  // closed (same as useEnrollmentRecorder): never captured once at the first render.
  const jobScopeRef = useRef<JobAbortScope | null>(null);
  if (jobScopeRef.current === null || jobScopeRef.current.isClosed()) jobScopeRef.current = new JobAbortScope();
  useEffect(() => () => jobScopeRef.current?.abortAll(), []);

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

  /**
   * Starts a build and follows its job. With `attachJobId` it only follows a profile job the daemon
   * is already running (card opened mid-build): no new build is requested.
   */
  const runBuild = async (origin: Extract<JobOrigin, 'enroll' | 'profile'>, attachJobId?: string): Promise<boolean> => {
    if (buildingRef.current) return false;
    buildingRef.current = true;
    setIsBuilding(true);
    jobs.begin(origin, 'build');
    let done = false;
    try {
      const scope = (jobScopeRef.current = ensureOpenScope(jobScopeRef.current));
      const follow = async (jobId: string, onUpdate?: (j: EnrollmentJob) => void) => {
        const signal = scope.signal();
        try {
          return await waitForJob(jobId, { onUpdate, signal });
        } finally {
          scope.release(signal);
        }
      };
      const resolved = attachJobId
        ? { jobId: attachJobId }
        : await resolveBuildJob({
            startBuild: () => buildProfile(t('voiceProfile.defaultProfileName')),
            activeJobs: getActiveEnrollmentJobs,
            waitForSampleJob: (id) => follow(id),
            sleep: (ms) => new Promise<void>((r) => setTimeout(r, ms)),
            onProcessingSample: setWaitingForSample,
          });
      if ('errorCode' in resolved) {
        jobs.setEnrollErrorText(errorLabelForJob(resolved.errorCode, 'build', labels, t));
      } else {
        const job = await follow(resolved.jobId, jobs.setCurrentJob);
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
      setWaitingForSample(false);
    }

    return done;
  };

  // The card (re)opens while the daemon is already building a profile (started earlier, or by a
  // click whose follower was lost): follow that job instead of showing an idle button. Only a
  // profile job is adopted here; a sample job belongs to the recording flow.
  const latestRunBuild = useRef(runBuild);
  latestRunBuild.current = runBuild;
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const active = await getActiveEnrollmentJobs();
      if (cancelled || buildingRef.current) return;
      const running = active.find((j) => j.kind === 'profile');
      if (!running) return;
      await latestRunBuild.current('profile', running.jobId);
    })();
    return () => { cancelled = true; };
  }, []);

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

  return { profileStatus, loadInitialStatus, buildProfile: runBuild, setVoiceIsolation, idsAtBuild, isBuilding, waitingForSample };
}
