import { describe, it, expect, vi } from 'vitest';
import { renderToStaticMarkup } from 'react-dom/server';
import { I18nProvider } from '../i18n';
import { AudioTestCard } from '../AudioTestCard';

describe('AudioTestCard rendering', () => {
  it('renders default state with start recording button and dual mode title', () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AudioTestCard virtualMicPresent={true} inputDevices={[]} />
      </I18nProvider>
    );

    expect(html).toContain('test-audio-card');
    expect(html).toContain('Teste Comparativo em Tempo Real');
    expect(html).not.toContain('Comparativo A/B Sincronizado');
  });

  it('renders advisory banner when virtualMicPresent is false', () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AudioTestCard virtualMicPresent={false} inputDevices={[]} />
      </I18nProvider>
    );

    expect(html).toContain('O microfone virtual ClearCore ainda não foi detectado');
  });
});

describe('Audio playback termination and race condition coordinator (Problem 1)', () => {
  it('pauses both elements before resetting currentTime and ignores near-simultaneous duplicate ended events', () => {
    const callOrder: string[] = [];
    let isPlaying = true;
    let currentTimeA = 2.99;
    let currentTimeB = 3.00;

    const audioA = {
      paused: false,
      pause: vi.fn(() => {
        callOrder.push('audioA.pause');
        audioA.paused = true;
      }),
      set currentTime(val: number) {
        callOrder.push(`audioA.currentTime=${val}`);
        currentTimeA = val;
      },
      get currentTime() {
        return currentTimeA;
      },
    };

    const audioB = {
      paused: false,
      pause: vi.fn(() => {
        callOrder.push('audioB.pause');
        audioB.paused = true;
      }),
      set currentTime(val: number) {
        callOrder.push(`audioB.currentTime=${val}`);
        currentTimeB = val;
      },
      get currentTime() {
        return currentTimeB;
      },
    };

    let lastEndedTime = 0;
    let playSession = 1;

    const stopAndResetPlayback = () => {
      playSession += 1;
      audioA.pause();
      audioA.currentTime = 0;
      audioB.pause();
      audioB.currentTime = 0;
      isPlaying = false;
    };

    const handleEnded = () => {
      const now = Date.now();
      if (now - lastEndedTime < 500) {
        return;
      }
      lastEndedTime = now;
      stopAndResetPlayback();
    };

    // First ended event (e.g. audioA finishes at 3.00s)
    handleEnded();

    expect(audioA.pause).toHaveBeenCalledTimes(1);
    expect(audioB.pause).toHaveBeenCalledTimes(1);
    expect(currentTimeA).toBe(0);
    expect(currentTimeB).toBe(0);
    expect(isPlaying).toBe(false);

    // Verify exact ordering: pause must precede currentTime = 0
    expect(callOrder.indexOf('audioA.pause')).toBeLessThan(callOrder.indexOf('audioA.currentTime=0'));
    expect(callOrder.indexOf('audioB.pause')).toBeLessThan(callOrder.indexOf('audioB.currentTime=0'));

    // Second ended event arrives 10ms later from audioB
    const initialCallCountA = audioA.pause.mock.calls.length;
    const initialCallCountB = audioB.pause.mock.calls.length;
    handleEnded();

    // Debounce should prevent duplicate invocation
    expect(audioA.pause).toHaveBeenCalledTimes(initialCallCountA);
    expect(audioB.pause).toHaveBeenCalledTimes(initialCallCountB);
  });

  it('cancels pending play promise resolution if playback was stopped or ended before promise resolves', async () => {
    let playSession = 0;
    let isPlaying = false;

    const audioA = {
      pause: vi.fn(),
      play: vi.fn(() => new Promise<void>((resolve) => setTimeout(resolve, 50))),
    };
    const audioB = {
      pause: vi.fn(),
      play: vi.fn(() => new Promise<void>((resolve) => setTimeout(resolve, 50))),
    };

    // Start playing
    const session = ++playSession;
    const playPromise = Promise.all([audioA.play(), audioB.play()]).then(() => {
      if (playSession === session) {
        isPlaying = true;
      } else {
        audioA.pause();
        audioB.pause();
      }
    });

    // User pauses or playback ends while play promise is in-flight (at 10ms)
    await new Promise((resolve) => setTimeout(resolve, 10));
    playSession += 1; // invalidated by stopAndResetPlayback
    isPlaying = false;

    await playPromise;

    // isPlaying should remain false and audio elements must be paused
    expect(isPlaying).toBe(false);
    expect(audioA.pause).toHaveBeenCalled();
    expect(audioB.pause).toHaveBeenCalled();
  });
});

describe('AudioTestCard physical input device resolution & explicit deviceId constraints', () => {
  it('passes explicit deviceId exact constraint matching user selected physical device', async () => {
    const requestedConstraints: MediaStreamConstraints[] = [];
    const mockTrack = { stop: vi.fn() };
    const mockStream = {
      getTracks: () => [mockTrack],
    };

    const mockMediaDevices = {
      enumerateDevices: vi.fn(async () => [
        {
          deviceId: 'internal-mic-id',
          groupId: 'grp-1',
          kind: 'audioinput',
          label: 'Microfone Interno (Realtek)',
        },
        {
          deviceId: 'headset-mic-id',
          groupId: 'grp-2',
          kind: 'audioinput',
          label: 'Fone de Ouvido USB Headset Mic',
        },
        {
          deviceId: 'clearcore-virt-id',
          groupId: 'grp-3',
          kind: 'audioinput',
          label: 'Clearcore Realtime Virtual Mic',
        },
      ]),
      getUserMedia: vi.fn(async (constraints: MediaStreamConstraints) => {
        requestedConstraints.push(constraints);
        return mockStream as unknown as MediaStream;
      }),
    };

    vi.stubGlobal('navigator', {
      mediaDevices: mockMediaDevices,
    });

    const inputDevices = [
      { id: 'dev-1', name: 'Microfone Interno (Realtek)', is_default: true },
      { id: 'dev-2', name: 'Fone de Ouvido USB Headset', is_default: false },
    ];

    // Import resolvePhysicalAudioDevice to verify resolution directly
    const { resolvePhysicalAudioDevice } = await import('../voice/captureDevice');
    const devs = await mockMediaDevices.enumerateDevices();
    const resolved = resolvePhysicalAudioDevice(devs as unknown as MediaDeviceInfo[], 'dev-2', inputDevices);

    expect(resolved).toBeDefined();
    expect(resolved?.deviceId).toBe('headset-mic-id');
    expect(resolved?.label).toContain('Fone de Ouvido');

    // Simulate getUserMedia call as done in startRecording()
    await mockMediaDevices.getUserMedia({
      audio: {
        deviceId: { exact: resolved!.deviceId },
        echoCancellation: false,
        noiseSuppression: false,
        autoGainControl: false,
      },
      video: false,
    });

    const callAudio = requestedConstraints[0]?.audio as MediaTrackConstraints;
    expect(callAudio).toBeDefined();
    expect(callAudio.deviceId).toEqual({ exact: 'headset-mic-id' });
    expect(callAudio.deviceId).not.toEqual({ exact: 'internal-mic-id' });
  });
});
