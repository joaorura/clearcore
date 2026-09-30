import React, { useState, useEffect } from 'react';
import { DiagnosticsPanel, DiagnosticsData } from './diagnostics';

export type DenoiseMode = 'Active' | 'Bypass' | 'Mute';

interface EngineStatus {
  state: string;
  is_terminal: boolean;
  can_restart: boolean;
  mode: DenoiseMode;
  crash_count_15m: number;
  total_crashes: number;
}

async function invokeTauri<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window) {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<T>(cmd, args);
  }
  return {} as T;
}

export const App: React.FC = () => {
  const [mode, setMode] = useState<DenoiseMode>('Active');
  const [status, setStatus] = useState<EngineStatus | null>({
    state: 'Running',
    is_terminal: false,
    can_restart: true,
    mode: 'Active',
    crash_count_15m: 0,
    total_crashes: 0,
  });
  const [diagnostics, setDiagnostics] = useState<DiagnosticsData | null>(null);
  const [isConnected, setIsConnected] = useState<boolean>(true);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const fetchStatus = async () => {
    try {
      const res = await invokeTauri<EngineStatus>('get_status');
      if (res && res.mode) {
        setStatus(res);
        setMode(res.mode);
        setIsConnected(true);
        setErrorMessage(null);
      }
    } catch (err) {
      setIsConnected(false);
      setErrorMessage(`Service unreachable: ${String(err)}`);
    }
  };

  const fetchDiagnostics = async () => {
    try {
      const res = await invokeTauri<DiagnosticsData>('get_diagnostics');
      if (res) {
        setDiagnostics(res);
      }
    } catch {
      // Diagnostics fallback
    }
  };

  useEffect(() => {
    fetchStatus();
    fetchDiagnostics();
    const interval = setInterval(() => {
      fetchStatus();
    }, 3000);
    return () => clearInterval(interval);
  }, []);

  const handleModeChange = async (newMode: DenoiseMode) => {
    try {
      await invokeTauri('set_mode', { mode: newMode });
      setMode(newMode);
    } catch (err) {
      setErrorMessage(`Failed to switch mode: ${String(err)}`);
    }
  };

  const handleRestartGeneration = async () => {
    try {
      await invokeTauri('restart_generation');
      await fetchStatus();
      await fetchDiagnostics();
    } catch (err) {
      setErrorMessage(`Restart generation failed: ${String(err)}`);
    }
  };

  return (
    <div className="container">
      <header className="header">
        <div className="title-area">
          <h1>Realtime Noise Suppression</h1>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
            Control Companion & Diagnostics (Zero Audio Processing in UI)
          </p>
        </div>
        <div>
          <span className={`service-badge ${isConnected ? 'online' : 'offline'}`}>
            ● {isConnected ? 'Service Connected' : 'Service Disconnected'}
          </span>
        </div>
      </header>

      {errorMessage && (
        <div style={{ padding: '10px 14px', background: '#451a1a', border: '1px solid #7f1d1d', borderRadius: 6, marginBottom: 16, color: '#fca5a5', fontSize: '0.9rem' }}>
          {errorMessage}
        </div>
      )}

      <div className="card">
        <h2 className="card-title">Suppression Operating Mode</h2>
        <div className="mode-group">
          <button
            className={`mode-btn ${mode === 'Active' ? 'active-mode' : ''}`}
            onClick={() => handleModeChange('Active')}
          >
            🛡 Active
          </button>
          <button
            className={`mode-btn ${mode === 'Bypass' ? 'bypass-mode' : ''}`}
            onClick={() => handleModeChange('Bypass')}
          >
            🔄 Bypass (Pass-Through)
          </button>
          <button
            className={`mode-btn ${mode === 'Mute' ? 'mute-mode' : ''}`}
            onClick={() => handleModeChange('Mute')}
          >
            🔇 Digital Mute
          </button>
        </div>
      </div>

      <div className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
          <h2 className="card-title" style={{ margin: 0 }}>Supervisor Status</h2>
          <button
            className="action-btn"
            disabled={!status?.can_restart}
            onClick={handleRestartGeneration}
          >
            Restart Generation
          </button>
        </div>

        <div className="grid-cols-2">
          <div className="metric-box">
            <div className="metric-label">Supervisor State</div>
            <div className="metric-value status-badge" style={{ color: status?.state === 'Running' ? '#4ade80' : '#f87171' }}>
              {status?.state ?? 'Unknown'}
            </div>
          </div>
          <div className="metric-box">
            <div className="metric-label">Crashes (15m Window / Total)</div>
            <div className="metric-value">
              {status?.crash_count_15m ?? 0} / {status?.total_crashes ?? 0}
            </div>
          </div>
        </div>
      </div>

      <DiagnosticsPanel
        diagnostics={diagnostics}
        onRefresh={() => {
          fetchStatus();
          fetchDiagnostics();
        }}
      />
    </div>
  );
};
export default App;
