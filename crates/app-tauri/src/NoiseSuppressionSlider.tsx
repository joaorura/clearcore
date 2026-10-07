import React, { useState, useEffect, useRef, useCallback } from 'react';
import { useI18n } from './i18n';
import { getFilterIntensity, setFilterIntensity } from './bridge';
import type { EngineStatus } from './types';
import { logger } from './utils/logger';

export interface NoiseSuppressionSliderProps {
  currentStatus?: EngineStatus | null;
  disabled?: boolean;
}

export const PRESET_ANCHORS = [
  { value: 0, key: 'filterIntensity.mild' as const, label: '0% Suave' },
  { value: 50, key: 'filterIntensity.standard' as const, label: '50% Padrão' },
  { value: 75, key: 'filterIntensity.aggressive' as const, label: '75% Agressivo' },
  { value: 100, key: 'filterIntensity.maximum' as const, label: '100% Máximo' },
];

export const NoiseSuppressionSlider: React.FC<NoiseSuppressionSliderProps> = ({
  currentStatus,
  disabled = false,
}) => {
  const { t } = useI18n();

  const [intensity, setIntensity] = useState<number>(() => {
    if (typeof currentStatus?.filter_intensity === 'number') {
      return currentStatus.filter_intensity;
    }
    const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_filter_intensity') : null;
    return saved ? Number(saved) : 50;
  });

  const [feedback, setFeedback] = useState<string | null>(null);
  const debounceTimerRef = useRef<NodeJS.Timeout | null>(null);
  const feedbackTimerRef = useRef<NodeJS.Timeout | null>(null);

  // Sync with currentStatus if updated externally by daemon poll
  useEffect(() => {
    if (typeof currentStatus?.filter_intensity === 'number') {
      setIntensity(currentStatus.filter_intensity);
    }
  }, [currentStatus?.filter_intensity]);

  // Initial load from bridge if status was missing
  useEffect(() => {
    let isMounted = true;
    const initIntensity = async () => {
      try {
        const val = await getFilterIntensity();
        if (isMounted && typeof val === 'number') {
          setIntensity(val);
        }
      } catch {
        // Fallback to local state
      }
    };
    initIntensity();

    return () => {
      isMounted = false;
      if (debounceTimerRef.current) clearTimeout(debounceTimerRef.current);
      if (feedbackTimerRef.current) clearTimeout(feedbackTimerRef.current);
    };
  }, []);

  const sendIntensityUpdate = useCallback(async (newVal: number) => {
    try {
      logger.info('UI', `Applying noise suppression filter intensity: ${newVal}%`);
      await setFilterIntensity(newVal);
      setFeedback(t('filterIntensity.feedback', { intensity: newVal }));
      if (feedbackTimerRef.current) clearTimeout(feedbackTimerRef.current);
      feedbackTimerRef.current = setTimeout(() => setFeedback(null), 2500);
    } catch (err) {
      logger.error('UI', `Failed to set filter intensity: ${err}`);
    }
  }, [t]);

  const handleSliderChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const val = Number(e.target.value);
    setIntensity(val);

    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }
    debounceTimerRef.current = setTimeout(() => {
      sendIntensityUpdate(val);
    }, 200);
  };

  const handleAnchorClick = (val: number) => {
    if (disabled) return;
    setIntensity(val);
    if (debounceTimerRef.current) {
      clearTimeout(debounceTimerRef.current);
    }
    sendIntensityUpdate(val);
  };

  const getAnchorDescription = (val: number) => {
    if (val === 0) return t('filterIntensity.mild');
    if (val === 50) return t('filterIntensity.standard');
    if (val === 75) return t('filterIntensity.aggressive');
    if (val === 100) return t('filterIntensity.maximum');
    return `${val}%`;
  };

  return (
    <div
      className="noise-suppression-intensity-block"
      style={{
        marginTop: 16,
        padding: '16px',
        background: 'var(--bg-secondary)',
        borderRadius: 8,
        border: '1px solid var(--border-color)',
      }}
      data-testid="noise-suppression-intensity"
    >
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 8,
          flexWrap: 'wrap',
          gap: 8,
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <span style={{ fontSize: '1.1rem' }}>🎚️</span>
          <span style={{ fontWeight: 600, fontSize: '0.95rem', color: 'var(--text-main)' }}>
            {t('filterIntensity.title')}
          </span>
        </div>
        <div
          style={{
            display: 'flex',
            alignItems: 'center',
            gap: 6,
            background: 'var(--bg-card)',
            padding: '3px 10px',
            borderRadius: 6,
            border: '1px solid var(--border-color)',
          }}
        >
          <span style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}>Nível:</span>
          <span
            style={{
              fontWeight: 700,
              fontSize: '0.95rem',
              color: intensity >= 75 ? '#38bdf8' : intensity >= 50 ? '#4ade80' : '#facc15',
            }}
            data-testid="intensity-value-display"
          >
            {intensity}%
          </span>
          <span style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}>
            ({getAnchorDescription(intensity)})
          </span>
        </div>
      </div>

      <p style={{ fontSize: '0.8rem', color: 'var(--text-muted)', marginBottom: 14 }}>
        {t('filterIntensity.description')}
      </p>

      {/* Main Slider Control */}
      <div style={{ position: 'relative', padding: '0 4px', marginBottom: 12 }}>
        <input
          type="range"
          min="0"
          max="100"
          step="1"
          value={intensity}
          disabled={disabled}
          onChange={handleSliderChange}
          aria-label={t('filterIntensity.ariaLabel')}
          data-testid="intensity-slider-input"
          style={{
            width: '100%',
            height: 6,
            borderRadius: 3,
            outline: 'none',
            background: `linear-gradient(to right, #22c55e 0%, #3b82f6 50%, #8b5cf6 75%, #ec4899 100%)`,
            cursor: disabled ? 'not-allowed' : 'pointer',
            opacity: disabled ? 0.5 : 1,
          }}
        />
      </div>

      {/* Anchor Marks Buttons */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(4, 1fr)',
          gap: 6,
        }}
      >
        {PRESET_ANCHORS.map((anchor) => {
          const isSelected = intensity === anchor.value;
          return (
            <button
              key={anchor.value}
              type="button"
              disabled={disabled}
              onClick={() => handleAnchorClick(anchor.value)}
              className="action-btn"
              style={{
                fontSize: '0.75rem',
                padding: '5px 4px',
                textAlign: 'center',
                background: isSelected ? 'var(--bg-card)' : 'transparent',
                borderColor: isSelected ? '#3b82f6' : 'var(--border-color)',
                color: isSelected ? '#93c5fd' : 'var(--text-muted)',
                fontWeight: isSelected ? 600 : 400,
                borderRadius: 4,
                cursor: disabled ? 'not-allowed' : 'pointer',
                transition: 'all 0.15s ease',
              }}
              data-testid={`anchor-btn-${anchor.value}`}
            >
              {t(anchor.key)}
            </button>
          );
        })}
      </div>

      {feedback && (
        <div
          className="feedback-banner success-banner"
          style={{ marginTop: 10, fontSize: '0.8rem', padding: '6px 12px' }}
        >
          ✓ {feedback}
        </div>
      )}
    </div>
  );
};
