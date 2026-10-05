import { useCallback, useEffect, useRef, useState } from 'react';
import type { InputDeviceInfo } from '../../types';
import type { CapturedPcm, SampleList } from '../enrollmentTypes';
import { MAX_RECORD_SECONDS } from '../enrollmentTypes';
import { enrollmentErrorCode, isBudgetError } from '../enrollmentErrors';
import { addSample, waitForJob, logVoiceDebug } from '../enrollmentClient';
import { acquireRawPhysicalStream, PhysicalMicUnavailableError } from '../captureDevice';
import { PcmRecorder } from '../pcmCapture';
import { CaptureController } from '../captureController';
import { JobAbortScope, isAbortError } from '../jobAbort';
import { nextStepAfterJob, shouldReportUnstartedCapture, type JobOutcome, type Translate } from './voiceProfileLogic';
import type { JobFeedback, JobOrigin } from './useJobFeedback';

export interface SubmitResult {
  /** 'cancelled': the user declined the microphone switch; nothing was sent and the PCM was wiped. */
  outcome: JobOutcome | { kind: 'cancelled' };
  /** Id of the sample the service stored (null unless the job reported one). */
  sampleId: string | null;
}

export interface EnrollmentRecorder {
  isRecording: boolean;
  /** True while the microphone is being opened; disable the record button. */
  isStarting: boolean;
  recordingElapsedSeconds: number;
  liveVoiceLevel: number;
  captureError: string | null;
  setCaptureError: (text: string | null) => void;
  startCapture: (onLimit: () => void) => Promise<boolean>;
  stopCapture: () => Promise<CapturedPcm | null>;
  cleanupRecording: () => void;
  submitSample: (captured: CapturedPcm, name: string, origin: JobOrigin) => Promise<SubmitResult>;
}

/**
 * Raw PCM capture from the physical microphone (no fallback) and submission of one sample to the
 * service, following its job until done/failed. Captured PCM stays in memory only.
 */
