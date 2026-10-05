import { useCallback, useEffect, useRef, useState } from 'react';
import type { InputDeviceInfo } from '../../types';
import type { CapturedPcm, DeviceInfo, SampleList } from '../enrollmentTypes';
import { MAX_RECORD_SECONDS } from '../enrollmentTypes';
import { enrollmentErrorCode, isBudgetError } from '../enrollmentErrors';
import { addSample, waitForJob } from '../enrollmentClient';
import { acquireRawPhysicalStream, PhysicalMicUnavailableError } from '../captureDevice';
import { PcmRecorder } from '../pcmCapture';
import { nextStepAfterJob, type JobOutcome, type Translate } from './voiceProfileLogic';
import type { JobFeedback, JobOrigin } from './useJobFeedback';

export interface SubmitResult {
  outcome: JobOutcome;
  /** Id of the sample the service stored (null unless the job reported one). */
  sampleId: string | null;
}

export interface EnrollmentRecorder {
  isRecording: boolean;
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
  const [recordingElapsedSeconds, setRecordingElapsedSeconds] = useState<number>(0);
  const [liveVoiceLevel, setLiveVoiceLevel] = useState<number>(0);
  const [captureError, setCaptureError] = useState<string | null>(null);

  const recorderRef = useRef<PcmRecorder | null>(null);
  const deviceRef = useRef<DeviceInfo | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const recordingStartTimeRef = useRef<number>(0);

  // Stops any capture in progress and discards its PCM.
  const cleanupRecording = useCallback(() => {
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    const recorder = recorderRef.current;
    const device = deviceRef.current;
    recorderRef.current = null;
    deviceRef.current = null;
    if (recorder && device) recorder.stop(device).catch(() => undefined);
    streamRef.current?.getTracks().forEach((track) => track.stop());
    streamRef.current = null;
    setIsRecording(false);
    setLiveVoiceLevel(0);
  }, []);

  useEffect(() => cleanupRecording, [cleanupRecording]);

  /**
   * Opens the raw physical microphone and starts the PCM recorder. If no physical microphone
   * opens, shows the error and does NOT start recording (spec D1: no fallback).
   */
  const startCapture = async (onLimit: () => void): Promise<boolean> => {
    cleanupRecording();
    setCaptureError(null);
    let acquired: { stream: MediaStream; device: DeviceInfo };
    try {
      acquired = await acquireRawPhysicalStream(selectedInputId, inputDevices);
    } catch (err) {
      if (!(err instanceof PhysicalMicUnavailableError)) console.warn('Physical microphone capture failed');
      setCaptureError(t('voiceProfile.physicalMicUnavailable'));
      return false;
    }
    streamRef.current = acquired.stream;
    const recorder = new PcmRecorder({ maxSeconds: MAX_RECORD_SECONDS });
    try {
      await recorder.start(acquired.stream, (level01) => setLiveVoiceLevel(Math.round(level01 * 100)));
    } catch {
      acquired.stream.getTracks().forEach((track) => track.stop());
      streamRef.current = null;
      setCaptureError(t('voiceProfile.physicalMicUnavailable'));
      return false;
    }
    recorderRef.current = recorder;
    deviceRef.current = acquired.device;
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
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    const recorder = recorderRef.current;
    const device = deviceRef.current;
    recorderRef.current = null;
    deviceRef.current = null;
    streamRef.current = null;
    setIsRecording(false);
    setLiveVoiceLevel(0);
    if (!recorder || !device) return null;
    try {
      return await recorder.stop(device);
    } catch {
      return null;
    }
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
    isRecording, recordingElapsedSeconds, liveVoiceLevel, captureError, setCaptureError,
    startCapture, stopCapture, cleanupRecording, submitSample,
  };
}
