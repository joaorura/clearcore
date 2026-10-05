import { useCallback, useEffect, useRef, useState } from 'react';
import type { CapturedPcm } from '../enrollmentTypes';

export interface AudioPlayback {
  playingAudioId: string | null;
  /** Plays a captured take from memory (no blob URL, nothing written anywhere). */
  playCaptured: (id: string, captured: CapturedPcm) => void;
  /** Plays audio the service points to (call takes). Nothing is synthesized when absent. */
  playUrl: (id: string, audioUrl?: string) => void;
  stopPlayback: () => void;
}

export function useAudioPlayback(): AudioPlayback {
  const [playingAudioId, setPlayingAudioId] = useState<string | null>(null);
  const audioElementRef = useRef<HTMLAudioElement | null>(null);
  const playbackCtxRef = useRef<AudioContext | null>(null);

  const stopPlayback = useCallback(() => {
    if (audioElementRef.current) {
      audioElementRef.current.pause();
      audioElementRef.current = null;
    }
    if (playbackCtxRef.current && playbackCtxRef.current.state !== 'closed') {
      playbackCtxRef.current.close().catch(() => undefined);
    }
    playbackCtxRef.current = null;
  }, []);

  useEffect(() => stopPlayback, [stopPlayback]);

  const playCaptured = (id: string, captured: CapturedPcm) => {
    const wasPlaying = playingAudioId === id;
    stopPlayback();
    setPlayingAudioId(null);
    if (wasPlaying || captured.pcm.length === 0) return;
    try {
      const ctx = new AudioContext({ sampleRate: captured.sampleRate });
      const buffer = ctx.createBuffer(1, captured.pcm.length, captured.sampleRate);
      buffer.getChannelData(0).set(captured.pcm);
      const src = ctx.createBufferSource();
      src.buffer = buffer;
      src.connect(ctx.destination);
      src.onended = () => {
        if (playbackCtxRef.current === ctx) stopPlayback();
        setPlayingAudioId((cur) => (cur === id ? null : cur));
      };
      playbackCtxRef.current = ctx;
      setPlayingAudioId(id);
      src.start();
    } catch {
      setPlayingAudioId(null);
    }
  };

  const playUrl = (id: string, audioUrl?: string) => {
    const wasPlaying = playingAudioId === id;
    stopPlayback();
    setPlayingAudioId(null);
    if (wasPlaying || !audioUrl) return;

    const audio = new Audio(audioUrl);
    audioElementRef.current = audio;
    setPlayingAudioId(id);
    audio.onended = () => setPlayingAudioId(null);
    audio.onerror = () => setPlayingAudioId(null);
    audio.play().catch(() => setPlayingAudioId(null));
  };

  return { playingAudioId, playCaptured, playUrl, stopPlayback };
}