export function useEnrollmentRecorder(opts: {
  selectedInputId?: string;
  inputDevices?: InputDeviceInfo[];
  t: Translate;
  jobs: JobFeedback;
  refreshSamples: () => Promise<SampleList | null>;
  /** Asked before a take is sent (device switch, spec §4.4); false = do not send. */
  beforeSend?: (captured: CapturedPcm) => Promise<boolean>;
}): EnrollmentRecorder {
  const { selectedInputId, inputDevices, t, jobs, refreshSamples, beforeSend } = opts;
  const [isRecording, setIsRecording] = useState<boolean>(false);
  const [isStarting, setIsStarting] = useState<boolean>(false);
  const [recordingElapsedSeconds, setRecordingElapsedSeconds] = useState<number>(0);
  const [liveVoiceLevel, setLiveVoiceLevel] = useState<number>(0);
  const [captureError, setCaptureError] = useState<string | null>(null);

  const controllerRef = useRef<CaptureController | null>(null);
  if (controllerRef.current === null) {
    controllerRef.current = new CaptureController(() => new PcmRecorder({ maxSeconds: MAX_RECORD_SECONDS }));
  }
  const controller = controllerRef.current;
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  // Set by cleanupRecording(): a start cancelled on purpose is not reported as an error.
  const cancelRequestedRef = useRef<boolean>(false);
  const recordingStartTimeRef = useRef<number>(0);

  const clearTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  };

  // Stops any capture in progress (or in flight) and discards its PCM.
  const cleanupRecording = useCallback(() => {
    cancelRequestedRef.current = true;
    clearTimer();
    controller.cancel();
    setIsStarting(false);
    setIsRecording(false);
    setLiveVoiceLevel(0);
  }, [controller]);

  useEffect(() => cleanupRecording, [cleanupRecording]);

  // Every job poller started here stops when the card unmounts.
  const jobScopeRef = useRef<JobAbortScope | null>(null);
  if (jobScopeRef.current === null || jobScopeRef.current.isClosed()) {
    jobScopeRef.current = new JobAbortScope();
  }
  useEffect(() => {
    return () => {
      jobScopeRef.current?.abortAll();
    };
  }, []);

  /**
   * Opens the raw physical microphone and starts the PCM recorder. Single-flight: a second call
   * while one is opening returns false. If no physical microphone opens, shows the error and does
   * NOT start recording (spec D1: no fallback).
   */
  const startCapture = async (onLimit: () => void): Promise<boolean> => {
    if (controller.isStarting) {
      setCaptureError(t('voiceProfile.captureNotStarted'));
      return false;
    }
    cancelRequestedRef.current = false;
    clearTimer();
    setCaptureError(null);
    setIsRecording(false);
    setIsStarting(true);
    const result = await controller.start({
      acquire: () => acquireRawPhysicalStream(selectedInputId, inputDevices),
      onLevel: (level01) => setLiveVoiceLevel(Math.round(level01 * 100)),
      onEnded: () => {
        // Microphone unplugged / track ended: discard the take and tell the user.
        cleanupRecording();
        setCaptureError(t('voiceProfile.physicalMicUnavailable'));
      },
    });
    if (result.status === 'busy' || result.status === 'cancelled') {
      // A cancel the user asked for (cleanup) already reset the state and needs no message.
      if (shouldReportUnstartedCapture(result.status, cancelRequestedRef.current)) {
        setIsStarting(false);
        setCaptureError(t('voiceProfile.captureNotStarted'));
      }
      return false;
    }
    setIsStarting(false);
    if (result.status === 'failed') {
      if (!(result.error instanceof PhysicalMicUnavailableError)) console.warn('Physical microphone capture failed');
      setCaptureError(t('voiceProfile.physicalMicUnavailable'));
      return false;
    }
    recordingStartTimeRef.current = Date.now();
    setRecordingElapsedSeconds(0);
    setIsRecording(true);
    const timer = setInterval(() => {
      const elapsed = (Date.now() - recordingStartTimeRef.current) / 1000;
      setRecordingElapsedSeconds(Math.round(elapsed * 10) / 10);
      if (elapsed >= MAX_RECORD_SECONDS) {
        clearInterval(timer);
        onLimit();
      }
    }, 100);
    timerRef.current = timer;
    return true;
  };

  /** Stops the recorder and returns the captured PCM (null if nothing was recording). */
  const stopCapture = async (): Promise<CapturedPcm | null> => {
    clearTimer();
    setIsStarting(false);
    setIsRecording(false);
    setLiveVoiceLevel(0);
    const captured = await controller.stop();
    logVoiceDebug('RECORDER', 'stopCapture called', {
      hasPcm: Boolean(captured),
      durationSec: captured?.durationSec,
      peak: captured?.peak,
      rmsDbfs: captured?.rmsDbfs,
      device: captured?.device,
    });
    return captured;
  };

  /** Sends one captured sample to the service and follows its job until done/failed. */
  const submitSample = async (captured: CapturedPcm, name: string, origin: JobOrigin): Promise<SubmitResult> => {
    logVoiceDebug('RECORDER', 'submitSample called', { name, origin, durationSec: captured.durationSec, device: captured.device });
    if (beforeSend) {
      const allowed = await beforeSend(captured);
      logVoiceDebug('RECORDER', 'beforeSend result:', { allowed });
      if (!allowed) {
        logVoiceDebug('RECORDER', 'submitSample ABORTED by beforeSend');
        console.warn('[VoiceRecorder] submitSample aborted before send (device switch cancelled)');
        return { outcome: { kind: 'cancelled' }, sampleId: null };
      }
    }
    jobs.begin(origin);
    let outcome: JobOutcome;
    let sampleId: string | null = null;
    try {
      logVoiceDebug('RECORDER', 'addSample calling IPC for name:', name);
      const start = await addSample(captured, name);
      const startError = enrollmentErrorCode(start);
      if (startError !== null || !('jobId' in start)) {
        logVoiceDebug('RECORDER', 'addSample rejected by daemon/IPC:', { startError, start });
        console.error('[VoiceRecorder] addSample rejected:', startError, start);
        outcome = isBudgetError(startError)
          ? { kind: 'show-budget-error', remainingSeconds: null }
          : { kind: 'show-error', code: startError ?? 'ENROLL_FAILED' };
      } else {
        captured.pcm.fill(0); // sent: drop the raw audio from memory
        if (!jobScopeRef.current || jobScopeRef.current.isClosed()) {
          jobScopeRef.current = new JobAbortScope();
        }
        const activeScope = jobScopeRef.current;
        const signal = activeScope.signal();
        try {
          logVoiceDebug('RECORDER', 'waitForJob starting for jobId:', start.jobId);
          const job = await waitForJob(start.jobId, { onUpdate: jobs.setCurrentJob, signal });
          jobs.setCurrentJob(job);
          outcome = nextStepAfterJob(job);
          sampleId = job.sampleId;
          logVoiceDebug('RECORDER', 'waitForJob completed:', { state: job.state, outcome, sampleId });
          console.log('[VoiceRecorder] waitForJob completed:', job.state, outcome);
        } finally {
          activeScope.release(signal);
        }
      }
      jobs.applyOutcome(outcome);
    } catch (err) {
      logVoiceDebug('RECORDER', 'submitSample caught exception:', { message: (err as Error)?.message });
      console.error('[VoiceRecorder] submitSample error:', err);
      outcome = { kind: 'show-error', code: 'SERVICE_UNAVAILABLE' };
      // Unmounted: no state update and no refresh.
      if (isAbortError(err)) return { outcome, sampleId };
      jobs.failWith(err);
    } finally {
      jobs.setJobBusy(false);
    }
    logVoiceDebug('RECORDER', 'submitSample calling refreshSamples()...');
    await refreshSamples();
    logVoiceDebug('RECORDER', 'submitSample finished, returning:', { outcome, sampleId });
    return { outcome, sampleId };
  };

  return {
    isRecording, isStarting, recordingElapsedSeconds, liveVoiceLevel, captureError, setCaptureError,
    startCapture, stopCapture, cleanupRecording, submitSample,
  };
}
