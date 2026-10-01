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

interface ClearcoreApi {
  getStatus: () => Promise<EngineStatus>;
  setMode: (mode: string) => Promise<unknown>;
  restartGeneration: () => Promise<unknown>;
  getDiagnostics: () => Promise<DiagnosticsData>;
  getAutostart: () => Promise<boolean>;
  setAutostart: (enabled: boolean) => Promise<boolean>;
  minimizeToTray: () => Promise<void>;
  quitApp: () => Promise<void>;
  onStatusUpdate: (cb: (data: { mode?: DenoiseMode }) => void) => () => void;
}

declare global {
  interface Window {
    clearcoreApi?: ClearcoreApi;
    __TAURI_INTERNALS__?: {
      invoke: <T>(cmd: string, args?: Record<string, unknown>) => Promise<T>;
    };
  }
}

async function invokeBridge<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (typeof window !== 'undefined' && window.clearcoreApi) {
    const api = window.clearcoreApi;
    if (cmd === 'get_status') return (await api.getStatus()) as unknown as T;
    if (cmd === 'set_mode') return (await api.setMode(String(args?.mode ?? 'Active'))) as unknown as T;
    if (cmd === 'restart_generation') return (await api.restartGeneration()) as unknown as T;
    if (cmd === 'get_diagnostics') return (await api.getDiagnostics()) as unknown as T;
    if (cmd === 'get_autostart') return (await api.getAutostart()) as unknown as T;
    if (cmd === 'set_autostart') return (await api.setAutostart(Boolean(args?.enabled))) as unknown as T;
    if (cmd === 'minimize_to_tray') return (await api.minimizeToTray()) as unknown as T;
    if (cmd === 'quit_app') return (await api.quitApp()) as unknown as T;
  }
  if (typeof window !== 'undefined' && window.__TAURI_INTERNALS__) {
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
  const [autostartEnabled, setAutostartEnabled] = useState<boolean>(false);

  const fetchStatus = async () => {
    try {
      const res = await invokeBridge<EngineStatus>('get_status');
      if (res && res.mode) {
        setStatus(res);
        setMode(res.mode);
        setIsConnected(true);
        setErrorMessage(null);
      }
    } catch (err) {
      setIsConnected(false);
      setErrorMessage(`Daemon unreachable: ${String(err)}`);
    }
  };

  const fetchDiagnostics = async () => {
    try {
      const res = await invokeBridge<DiagnosticsData>('get_diagnostics');
      if (res) {
        setDiagnostics(res);
      }
    } catch {
      // Diagnostics fallback
    }
  };

  const fetchAutostart = async () => {
    try {
      const enabled = await invokeBridge<boolean>('get_autostart');
      setAutostartEnabled(Boolean(enabled));
    } catch {
      // Not in electron or autostart unavailable
    }
  };

  useEffect(() => {
    fetchStatus();
    fetchDiagnostics();
    fetchAutostart();

    // Listen to real-time status push from electron tray if available
    let cleanupTrayListener: (() => void) | undefined;
    if (window.clearcoreApi?.onStatusUpdate) {
      cleanupTrayListener = window.clearcoreApi.onStatusUpdate((data) => {
        if (data.mode) {
          setMode(data.mode);
        }
      });
    }

    const interval = setInterval(() => {
      fetchStatus();
    }, 2500);

    return () => {
      clearInterval(interval);
      if (cleanupTrayListener) cleanupTrayListener();
    };
  }, []);

  const handleModeChange = async (newMode: DenoiseMode) => {
    try {
      await invokeBridge('set_mode', { mode: newMode });
      setMode(newMode);
    } catch (err) {
      setErrorMessage(`Failed to switch mode: ${String(err)}`);
    }
  };

  const handleRestartGeneration = async () => {
    try {
      await invokeBridge('restart_generation');
      await fetchStatus();
      await fetchDiagnostics();
    } catch (err) {
      setErrorMessage(`Restart generation failed: ${String(err)}`);
    }
  };

  const handleToggleAutostart = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const nextVal = e.target.checked;
    try {
      const res = await invokeBridge<boolean>('set_autostart', { enabled: nextVal });
      setAutostartEnabled(res);
    } catch (err) {
      setErrorMessage(`Failed to change autostart: ${String(err)}`);
    }
  };

  const handleMinimizeToTray = async () => {
    try {
      await invokeBridge('minimize_to_tray');
    } catch {
      // Ignore
    }
  };

  return (
    <div className="container">
      <header className="header">
        <div className="title-area">
          <h1>Clearcore / Orca Noise Suppression</h1>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
            Companion Desktop com Bandeja do Sistema e Início Automático
          </p>
        </div>
        <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
          <span className={`service-badge ${isConnected ? 'online' : 'offline'}`}>
            ● {isConnected ? 'Daemon Conectado' : 'Daemon Desconectado'}
          </span>
          <button
            className="action-btn"
            style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            title="Minimizar janela para a bandeja do sistema"
            onClick={handleMinimizeToTray}
          >
            📥 Minimizar para a Bandeja
          </button>
        </div>
      </header>

      {errorMessage && (
        <div style={{ padding: '10px 14px', background: '#451a1a', border: '1px solid #7f1d1d', borderRadius: 6, marginBottom: 16, color: '#fca5a5', fontSize: '0.9rem' }}>
          {errorMessage}
        </div>
      )}

      <div className="card">
        <h2 className="card-title">Modo de Operação de Supressão</h2>
        <div className="mode-group">
          <button
            className={`mode-btn ${mode === 'Active' ? 'active-mode' : ''}`}
            onClick={() => handleModeChange('Active')}
          >
            🛡 Ativo (DeepFilterNet3)
          </button>
          <button
            className={`mode-btn ${mode === 'Bypass' ? 'bypass-mode' : ''}`}
            onClick={() => handleModeChange('Bypass')}
          >
            🔄 Bypass (Passagem Direta)
          </button>
          <button
            className={`mode-btn ${mode === 'Mute' ? 'mute-mode' : ''}`}
            onClick={() => handleModeChange('Mute')}
          >
            🔇 Mudo (Silêncio Digital)
          </button>
        </div>
      </div>

      <div className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
          <h2 className="card-title" style={{ margin: 0 }}>Status do Supervisor & Motor de Áudio</h2>
          <button
            className="action-btn"
            disabled={!status?.can_restart}
            onClick={handleRestartGeneration}
          >
            Reiniciar Geração
          </button>
        </div>

        <div className="grid-cols-2">
          <div className="metric-box">
            <div className="metric-label">Estado do Supervisor</div>
            <div className="metric-value status-badge" style={{ color: status?.state === 'Running' ? '#4ade80' : '#f87171' }}>
              {status?.state ?? 'Unknown'}
            </div>
          </div>
          <div className="metric-box">
            <div className="metric-label">Falhas (Janela de 15m / Total)</div>
            <div className="metric-value">
              {status?.crash_count_15m ?? 0} / {status?.total_crashes ?? 0}
            </div>
          </div>
        </div>
      </div>

      <div className="card">
        <h2 className="card-title">Configurações de Inicialização e Bandeja</h2>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', background: 'var(--bg-secondary)', padding: '12px 16px', borderRadius: 6, border: '1px solid var(--border-color)' }}>
          <div>
            <div style={{ fontWeight: 600, color: 'var(--text-main)', fontSize: '0.95rem' }}>
              Iniciar com o Sistema (Minimizado na Bandeja)
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 2 }}>
              Inicia o Clearcore silenciosamente na barra de tarefas ao ligar o computador.
            </div>
          </div>
          <label style={{ display: 'flex', alignItems: 'center', cursor: 'pointer', gap: 8 }}>
            <input
              type="checkbox"
              checked={autostartEnabled}
              onChange={handleToggleAutostart}
              style={{ width: 18, height: 18, cursor: 'pointer', accentColor: '#22c55e' }}
            />
            <span style={{ fontSize: '0.9rem', color: autostartEnabled ? '#4ade80' : 'var(--text-muted)' }}>
              {autostartEnabled ? 'Ativado' : 'Desativado'}
            </span>
          </label>
        </div>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', marginTop: 10 }}>
          💡 Dica: Ao fechar ou minimizar esta janela, o Clearcore continuará ativo na bandeja do sistema. Clique com o botão direito no ícone da bandeja para trocar de modo instantaneamente ou sair.
        </p>
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
