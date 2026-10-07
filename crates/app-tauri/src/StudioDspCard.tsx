import React, { useState, useEffect } from 'react';
import { useI18n } from './i18n';
import { invokeBridge } from './bridge';
import type { StudioPreset } from './types';
import { logger } from './utils/logger';

const STORAGE_PRESET_KEY = 'clearcore_studio_preset';

interface DspBlockInfo {
  id: string;
  nameKey: string;
  descKey: string;
  icon: string;
  activeInPresets: StudioPreset[];
  params: { [preset in StudioPreset]?: string };
}

const DSP_BLOCKS: DspBlockInfo[] = [
  {
    id: 'highpass',
    nameKey: 'studioDsp.blockHighpass',
    descKey: 'studioDsp.blockHighpassDesc',
    icon: '⚡',
    activeInPresets: ['Natural', 'Podcast', 'Broadcast'],
    params: {
      Natural: '80 Hz • 18 dB/oct Butterworth',
      Podcast: '70 Hz • 12 dB/oct Suave',
      Broadcast: '90 Hz • 24 dB/oct Corte Agressivo',
      Off: 'Bypass Direto',
    },
  },
  {
    id: 'biquads',
    nameKey: 'studioDsp.blockBiquads',
    descKey: 'studioDsp.blockBiquadsDesc',
    icon: '🎚️',
    activeInPresets: ['Natural', 'Podcast', 'Broadcast'],
    params: {
      Natural: 'Presença +1.2 dB @ 3.2 kHz • Ar +1.0 dB @ 10 kHz',
      Podcast: 'Calor +2.5 dB @ 200 Hz • Presença +1.5 dB @ 2.8 kHz',
      Broadcast: 'Ataque +3.5 dB @ 3.5 kHz • Brilho +2.0 dB @ 8 kHz',
      Off: 'Bypass Direto',
    },
  },
  {
    id: 'deesser',
    nameKey: 'studioDsp.blockDeesser',
    descKey: 'studioDsp.blockDeesserDesc',
    icon: '🔇',
    activeInPresets: ['Natural', 'Podcast', 'Broadcast'],
    params: {
      Natural: 'Banda 6.5 kHz • Limiar -22 dB • Atenuação Suave',
      Podcast: 'Banda 6.0 kHz • Limiar -18 dB • Filtro Musical',
      Broadcast: 'Banda 6.2 kHz • Limiar -14 dB • Ação Rápida',
      Off: 'Bypass Direto',
    },
  },
  {
    id: 'compressor',
    nameKey: 'studioDsp.blockCompressor',
    descKey: 'studioDsp.blockCompressorDesc',
    icon: '🗜️',
    activeInPresets: ['Natural', 'Podcast', 'Broadcast'],
    params: {
      Natural: 'Razão 2:1 • Ataque 20ms • Release 100ms • Joelho Suave',
      Podcast: 'Razão 3:1 • Ataque 15ms • Release 80ms • Calor Analógico',
      Broadcast: 'Razão 4:1 • Ataque 5ms • Release 40ms • Densidade Vocal',
      Off: 'Bypass Direto',
    },
  },
  {
    id: 'agc',
    nameKey: 'studioDsp.blockAgc',
    descKey: 'studioDsp.blockAgcDesc',
    icon: '📊',
    activeInPresets: ['Natural', 'Podcast', 'Broadcast'],
    params: {
      Natural: 'Alvo -18 LUFS • Resposta Suave à Distância',
      Podcast: 'Alvo -16 LUFS • Dinâmica de Conversa Nivelada',
      Broadcast: 'Alvo -14 LUFS • Presença Firme Constante',
      Off: 'Bypass Direto',
    },
  },
  {
    id: 'limiter',
    nameKey: 'studioDsp.blockLimiter',
    descKey: 'studioDsp.blockLimiterDesc',
    icon: '🛡️',
    activeInPresets: ['Natural', 'Podcast', 'Broadcast'],
    params: {
      Natural: 'Teto -0.5 dB True-Peak • Zero Distorção',
      Podcast: 'Teto -0.5 dB True-Peak • Lookahead 96 amostras',
      Broadcast: 'Teto -0.2 dB True-Peak • Transmissão Segura',
      Off: 'Bypass Direto',
    },
  },
];

