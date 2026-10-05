import { useState } from 'react';
import type { CapturedPcm } from '../enrollmentTypes';
import type { AudioPlayback } from './useAudioPlayback';
import type { EnrollmentRecorder } from './useEnrollmentRecorder';
import type { JobFeedback } from './useJobFeedback';
import type { Translate } from './voiceProfileLogic';

export interface VoluntarySample {
  isModalOpen: boolean;
  modalSampleName: string;
  setModalSampleName: (name: string) => void;
  modalCaptured: CapturedPcm | null;
  openModal: () => void;
  cancelModal: () => void;
  startRecording: () => Promise<void>;
  finishRecording: () => Promise<void>;
  save: () => Promise<void>;
}

/** The "+ add sample" modal: one extra sample recorded with the same physical microphone. */
export function useVoluntarySample(opts: {
  recorder: EnrollmentRecorder;
  playback: AudioPlayback;
  jobs: JobFeedback;
  t: Translate;
  samplesCount: number;
  flash: (message: string, ms: number) => void;
}): VoluntarySample {
  const { recorder, playback, jobs, t, samplesCount, flash } = opts;
  const [isModalOpen, setIsModalOpen] = useState<boolean>(false);
  const [modalSampleName, setModalSampleName] = useState<string>('');
  const [modalCaptured, setModalCaptured] = useState<CapturedPcm | null>(null);

  const finishRecording = async () => {
    const captured = await recorder.stopCapture();
    if (captured) setModalCaptured(captured);
  };

  const save = async () => {
    if (!modalCaptured) return;
    const name = modalSampleName.trim() || t('voiceProfile.defaultSampleName', { n: String(samplesCount + 1) });
    const { outcome } = await recorder.submitSample(modalCaptured, name, 'gallery');
    if (outcome.kind === 'show-error') return; // the modal stays open with the error
    // Done, or budget exceeded: close the modal so the gallery (and its delete buttons) is reachable.
    setIsModalOpen(false);
    if (outcome.kind === 'done') {
      setModalSampleName('');
      setModalCaptured(null);
      flash(t('voiceProfile.sampleCompleted'), 3500);
    }
  };

  return {
    isModalOpen, modalSampleName, setModalSampleName, modalCaptured,
    openModal: () => {
      jobs.clearMessages();
      setIsModalOpen(true);
    },
    cancelModal: () => {
      recorder.cleanupRecording();
      playback.stopPlayback();
      setIsModalOpen(false);
      setModalCaptured(null);
      recorder.setCaptureError(null);
      jobs.setEnrollErrorText(null);
    },
    startRecording: async () => {
      await recorder.startCapture(() => {
        void finishRecording();
      });
    },
    finishRecording,
    save,
  };
}
