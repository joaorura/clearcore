import type { CallSuggestionTake } from '../../types';
import { formatSecondsLocale } from '../format';
import type { Translate } from '../hooks/voiceProfileLogic';

export interface CallsPanelProps {
  t: Translate;
  locale: string;
  /** Takes suggested by the service (empty when none; nothing is invented here). */
  takes: CallSuggestionTake[];
  playingAudioId: string | null;
  /** Translated error of an approval the user asked for. */
  errorText: string | null;
  /** Takes whose approval is in flight (approve disabled: no double approval). */
  approvingIds?: readonly string[];
  onPlay: (id: string, audioUrl?: string) => void;
  onApprove: (take: CallSuggestionTake) => void;
  onDismiss: (id: string) => void;
}

export function CallsPanel(p: CallsPanelProps) {
  const { t, locale } = p;
  return (
    <div>
      <div style={{ marginBottom: 14 }}>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', margin: 0, lineHeight: 1.45 }}>
          {t('voiceProfile.callSuggestionsDesc')}
        </p>
      </div>

      {p.errorText && (
        <div role="alert" style={{ color: '#f87171', fontSize: 13, marginBottom: 12 }}>{p.errorText}</div>
      )}

      {p.takes.length > 0 ? (
        <div className="intake-takes-list">
          {p.takes.map((take) => (
            <div key={take.id} className="intake-take-card">
              <div className="intake-take-info">
                <div className="take-title-row">
                  <span className="take-badge-live">{t('voiceProfile.takeBadge')}</span>
                  {take.title && <strong style={{ fontSize: '0.95rem' }}>{take.title}</strong>}
                </div>
                <div className="take-meta-row">
                  <span>🕒 {take.timestamp}</span>
                  {typeof take.speech_seconds === 'number' && (
                    <span>⏱ {t('voiceProfile.takeSpeech', { sec: formatSecondsLocale(take.speech_seconds, locale) })}</span>
                  )}
                  {typeof take.speech_seconds !== 'number' && typeof take.durationSec === 'number' && (
                    <span>⏱ {t('voiceProfile.takeDuration', { sec: formatSecondsLocale(take.durationSec, locale) })}</span>
                  )}
                  {typeof take.snrDb === 'number' && (
                    <span className="take-snr-badge">{t('voiceProfile.takeSnr', { snr: formatSecondsLocale(take.snrDb, locale) })}</span>
                  )}
                  {take.device_label && <span>🎙️ {take.device_label}</span>}
                </div>
              </div>

              <div className="intake-take-actions">
                {take.audioUrl && (
                  <button className="action-btn take-play-btn" onClick={() => p.onPlay(take.id, take.audioUrl)}>
                    {p.playingAudioId === take.id ? t('voiceProfile.stopSample') : t('voiceProfile.playSample')}
                  </button>
                )}
                <button className="action-btn take-approve-btn" disabled={p.approvingIds?.includes(take.id) ?? false} onClick={() => p.onApprove(take)}>
                  {t('voiceProfile.approveTake')}
                </button>
                <button className="action-btn take-dismiss-btn" onClick={() => p.onDismiss(take.id)}>
                  {t('voiceProfile.dismissTake')}
                </button>
              </div>
            </div>
          ))}
        </div>
      ) : (
        <div className="empty-state-card">{t('voiceProfile.emptyCallSuggestions')}</div>
      )}
    </div>
  );
}