interface EqBandDef {
  id: string;
  label: string;
  calibratedHeight: string;
  calibratedDb: string;
}

const NEURAL_EQ_SPECTRUM_BANDS: EqBandDef[] = [
  { id: '80hz', label: '80 Hz (Rumble / Corte de Subgraves)', calibratedHeight: '40%', calibratedDb: '-1.2 dB' },
  { id: '250hz', label: '250 Hz (Corpo Vocal)', calibratedHeight: '65%', calibratedDb: '+2.1 dB' },
  { id: '1khz', label: '1 kHz (Presença)', calibratedHeight: '55%', calibratedDb: '+0.7 dB' },
  { id: '35khz', label: '3.5 kHz (Clareza)', calibratedHeight: '72%', calibratedDb: '+2.4 dB' },
  { id: '10khz', label: '10 kHz (Ar / Brilho)', calibratedHeight: '60%', calibratedDb: '+1.1 dB' },
];

/**
 * The microphone EQ counts as calibrated only when the SERVICE says so (`neural_eq_calibrated`
 * in the profile status pushed by the voice profile card). Never inferred from local storage.
 */
export function neuralEqCalibratedFrom(detail: unknown): boolean {
  return typeof detail === 'object' && detail !== null && (detail as { neural_eq_calibrated?: unknown }).neural_eq_calibrated === true;
}

