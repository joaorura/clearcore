import type { Translate } from '../hooks/voiceProfileLogic';

export interface DeviceSwitchInfo { newLabel: string; oldLabel: string }

/** Asked before a sample from another microphone is sent (spec §4.4, D7). */
export function DeviceSwitchDialog({ t, info, onAnswer }: { t: Translate; info: DeviceSwitchInfo; onAnswer: (ok: boolean) => void }) {
  return (
    <div className="modal-overlay" role="dialog" aria-modal="true" aria-labelledby="device-switch-title">
      <div className="modal-content">
        <h3 id="device-switch-title" style={{ fontSize: '1.1rem', marginBottom: 8, color: 'var(--text-main)' }}>
          {t('voiceProfile.deviceSwitchTitle')}
        </h3>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginBottom: 16, lineHeight: 1.45 }}>
          {t('voiceProfile.deviceSwitchBody', { newLabel: info.newLabel || '?', oldLabel: info.oldLabel || '?' })}
        </p>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10 }}>
          <button className="action-btn" onClick={() => onAnswer(false)}>{t('voiceProfile.deviceSwitchCancel')}</button>
          <button className="action-btn primary-next-btn" onClick={() => onAnswer(true)}>{t('voiceProfile.deviceSwitchConfirm')}</button>
        </div>
      </div>
    </div>
  );
}
