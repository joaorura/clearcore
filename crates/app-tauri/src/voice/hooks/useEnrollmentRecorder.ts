import { useCallback, useEffect, useRef, useState } from 'react';
import type { InputDeviceInfo } from '../../types';
import type { CapturedPcm, SampleList } from '../enrollmentTypes';
import { MAX_RECORD_SECONDS } from '../enrollmentTypes';
import { enrollmentErrorCode, isBudgetError } from '../enrollmentErrors';
import { addSample, waitForJob } from '../enrollmentClient';
import { acquireRawPhysicalStream, PhysicalMicUnavailableError } from '../captureDevice';
import { PcmRecorder } from '../pcmCapture';
import { CaptureController } from '../captureController';
import { nextStepAfterJob, type JobOutcome, type Translate } from './voiceProfileLogic';
import type { JobFeedback, JobOrigin } from './useJobFeedback';

export interface SubmitResult {
  outcome: JobOutcome;
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
}): EnrollmentRecorder {
  const { selectedInputId, inputDevices, t, jobs, refreshSamples } = opts;
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
  const recordingStartTimeRef = useRef<number>(0);

  const clearTimer = () => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
  };

  // Stops any capture in progress (or in flight) and discards its PCM.
  const cleanupRecording = useCallback(() => {
    clearTimer();
    controller.cancel();
    setIsStarting(false);
    setIsRecording(false);
    setLiveVoiceLevel(0);
  }, [controller]);

  useEffect(() => cleanupRecording, [cleanupRecording]);

  /**
   * Opens the raw physical microphone and starts the PCM recorder. Single-flight: a second call
   * while one is opening returns false. If no physical microphone opens, shows the error and does
   * NOT start recording (spec D1: no fallback).
   */
  const startCapture = async (onLimit: () => void): Promise<boolean> => {
    if (controller.isStarting) return false;
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
    if (result.status === 'busy') return false;
    if (result.status === 'cancelled') return false; // cleanup already reset the state
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
    return controller.stop();
  };

  /** Sends one captured sample to the service and follows its job until done/failed. */
  const submitSample = async (captured: CapturedPcm, name: string, origin: JobOrigin): Promise<SubmitResult> => {
    jobs.begin(origin);
    let outcome: JobOutcome;
    let sampleId: string | null = null;
    try {
      const start = await addSample(captured, name);
      const startError = enrollmentErrorCode(start);
      if (startError !== null || !('jobId' in start)) {
        outcome = isBudgetError(startError)
          ? { kind: 'show-budget-error', remainingSeconds: null }
          : { kind: 'show-error', code: startError ?? 'ENROLL_FAILED' };
      } else {
        captured.pcm.fill(0); // sent: drop the raw audio from memory
        const job = await waitForJob(start.jobId, { onUpdate: jobs.setCurrentJob });
        jobs.setCurrentJob(job);
        outcome = nextStepAfterJob(job);
        sampleId = job.sampleId;
      }
      jobs.applyOutcome(outcome);
    } catch (err) {
      outcome = { kind: 'show-error', code: 'SERVICE_UNAVAILABLE' };
      jobs.failWith(err);
    } finally {
      jobs.setJobBusy(false);
    }
    await refreshSamples();
    return { outcome, sampleId };
  };

  return {
    isRecording, isStarting, recordingElapsedSeconds, liveVoiceLevel, captureError, setCaptureError,
    startCapture, stopCapture, cleanupRecording, submitSample,
  };
}
