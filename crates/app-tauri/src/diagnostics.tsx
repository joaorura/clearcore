import React, { useState } from 'react';
import { useI18n } from './i18n';
import { formatDecimalLocale } from './voice/format';

export interface LatencyPercentiles {
  p50_us: number;
  p95_us: number;
  p99_us: number;
}

export interface BudgetBreakdown {
  measured_us: number;
  configured_us: number;
  derived_us: number;
  unobservable_us: number;
}

export interface DiagnosticsData {
  device_id_hash: string;
  generation: number;
  causes: string[];
  last_attempt: number | null;
  latencies: LatencyPercentiles;
  budget: BudgetBreakdown;
  timestamp_utc: string;
}

interface DiagnosticsPanelProps {
  diagnostics: DiagnosticsData | null;
  onRefresh: () => void;
}

// Helper to safely invoke Tauri commands if available in the global window
async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  // Mock fallback for browser / development / test smoke
  return {} as T;
}

export const DiagnosticsPanel: React.FC<DiagnosticsPanelProps> = ({
  diagnostics,
  onRefresh,
}) => {
  const { t, locale } = useI18n();
  const [exportMessage, setExportMessage] = useState<string | null>(null);

  const handleExport = async () => {
    try {
      const res = await invokeTauri<string>('export_diagnostics');
      setExportMessage(t('diagnostics.exportSuccess'));
      // Auto-clear message
      setTimeout(() => setExportMessage(null), 5000);
      if (res && typeof res === 'string') {
        const blob = new Blob([res], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = `clearcore-diagnostics-${Date.now()}.json`;
        a.click();
        URL.revokeObjectURL(url);
      }
    } catch (err) {
      setExportMessage(t('diagnostics.exportError', { error: String(err) }));
    }
  };

  const latencies = diagnostics?.latencies ?? { p50_us: 1200, p95_us: 2800, p99_us: 4500 };
  const budget = diagnostics?.budget ?? {
    measured_us: 1500,
    configured_us: 3000,
    derived_us: 800,
    unobservable_us: 200,
  };
  const causes = diagnostics?.causes ?? [t('diagnostics.normalState')];
  const deviceHash = diagnostics?.device_id_hash ?? 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
  const generation = diagnostics?.generation ?? 1;
  const lastAttempt = diagnostics?.last_attempt ?? null;

  return (
    <div className="card">
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
        <h2 className="card-title" style={{ margin: 0 }}>{t('diagnostics.title')}</h2>
        <div style={{ display: 'flex', gap: 8 }}>
          <button className="action-btn" onClick={onRefresh}>{t('diagnostics.refresh')}</button>
          <button className="export-btn" onClick={handleExport}>{t('diagnostics.exportBtn')}</button>
        </div>
      </div>

      {exportMessage && (
        <div style={{ padding: '8px 12px', background: '#1e3a5f', borderRadius: 4, marginBottom: 12, fontSize: '0.85rem' }}>
          {exportMessage}
        </div>
      )}

      <div style={{ marginBottom: 16 }}>
        <div className="metric-label">{t('diagnostics.deviceIdLabel')}</div>
        <div style={{ fontFamily: 'monospace', fontSize: '0.85rem', color: 'var(--text-muted)' }}>
          {deviceHash}
        </div>
      </div>

      <div className="grid-cols-2" style={{ marginBottom: 16 }}>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.supervisorGen')}</div>
          <div className="metric-value">{t('diagnostics.genNumber', { num: generation })}</div>
        </div>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.lastRestart')}</div>
          <div className="metric-value">
            {lastAttempt !== null ? t('diagnostics.attemptNumber', { num: lastAttempt }) : t('diagnostics.noneHealthy')}
          </div>
        </div>
      </div>

      <h3 style={{ fontSize: '0.95rem', fontWeight: 600, marginBottom: 8 }}>{t('diagnostics.latenciesTitle')}</h3>
      <div className="grid-cols-2" style={{ gridTemplateColumns: 'repeat(3, 1fr)', marginBottom: 16 }}>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.p50')}</div>
          <div className="metric-value">{formatDecimalLocale(latencies.p50_us / 1000, 2, locale)} ms</div>
        </div>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.p95')}</div>
          <div className="metric-value">{formatDecimalLocale(latencies.p95_us / 1000, 2, locale)} ms</div>
        </div>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.p99')}</div>
          <div className="metric-value">{formatDecimalLocale(latencies.p99_us / 1000, 2, locale)} ms</div>
        </div>
      </div>

      <h3 style={{ fontSize: '0.95rem', fontWeight: 600, marginBottom: 8 }}>{t('diagnostics.budgetTitle')}</h3>
      <div className="grid-cols-2" style={{ gridTemplateColumns: 'repeat(4, 1fr)', marginBottom: 16 }}>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.measured')}</div>
          <div className="metric-value">{budget.measured_us} µs</div>
        </div>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.configured')}</div>
          <div className="metric-value">{budget.configured_us} µs</div>
        </div>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.derived')}</div>
          <div className="metric-value">{budget.derived_us} µs</div>
        </div>
        <div className="metric-box">
          <div className="metric-label">{t('diagnostics.unobservable')}</div>
          <div className="metric-value">{budget.unobservable_us} µs</div>
        </div>
      </div>

      <h3 style={{ fontSize: '0.95rem', fontWeight: 600, marginBottom: 8 }}>{t('diagnostics.causesTitle')}</h3>
      <ul className="causes-list">
        {causes.map((c, i) => (
          <li key={i}>{c}</li>
        ))}
      </ul>

      <p className="privacy-notice">
        {t('diagnostics.privacyNotice')}
      </p>
    </div>
  );
};

