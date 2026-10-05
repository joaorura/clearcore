import React, { useState, useEffect } from 'react';
import { useI18n } from './i18n';
import { invokeBridge } from './bridge';
import type { StudioPreset } from './types';

const STORAGE_PRESET_KEY = 'clearcore_studio_preset';
const STORAGE_ENROLLED_KEY = 'clearcore_voice_profile_enrolled';

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

export const StudioDspCard: React.FC = () => {
  const { t } = useI18n();

  const [activePreset, setActivePreset] = useState<StudioPreset>('Natural');
  const [isNeuralEqCalibrated, setIsNeuralEqCalibrated] = useState<boolean>(true);
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null);

  // Load preset on startup
  useEffect(() => {
    const loadPreset = async () => {
      try {
        const presetRes = await invokeBridge<StudioPreset>('get_studio_preset');
        if (presetRes && (presetRes === 'Natural' || presetRes === 'Podcast' || presetRes === 'Broadcast' || presetRes === 'Off')) {
          setActivePreset(presetRes);
        } else {
          const saved = localStorage.getItem(STORAGE_PRESET_KEY) as StudioPreset;
          if (saved) setActivePreset(saved);
        }

        const enrolled = localStorage.getItem(STORAGE_ENROLLED_KEY) === 'true';
        setIsNeuralEqCalibrated(enrolled);
      } catch {
        // Fallback
      }
    };
    loadPreset();
  }, []);

  const handleSelectPreset = async (preset: StudioPreset) => {
    setActivePreset(preset);
    try {
      localStorage.setItem(STORAGE_PRESET_KEY, preset);
      await invokeBridge('set_studio_preset', { preset });
    } catch {
      // Ignore
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
            const isBlockActive = activePreset !== 'Off' && block.activeInPresets.includes(activePreset);
            const paramText = block.params[activePreset] || block.params.Natural;

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
          <span className={`status-pill ${isNeuralEqCalibrated ? 'pill-active' : 'pill-pending'}`}>
            {isNeuralEqCalibrated ? t('studioDsp.neuralEqStatusCalibrated') : t('studioDsp.neuralEqStatusPending')}
          </span>
        </div>

        <div className="neural-eq-spectrum-visual">
          <div className="spectrum-label-row">
            <span>80 Hz (Rumble)</span>
            <span>250 Hz (Corpo)</span>
            <span>1 kHz (Presença)</span>
            <span>3.5 kHz (Clareza)</span>
            <span>10 kHz (Ar)</span>
          </div>
          <div className="spectrum-bars-row">
            <div className="spectrum-band">
              <div className="spectrum-bar" style={{ height: isNeuralEqCalibrated ? '40%' : '50%' }} />
              <span className="band-val">{isNeuralEqCalibrated ? '-1.5 dB' : '0.0 dB'}</span>
            </div>
            <div className="spectrum-band">
              <div className="spectrum-bar" style={{ height: isNeuralEqCalibrated ? '65%' : '50%' }} />
              <span className="band-val">{isNeuralEqCalibrated ? '+1.8 dB' : '0.0 dB'}</span>
            </div>
            <div className="spectrum-band">
              <div className="spectrum-bar" style={{ height: isNeuralEqCalibrated ? '55%' : '50%' }} />
              <span className="band-val">{isNeuralEqCalibrated ? '+0.6 dB' : '0.0 dB'}</span>
            </div>
            <div className="spectrum-band">
              <div className="spectrum-bar" style={{ height: isNeuralEqCalibrated ? '72%' : '50%' }} />
              <span className="band-val">{isNeuralEqCalibrated ? '+2.4 dB' : '0.0 dB'}</span>
            </div>
            <div className="spectrum-band">
              <div className="spectrum-bar" style={{ height: isNeuralEqCalibrated ? '60%' : '50%' }} />
              <span className="band-val">{isNeuralEqCalibrated ? '+1.1 dB' : '0.0 dB'}</span>
            </div>
          </div>
        </div>

        <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', margin: '10px 0 0 0', lineHeight: 1.4 }}>
          {t('studioDsp.neuralEqDetail')}
        </p>
      </div>
    </div>
  );
};
