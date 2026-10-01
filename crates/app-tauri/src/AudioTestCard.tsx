import React, { useState, useRef, useEffect } from 'react';
import { useI18n } from './i18n';
import { InputDeviceInfo } from './App';

interface AudioTestCardProps {
  virtualMicPresent: boolean;
  selectedInputId?: string;
  inputDevices: InputDeviceInfo[];
}

export const AudioTestCard: React.FC<AudioTestCardProps> = ({
  virtualMicPresent,
  selectedInputId,
  inputDevices,
}) => {
  const { t } = useI18n();

  const [testSource, setTestSource] = useState<'filtered' | 'raw'>('filtered');
  const [isRecording, setIsRecording] = useState<boolean>(false);
  const [recordingSeconds, setRecordingSeconds] = useState<number>(0);
  const [audioUrl, setAudioUrl] = useState<string | null>(null);
  const [recordedSource, setRecordedSource] = useState<'filtered' | 'raw' | null>(null);
  const [micLevel, setMicLevel] = useState<number>(0);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const audioStreamRef = useRef<MediaStream | null>(null);
  const audioContextRef = useRef<AudioContext | null>(null);
  const animationFrameRef = useRef<number | null>(null);
  const timerRef = useRef<NodeJS.Timeout | null>(null);

  const cleanupAudio = () => {
    if (animationFrameRef.current !== null) {
      cancelAnimationFrame(animationFrameRef.current);
      animationFrameRef.current = null;
    }
    if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    if (audioStreamRef.current) {
      audioStreamRef.current.getTracks().forEach((track) => track.stop());
      audioStreamRef.current = null;
    }
    if (audioContextRef.current && audioContextRef.current.state !== 'closed') {
      audioContextRef.current.close().catch(() => {});
      audioContextRef.current = null;
    }
    setMicLevel(0);
  };

  useEffect(() => {
    return () => {
      cleanupAudio();
      if (audioUrl) {
        URL.revokeObjectURL(audioUrl);
      }
    };
  }, [audioUrl]);

  const startRecording = async () => {
    setErrorMessage(null);
    if (audioUrl) {
      URL.revokeObjectURL(audioUrl);
      setAudioUrl(null);
    }
    setRecordingSeconds(0);

    try {
      let audioConstraint: boolean | MediaTrackConstraints = true;

      if (typeof navigator !== 'undefined' && navigator.mediaDevices?.enumerateDevices) {
        let devs = await navigator.mediaDevices.enumerateDevices();
        if (devs.length > 0 && devs.every((d) => !d.label)) {
          try {
            const probe = await navigator.mediaDevices.getUserMedia({ audio: true });
            probe.getTracks().forEach((t) => t.stop());
            devs = await navigator.mediaDevices.enumerateDevices();
          } catch {}
        }
        const audioInputs = devs.filter((d) => d.kind === 'audioinput');

        if (testSource === 'filtered') {
          // Look for virtual mic by name
          const virtualDev = audioInputs.find(
            (d) =>
              d.label.toLowerCase().includes('realtime') ||
              d.label.toLowerCase().includes('clearcore') ||
              d.label.toLowerCase().includes('virtual')
          );
          if (virtualDev && virtualDev.deviceId) {
            audioConstraint = { deviceId: { exact: virtualDev.deviceId } };
          } else {
            // Default system source is likely the virtual mic if activated
            audioConstraint = true;
          }
        } else {
          // Raw physical microphone
          const knownPhysical = inputDevices.find((d) => d.id === selectedInputId);
          const physicalDev = audioInputs.find(
            (d) =>
              (selectedInputId && d.deviceId === selectedInputId) ||
              (knownPhysical && d.label.includes(knownPhysical.name)) ||
              (!d.label.toLowerCase().includes('realtime') &&
                !d.label.toLowerCase().includes('clearcore') &&
                !d.label.toLowerCase().includes('virtual'))
          );
          if (physicalDev && physicalDev.deviceId) {
            audioConstraint = { deviceId: { exact: physicalDev.deviceId } };
          } else {
            audioConstraint = true;
          }
        }
      }

      const baseConstraints: MediaTrackConstraints = {
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      };

      const finalAudioConstraints: MediaTrackConstraints =
        typeof audioConstraint === 'boolean'
          ? baseConstraints
          : { ...(audioConstraint as MediaTrackConstraints), ...baseConstraints };

      const stream = await navigator.mediaDevices.getUserMedia({
        audio: finalAudioConstraints,
        video: false,
      });

      audioStreamRef.current = stream;

      // Setup Web Audio Analyser for VU Meter
      const AudioCtx = window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext;
      const audioCtx = new AudioCtx();
      audioContextRef.current = audioCtx;

      const sourceNode = audioCtx.createMediaStreamSource(stream);
      const analyser = audioCtx.createAnalyser();
      analyser.fftSize = 256;
      sourceNode.connect(analyser);

      const bufferLength = analyser.frequencyBinCount;
      const dataArray = new Uint8Array(bufferLength);

      const updateLevel = () => {
        analyser.getByteFrequencyData(dataArray);
        let sum = 0;
        for (let i = 0; i < bufferLength; i++) {
          sum += dataArray[i];
        }
        const avg = sum / bufferLength;
        // Normalize roughly to 0-100%
        const normalized = Math.min(100, Math.round((avg / 128) * 100));
        setMicLevel(normalized);
        animationFrameRef.current = requestAnimationFrame(updateLevel);
      };
      updateLevel();

      // Setup MediaRecorder
      let mimeType = '';
      if (typeof MediaRecorder !== 'undefined') {
        if (MediaRecorder.isTypeSupported('audio/webm;codecs=opus')) {
          mimeType = 'audio/webm;codecs=opus';
        } else if (MediaRecorder.isTypeSupported('audio/webm')) {
          mimeType = 'audio/webm';
        } else if (MediaRecorder.isTypeSupported('audio/ogg;codecs=opus')) {
          mimeType = 'audio/ogg;codecs=opus';
        }
      }

      const options = mimeType ? { mimeType } : undefined;
      const recorder = new MediaRecorder(stream, options);
      mediaRecorderRef.current = recorder;

      const chunks: Blob[] = [];
      recorder.ondataavailable = (e) => {
        if (e.data && e.data.size > 0) {
          chunks.push(e.data);
        }
      };

      recorder.onstop = () => {
        const finalBlob = new Blob(chunks, { type: recorder.mimeType || 'audio/webm' });
        const url = URL.createObjectURL(finalBlob);
        setAudioUrl(url);
        setRecordedSource(testSource);
        setIsRecording(false);
        cleanupAudio();
      };

      recorder.start(100);
      setIsRecording(true);

      // Start duration countdown/countup, max 10 seconds
      let sec = 0;
      timerRef.current = setInterval(() => {
        sec += 1;
        setRecordingSeconds(sec);
        if (sec >= 10) {
          stopRecording();
        }
      }, 1000);
    } catch (err) {
      cleanupAudio();
      setIsRecording(false);
      setErrorMessage(t('testAudio.errorMicAccess', { error: String(err) }));
    }
  };

  const stopRecording = () => {
    if (mediaRecorderRef.current && mediaRecorderRef.current.state === 'recording') {
      mediaRecorderRef.current.stop();
    } else {
      cleanupAudio();
      setIsRecording(false);
    }
  };

  const handleDiscard = () => {
    if (audioUrl) {
      URL.revokeObjectURL(audioUrl);
      setAudioUrl(null);
    }
    setRecordingSeconds(0);
    setRecordedSource(null);
    setErrorMessage(null);
  };

  return (
    <div className="card test-audio-card" style={{ border: '1px solid #334155', background: 'var(--card-bg)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 10 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <h2 className="card-title" style={{ margin: 0 }}>{t('testAudio.title')}</h2>
            <span style={{ fontSize: '0.75rem', background: '#064e3b', border: '1px solid #059669', borderRadius: 4, padding: '2px 8px', color: '#6ee7b7' }}>
              {t('testAudio.badge')}
            </span>
          </div>
          <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
            {t('testAudio.description')}
          </div>
        </div>
      </div>

      {errorMessage && (
        <div style={{ padding: '10px 14px', background: '#451a1a', border: '1px solid #7f1d1d', borderRadius: 6, marginBottom: 14, color: '#fca5a5', fontSize: '0.85rem' }}>
          {errorMessage}
        </div>
      )}

      {testSource === 'filtered' && !virtualMicPresent && (
        <div style={{ padding: '8px 12px', background: '#272010', border: '1px solid #78350f', borderRadius: 6, marginBottom: 14, color: '#fcd34d', fontSize: '0.8rem' }}>
          ⚠️ O microfone virtual ainda não foi detectado no sistema operacional. A gravação filtrada tentará capturar o dispositivo de áudio padrão do sistema.
        </div>
      )}

      {/* Source Selection Buttons */}
      <div style={{ marginBottom: 16 }}>
        <div style={{ fontSize: '0.85rem', fontWeight: 600, color: 'var(--text-main)', marginBottom: 6 }}>
          {t('testAudio.sourceLabel')}
        </div>
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          <button
            type="button"
            className={`action-btn ${testSource === 'filtered' ? 'active-test-source' : ''}`}
            disabled={isRecording}
            onClick={() => setTestSource('filtered')}
            style={{
              background: testSource === 'filtered' ? '#1e293b' : 'var(--bg-secondary)',
              borderColor: testSource === 'filtered' ? '#38bdf8' : 'var(--border-color)',
              color: testSource === 'filtered' ? '#38bdf8' : 'var(--text-main)',
              fontWeight: testSource === 'filtered' ? 600 : 400,
            }}
          >
            {t('testAudio.sourceFiltered')}
          </button>
          <button
            type="button"
            className={`action-btn ${testSource === 'raw' ? 'active-test-source' : ''}`}
            disabled={isRecording}
            onClick={() => setTestSource('raw')}
            style={{
              background: testSource === 'raw' ? '#1e293b' : 'var(--bg-secondary)',
              borderColor: testSource === 'raw' ? '#fbbf24' : 'var(--border-color)',
              color: testSource === 'raw' ? '#fbbf24' : 'var(--text-main)',
              fontWeight: testSource === 'raw' ? 600 : 400,
            }}
          >
            {t('testAudio.sourceRaw')}
          </button>
        </div>
      </div>

      {/* Live VU Meter during recording */}
      {isRecording && (
        <div style={{ marginBottom: 16, background: 'var(--bg-secondary)', padding: '12px 14px', borderRadius: 6, border: '1px solid var(--border-color)' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6, fontSize: '0.8rem' }}>
            <span style={{ color: '#f87171', fontWeight: 600 }}>{t('testAudio.recordingStatus')}</span>
            <span style={{ color: 'var(--text-muted)' }}>{recordingSeconds}s / 10s</span>
          </div>
          <div style={{ width: '100%', height: 12, background: '#1e293b', borderRadius: 6, overflow: 'hidden' }}>
            <div
              style={{
                width: `${micLevel}%`,
                height: '100%',
                background: micLevel > 75 ? '#ef4444' : micLevel > 40 ? '#eab308' : '#22c55e',
                transition: 'width 60ms ease-out',
                borderRadius: 6,
              }}
            />
          </div>
        </div>
      )}

      {/* Recording Control Button */}
      <div style={{ display: 'flex', gap: 10, alignItems: 'center', flexWrap: 'wrap' }}>
        {!isRecording ? (
          <button
            className="action-btn"
            onClick={startRecording}
            style={{
              background: '#047857',
              borderColor: '#059669',
              color: '#ffffff',
              fontWeight: 600,
              padding: '8px 16px',
            }}
          >
            {t('testAudio.startRecording')}
          </button>
        ) : (
          <button
            className="action-btn"
            onClick={stopRecording}
            style={{
              background: '#b91c1c',
              borderColor: '#dc2626',
              color: '#ffffff',
              fontWeight: 600,
              padding: '8px 16px',
            }}
          >
            {t('testAudio.stopRecording', { seconds: String(recordingSeconds) })}
          </button>
        )}

        {audioUrl && !isRecording && (
          <button
            className="action-btn"
            onClick={handleDiscard}
            style={{ fontSize: '0.85rem' }}
          >
            {t('testAudio.reRecordBtn')}
          </button>
        )}
      </div>

      {/* Audio Playback Player */}
      {audioUrl && !isRecording && (
        <div style={{ marginTop: 16, padding: '14px 16px', background: 'var(--bg-secondary)', borderRadius: 6, border: '1px solid var(--border-color)' }}>
          <div style={{ fontWeight: 600, color: 'var(--text-main)', fontSize: '0.9rem', marginBottom: 4 }}>
            {t('testAudio.playbackTitle')}
          </div>
          <div style={{ fontSize: '0.8rem', color: '#93c5fd', marginBottom: 10 }}>
            {recordedSource === 'filtered' ? t('testAudio.sourceUsedFiltered') : t('testAudio.sourceUsedRaw')}
          </div>
          <p style={{ fontSize: '0.85rem', color: 'var(--text-muted)', marginBottom: 8 }}>
            {t('testAudio.listenPrompt')}
          </p>
          <audio
            controls
            src={audioUrl}
            style={{ width: '100%', outline: 'none', borderRadius: 4 }}
          />
        </div>
      )}

      <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', marginTop: 12, marginBottom: 0 }}>
        {t('testAudio.hint')}
      </p>
    </div>
  );
};