export const StudioDspCard: React.FC = () => {
  const { t } = useI18n();

  const [activePreset, setActivePreset] = useState<StudioPreset>('Natural');
  const [voiceLeveler, setVoiceLeveler] = useState<number>(0);
  const [isNeuralEqCalibrated, setIsNeuralEqCalibrated] = useState<boolean>(false);
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null);

  const isEqEnabled = activePreset !== 'Off';
  const [lastActivePreset, setLastActivePreset] = useState<StudioPreset>(() => {
    const saved = typeof localStorage !== 'undefined' ? (localStorage.getItem(STORAGE_PRESET_KEY) as StudioPreset) : null;
    return saved && saved !== 'Off' ? saved : 'Natural';
  });

  const handleToggleEq = (enable: boolean) => {
    logger.info('StudioDspCard: toggling neural eq', { enable });
    if (enable) {
      const target = lastActivePreset === 'Off' ? 'Natural' : lastActivePreset;
      void handleSelectPreset(target);
      setFeedbackMessage(t('studioDsp.neuralEqEnabledFeedback'));
    } else {
      if (activePreset !== 'Off') {
        setLastActivePreset(activePreset);
      }
      void handleSelectPreset('Off');
      setFeedbackMessage(t('studioDsp.neuralEqDisabledFeedback'));
    }
  };

  const handleVoiceLevelerChange = async (newVal: number) => {
    const clamped = Math.max(0, Math.min(100, Math.round(newVal)));
    setVoiceLeveler(clamped);
    try {
      localStorage.setItem('clearcore_voice_leveler', String(clamped));
      await invokeBridge('set_voice_leveler', { intensity: clamped });
      logger.debug('StudioDspCard: set_voice_leveler bridge call succeeded', { clamped });
    } catch (err) {
      logger.error('StudioDspCard: failed to set_voice_leveler', { clamped, error: String(err) });
    }
    setFeedbackMessage(t('studioDsp.voiceLevelerFeedback', { intensity: clamped }));
    setTimeout(() => setFeedbackMessage(null), 3000);
  };

  // Load preset, leveler and calibration on startup, plus register reactive listeners
  useEffect(() => {
    const refreshCalibration = async () => {
      try {
        const res = await invokeBridge<unknown>('get_voice_profile');
        const status = (res as { profile?: unknown } | null)?.profile ?? res;
        setIsNeuralEqCalibrated(neuralEqCalibratedFrom(status));
      } catch {
        // unreachable service: stays not calibrated
      }
    };

    const loadVoiceLeveler = async () => {
      try {
        const res = await invokeBridge<number>('get_voice_leveler');
        if (typeof res === 'number' && !isNaN(res)) {
          setVoiceLeveler(Math.max(0, Math.min(100, Math.round(res))));
        } else {
          const saved = localStorage.getItem('clearcore_voice_leveler');
          if (saved !== null) {
            const parsed = Number(saved);
            if (!isNaN(parsed)) setVoiceLeveler(Math.max(0, Math.min(100, Math.round(parsed))));
          }
        }
      } catch {
        const saved = localStorage.getItem('clearcore_voice_leveler');
        if (saved !== null) {
          const parsed = Number(saved);
          if (!isNaN(parsed)) setVoiceLeveler(Math.max(0, Math.min(100, Math.round(parsed))));
        }
      }
    };

    const loadPreset = async () => {
      try {
        const presetRes = await invokeBridge<StudioPreset>('get_studio_preset');
        if (presetRes && (presetRes === 'Natural' || presetRes === 'Podcast' || presetRes === 'Broadcast' || presetRes === 'Off')) {
          setActivePreset(presetRes);
          if (presetRes !== 'Off') setLastActivePreset(presetRes);
        } else {
          const saved = localStorage.getItem(STORAGE_PRESET_KEY) as StudioPreset;
          if (saved) {
            setActivePreset(saved);
            if (saved !== 'Off') setLastActivePreset(saved);
          }
        }
      } catch {
        // Fallback
      }
      await refreshCalibration();
      await loadVoiceLeveler();
    };
    loadPreset();

    const handleProfileUpdate = (e: Event) => {
      setIsNeuralEqCalibrated(neuralEqCalibratedFrom((e as CustomEvent<unknown>).detail));
    };
    window.addEventListener('clearcore_profile_updated', handleProfileUpdate);

    // Also register onVoiceProfileUpdate bridge listener if available
    const api = typeof window !== 'undefined' ? window.clearcoreApi : undefined;
    const cleanupBridgeListener = api?.onVoiceProfileUpdate?.((profile) => {
      setIsNeuralEqCalibrated(neuralEqCalibratedFrom(profile));
    });

    return () => {
      window.removeEventListener('clearcore_profile_updated', handleProfileUpdate);
      if (cleanupBridgeListener) cleanupBridgeListener();
    };
  }, []);

  const handleSelectPreset = async (preset: StudioPreset) => {
    logger.info('StudioDspCard: user selected DSP preset', { preset, previousPreset: activePreset });
    if (preset !== 'Off') {
      setLastActivePreset(preset);
    }
    setActivePreset(preset);
    try {
      localStorage.setItem(STORAGE_PRESET_KEY, preset);
      await invokeBridge('set_studio_preset', { preset });
      logger.debug('StudioDspCard: set_studio_preset bridge call succeeded', { preset });
    } catch (err) {
      logger.error('StudioDspCard: failed to set_studio_preset', { preset, error: String(err) });
    }

    const presetNames: { [k in StudioPreset]: string } = {
      Natural: t('studioDsp.presetNatural'),
      Podcast: t('studioDsp.presetWarm'),
      Broadcast: t('studioDsp.presetBroadcast'),
      Off: t('studioDsp.presetOff'),
    };

    setFeedbackMessage(t('studioDsp.presetSwitchedFeedback', { name: presetNames[preset] }));
    setTimeout(() => setFeedbackMessage(null), 3000);
  };

  const presetsConfig: {
    id: StudioPreset;
    titleKey: string;
    descKey: string;
    tag: string;
  }[] = [
    {
      id: 'Natural',
      titleKey: 'studioDsp.presetNatural',
      descKey: 'studioDsp.presetNaturalDesc',
      tag: 'Reuniões & Chamadas',
    },
    {
      id: 'Podcast',
      titleKey: 'studioDsp.presetWarm',
      descKey: 'studioDsp.presetWarmDesc',
      tag: 'Gravações & Podcasts',
    },
    {
      id: 'Broadcast',
      titleKey: 'studioDsp.presetBroadcast',
      descKey: 'studioDsp.presetBroadcastDesc',
      tag: 'Lives & Transmissão',
    },
    {
      id: 'Off',
      titleKey: 'studioDsp.presetOff',
      descKey: 'studioDsp.presetOffDesc',
      tag: 'Bypass Total',
    },
  ];

  return (
    <div className="card studio-dsp-card">
      {/* Header */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', flexWrap: 'wrap', gap: 12, marginBottom: 16 }}>
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap' }}>
            <h2 className="card-title" style={{ margin: 0 }}>{t('studioDsp.title')}</h2>
            <span className="dsp-badge-latency">
              {t('studioDsp.badge')}
            </span>
          </div>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 6, lineHeight: 1.45 }}>
            {t('studioDsp.description')}
          </p>
        </div>
      </div>

      {feedbackMessage && (
        <div className="feedback-banner success-banner">
          {feedbackMessage}
        </div>
      )}

      {/* 4 Studio Presets Grid */}
      <div style={{ marginBottom: 20 }}>
        <div style={{ fontSize: '0.9rem', fontWeight: 600, color: 'var(--text-main)', marginBottom: 10 }}>
          {t('studioDsp.presetLabel')}
        </div>
        <div className="presets-grid">
          {presetsConfig.map((p) => {
            const isSelected = activePreset === p.id;
            return (
              <div
                key={p.id}
                className={`preset-card ${isSelected ? 'preset-active' : ''}`}
                onClick={() => handleSelectPreset(p.id)}
              >
                <div className="preset-card-top">
                  <span className="preset-name">{t(p.titleKey)}</span>
                  <span className={`preset-tag ${isSelected ? 'tag-active' : ''}`}>{p.tag}</span>
                </div>
                <p className="preset-desc">{t(p.descKey)}</p>
                <div className="preset-selection-indicator">
                  {isSelected ? '✓ Ativo no Áudio' : 'Selecionar'}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Signal Chain Flowchart & DSP Blocks */}
      <div className="signal-chain-container">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 8 }}>
          <div>
            <div style={{ fontWeight: 600, fontSize: '0.95rem', color: '#93c5fd' }}>
              {t('studioDsp.signalChainTitle')}
            </div>
            <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginTop: 2 }}>
              {t('studioDsp.signalChainDesc')}
            </div>
          </div>
          <div className="flow-badge-step">
            Entrada IA → 6 Blocos DSP → Saída Virtual (96 amostras / 2ms)
          </div>
        </div>

        {/* DSP Blocks Grid */}
        <div className="dsp-blocks-grid">
          {DSP_BLOCKS.map((block, idx) => {
            const isBlockActive = block.id === 'agc'
              ? voiceLeveler > 0
              : (block.id === 'limiter' && voiceLeveler > 0 && activePreset === 'Off')
                ? true
                : (activePreset !== 'Off' && block.activeInPresets.includes(activePreset));

            let paramText = block.params[activePreset] || block.params.Natural;
            if (block.id === 'agc') {
              if (voiceLeveler > 0) {
                const targetLufs = (-20.0 + (voiceLeveler / 100) * 8.0).toFixed(0);
                paramText = `${voiceLeveler}% • Alvo ${targetLufs} LUFS`;
              } else if (activePreset === 'Off') {
                paramText = t('studioDsp.voiceLevelerOff');
              }
            }

            return (
              <div
                key={block.id}
                className={`dsp-block-card ${isBlockActive ? 'block-enabled' : 'block-bypassed'}`}
              >
                <div className="dsp-block-header">
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <span className="block-step-num">{idx + 1}</span>
                    <span className="block-icon">{block.icon}</span>
                    <strong className="block-title">{t(block.nameKey)}</strong>
                  </div>
                  <span className={`block-status-pill ${isBlockActive ? 'pill-on' : 'pill-off'}`}>
                    {isBlockActive ? t('studioDsp.blockActive') : t('studioDsp.blockBypassed')}
                  </span>
                </div>

                <div className="dsp-block-desc">{t(block.descKey)}</div>

                <div className="dsp-block-param-box">
                  <span className="param-label">Ajuste Atual:</span>
                  <span className="param-value">{paramText}</span>
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Voice Auto-Leveler (AGC) Section */}
      <div className="voice-leveler-box" data-testid="voice-leveler-section">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 10 }}>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: '1.2rem' }}>🎚️</span>
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.95rem', color: 'var(--text-main)' }}>
                {t('studioDsp.voiceLevelerSectionTitle')}
              </div>
              <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {t('studioDsp.voiceLevelerDesc')}
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span
              className={`block-status-pill ${voiceLeveler > 0 ? 'pill-on' : 'pill-off'}`}
              data-testid="voice-leveler-badge"
            >
              {voiceLeveler === 0
                ? t('studioDsp.voiceLevelerOff')
                : `${voiceLeveler}% • ${
                    voiceLeveler <= 35
                      ? t('studioDsp.voiceLevelerGentle')
                      : voiceLeveler <= 70
                      ? t('studioDsp.voiceLevelerBalanced')
                      : t('studioDsp.voiceLevelerFirm')
                  }`}
            </span>
          </div>
        </div>

        {/* Main Slider Control */}
        <div style={{ position: 'relative', padding: '4px 0', marginBottom: 12 }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 6 }}>
            <label
              htmlFor="voice-leveler-slider"
              style={{ fontSize: '0.8rem', fontWeight: 500, color: 'var(--text-main)' }}
            >
              {t('studioDsp.voiceLevelerSliderLabel')}
            </label>
            <span
              style={{
                fontSize: '0.85rem',
                fontWeight: 700,
                color: voiceLeveler > 0 ? '#38bdf8' : 'var(--text-muted)',
              }}
              data-testid="voice-leveler-value"
            >
              {voiceLeveler}%
            </span>
          </div>
          <input
            id="voice-leveler-slider"
            type="range"
            min="0"
            max="100"
            step="1"
            value={voiceLeveler}
            onChange={(e) => handleVoiceLevelerChange(Number(e.target.value))}
            aria-label={t('studioDsp.voiceLevelerSliderLabel')}
            data-testid="voice-leveler-slider-input"
            style={{
              width: '100%',
              height: 6,
              borderRadius: 3,
              outline: 'none',
              background: 'linear-gradient(to right, #334155 0%, #3b82f6 50%, #38bdf8 100%)',
              cursor: 'pointer',
            }}
          />
        </div>

        {/* Quick Anchor Points */}
        <div style={{ display: 'grid', gridTemplateColumns: 'repeat(4, 1fr)', gap: 8 }}>
          {[
            { value: 0, label: t('studioDsp.voiceLevelerOff') },
            { value: 30, label: `30% • ${t('studioDsp.voiceLevelerGentle')}` },
            { value: 50, label: `50% • ${t('studioDsp.voiceLevelerBalanced')}` },
            { value: 100, label: `100% • ${t('studioDsp.voiceLevelerFirm')}` },
          ].map((anchor) => {
            const isSelected = voiceLeveler === anchor.value;
            return (
              <button
                key={anchor.value}
                type="button"
                onClick={() => handleVoiceLevelerChange(anchor.value)}
                className="action-btn"
                style={{
                  fontSize: '0.75rem',
                  padding: '5px 4px',
                  textAlign: 'center',
                  background: isSelected ? 'rgba(59, 130, 246, 0.15)' : 'rgba(30, 41, 59, 0.4)',
                  borderColor: isSelected ? '#3b82f6' : 'var(--border-color)',
                  color: isSelected ? '#93c5fd' : 'var(--text-muted)',
                  fontWeight: isSelected ? 600 : 400,
                  borderRadius: 4,
                  cursor: 'pointer',
                  transition: 'all 0.15s ease',
                }}
                data-testid={`voice-leveler-anchor-${anchor.value}`}
              >
                {anchor.label}
              </button>
            );
          })}
        </div>
      </div>

      {/* Neural EQ Calibration Status Section */}
      <div className="neural-eq-box">
        <div className="neural-eq-header">
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <span style={{ fontSize: '1.2rem' }}>🧠</span>
            <div>
              <div style={{ fontWeight: 600, fontSize: '0.95rem', color: 'var(--text-main)' }}>
                {t('studioDsp.neuralEqSectionTitle')}
              </div>
              <div style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>
                {t('studioDsp.neuralEqDesc')}
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span className={`status-pill ${isNeuralEqCalibrated ? 'pill-active' : 'pill-pending'}`}>
              {isNeuralEqCalibrated ? t('studioDsp.neuralEqStatusCalibrated') : t('studioDsp.neuralEqStatusPending')}
            </span>
            <label className="toggle-switch-wrapper" title={t('studioDsp.neuralEqSwitch')}>
              <input
                type="checkbox"
                checked={isEqEnabled}
                onChange={(e) => handleToggleEq(e.target.checked)}
              />
              <span className="toggle-switch-slider" />
            </label>
            <span className={`toggle-status-pill ${isEqEnabled ? 'pill-on' : 'pill-off'}`}>
              {isEqEnabled ? t('studioDsp.neuralEqEnabled') : t('studioDsp.neuralEqDisabled')}
            </span>
          </div>
        </div>

        {/* Calibration Info Card (displays when calibrated by the service) */}
        {isNeuralEqCalibrated && isEqEnabled && (
          <div
            className="neural-eq-calibrated-card"
            style={{
              background: 'rgba(34, 197, 94, 0.08)',
              border: '1px solid rgba(34, 197, 94, 0.25)',
              borderRadius: 6,
              padding: '12px 14px',
              marginTop: 10,
              marginBottom: 10,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
              <span style={{ fontSize: '0.85rem' }}>🎯</span>
              <strong style={{ fontSize: '0.85rem', color: '#4ade80' }}>
                {t('studioDsp.neuralEqCardCalibratedBadge')}
              </strong>
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', margin: 0, lineHeight: 1.45 }}>
              {t('studioDsp.neuralEqCardActiveDesc')}
            </p>
          </div>
        )}

        {isNeuralEqCalibrated && !isEqEnabled && (
          <div
            role="status"
            className="neural-eq-bypassed-card"
            style={{
              background: 'rgba(148, 163, 184, 0.08)',
              border: '1px solid rgba(148, 163, 184, 0.2)',
              borderRadius: 6,
              padding: '12px 14px',
              marginTop: 10,
              marginBottom: 10,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 8, marginBottom: 4 }}>
              <span style={{ fontSize: '0.85rem' }}>⏸️</span>
              <strong style={{ fontSize: '0.85rem', color: '#94a3b8' }}>
                {t('studioDsp.neuralEqDisabled')}
              </strong>
            </div>
            <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', margin: 0, lineHeight: 1.45 }}>
              {t('studioDsp.neuralEqBypassedDesc')}
            </p>
          </div>
        )}

        {/* Neural EQ Spectrum Visualization */}
        <div className="neural-eq-spectrum-visual">
          <div className="spectrum-label-row">
            {NEURAL_EQ_SPECTRUM_BANDS.map((band) => (
              <span key={band.id}>{band.label}</span>
            ))}
          </div>
          <div className="spectrum-bars-row">
            {NEURAL_EQ_SPECTRUM_BANDS.map((band) => {
              const isVisualCalibrated = isNeuralEqCalibrated && isEqEnabled;
              return (
                <div
                  key={band.id}
                  className={`spectrum-band ${isVisualCalibrated ? 'band-calibrated' : 'band-neutral'}`}
                >
                  <div
                    className={`spectrum-bar ${isVisualCalibrated ? 'bar-calibrated' : 'bar-neutral'}`}
                    style={{ height: isVisualCalibrated ? band.calibratedHeight : '50%' }}
                  />
                  <span className={`band-val ${isVisualCalibrated ? 'val-calibrated' : 'val-neutral'}`}>
                    {isVisualCalibrated ? band.calibratedDb : '0.0 dB'}
                  </span>
                </div>
              );
            })}
          </div>
        </div>

        <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', margin: '10px 0 0 0', lineHeight: 1.4 }}>
          {t('studioDsp.neuralEqDetail')}
        </p>
      </div>
    </div>
  );
};
