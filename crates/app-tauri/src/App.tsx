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

export interface VirtualMicStatus {
  platform?: 'linux' | 'windows' | 'macos' | string;
  platform_label?: string;
  present: boolean;
  node_id: number | string | null;
  node_name: string;
  node_description: string;
  driver_status?: string;
  driver_bundle?: string;
  driver_installed?: boolean;
  install_inf?: string;
  is_default: boolean;
  format?: string;
  rate?: number;
  channels?: number;
  quantum?: number;
  error?: string;
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
  getVirtualMicStatus: () => Promise<VirtualMicStatus>;
  recreateVirtualMic: () => Promise<VirtualMicStatus>;
  setDefaultVirtualMic: () => Promise<VirtualMicStatus>;
  onStatusUpdate: (cb: (data: { mode?: DenoiseMode }) => void) => () => void;
  onVirtualMicUpdate: (cb: (data: VirtualMicStatus) => void) => () => void;
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
    if (cmd === 'get_virtual_mic_status') return (await api.getVirtualMicStatus()) as unknown as T;
    if (cmd === 'recreate_virtual_mic') return (await api.recreateVirtualMic()) as unknown as T;
    if (cmd === 'set_default_virtual_mic') return (await api.setDefaultVirtualMic()) as unknown as T;
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
  const [virtualMic, setVirtualMic] = useState<VirtualMicStatus | null>(null);
  const [isCheckingMic, setIsCheckingMic] = useState<boolean>(false);
  const [micActionMessage, setMicActionMessage] = useState<string | null>(null);

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

  const fetchVirtualMic = async () => {
    try {
      const res = await invokeBridge<VirtualMicStatus>('get_virtual_mic_status');
      if (res) {
        setVirtualMic(res);
      }
    } catch {
      // Fallback
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
    fetchVirtualMic();
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

    let cleanupMicListener: (() => void) | undefined;
    if (window.clearcoreApi?.onVirtualMicUpdate) {
      cleanupMicListener = window.clearcoreApi.onVirtualMicUpdate((mic) => {
        setVirtualMic(mic);
      });
    }

    const interval = setInterval(() => {
      fetchStatus();
      fetchVirtualMic();
    }, 2500);

    return () => {
      clearInterval(interval);
      if (cleanupTrayListener) cleanupTrayListener();
      if (cleanupMicListener) cleanupMicListener();
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

  const handleRecreateVirtualMic = async () => {
    setIsCheckingMic(true);
    const platLabel = virtualMic?.platform_label || 'Sistema';
    setMicActionMessage(`Verificando / Criando microfone virtual (${platLabel})...`);
    try {
      const res = await invokeBridge<VirtualMicStatus>('recreate_virtual_mic');
      setVirtualMic(res);
      if (res.present) {
        setMicActionMessage(`Microfone virtual criado e verificado com sucesso! (ID: ${res.node_id ?? 'Ativo'})`);
      } else {
        setMicActionMessage(`Não foi possível registrar o microfone virtual: ${res.error ?? 'Verifique permissões de Administrador/Root'}`);
      }
    } catch (err) {
      setMicActionMessage(`Erro ao criar microfone: ${String(err)}`);
    } finally {
      setIsCheckingMic(false);
      setTimeout(() => setMicActionMessage(null), 5000);
    }
  };

  const handleSetDefaultVirtualMic = async () => {
    setIsCheckingMic(true);
    setMicActionMessage('Definindo microfone virtual como padrão do sistema...');
    try {
      const res = await invokeBridge<VirtualMicStatus>('set_default_virtual_mic');
      setVirtualMic(res);
      if (res.is_default) {
        setMicActionMessage('Microfone virtual agora é o microfone padrão do sistema!');
      } else {
        setMicActionMessage('Tentativa enviada. Atualizando dispositivos do sistema...');
      }
    } catch (err) {
      setMicActionMessage(`Erro ao definir padrão: ${String(err)}`);
    } finally {
      setIsCheckingMic(false);
      setTimeout(() => setMicActionMessage(null), 5000);
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

  const platformTitle = virtualMic?.platform_label || 'Multi-Plataforma';
  const isWindows = virtualMic?.platform === 'windows';
  const isMac = virtualMic?.platform === 'macos';

  return (
    <div className="container">
      <header className="header">
        <div className="title-area">
          <h1>Clearcore / Orca Noise Suppression</h1>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
            Companion Desktop Multi-Plataforma (Linux, Windows, macOS) com Bandeja e Início Automático
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

      {micActionMessage && (
        <div style={{ padding: '10px 14px', background: '#142a1f', border: '1px solid #166534', borderRadius: 6, marginBottom: 16, color: '#86efac', fontSize: '0.9rem' }}>
          {micActionMessage}
        </div>
      )}

      {/* Modo de Operação */}
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

      {/* Microfone Virtual Multi-Plataforma */}
      <div className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 10 }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <h2 className="card-title" style={{ margin: 0 }}>Microfone Virtual do Sistema</h2>
              <span style={{ fontSize: '0.75rem', background: '#1e293b', border: '1px solid #334155', borderRadius: 4, padding: '2px 8px', color: '#93c5fd' }}>
                {platformTitle}
              </span>
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
              Dispositivo virtual de captura exposto para Discord, Teams, Zoom, OBS e navegadores.
            </div>
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              className="action-btn"
              disabled={isCheckingMic}
              onClick={handleRecreateVirtualMic}
              title="Verifica o subsistema de áudio e recria/instala o microfone virtual caso não tenha sido criado"
            >
              {isCheckingMic ? '⏳ Verificando...' : '🔄 Verificar / Criar'}
            </button>
            {virtualMic?.present && (
              <button
                className="action-btn"
                disabled={virtualMic.is_default || isCheckingMic}
                onClick={handleSetDefaultVirtualMic}
                style={{
                  borderColor: virtualMic.is_default ? '#166534' : 'var(--border-color)',
                  color: virtualMic.is_default ? '#4ade80' : 'var(--text-main)',
                }}
              >
                {virtualMic.is_default ? '✓ Padrão do Sistema' : '🎙 Tornar Padrão'}
              </button>
            )}
          </div>
        </div>

        <div className="grid-cols-2">
          <div className="metric-box">
            <div className="metric-label">
              {isWindows ? 'Status do Driver WaveRT / PnP' : isMac ? 'Status do Plug-In CoreAudio HAL' : 'Estado do Nó PipeWire'}
            </div>
            <div className="metric-value status-badge" style={{ color: virtualMic?.present ? '#4ade80' : '#f87171' }}>
              {virtualMic?.present ? `🟢 Ativo (ID: ${virtualMic.node_id ?? 'OK'})` : '🔴 Não Detectado no Sistema'}
            </div>
          </div>
          <div className="metric-box">
            <div className="metric-label">Padrão do Sistema / Dispositivo</div>
            <div className="metric-value" style={{ color: virtualMic?.is_default ? '#4ade80' : 'var(--text-muted)' }}>
              {virtualMic?.is_default ? 'Sim (Dispositivo Primário)' : 'Não (Dispositivo Secundário)'}
            </div>
          </div>
        </div>

        {virtualMic?.present ? (
          <div style={{ marginTop: 12, padding: '10px 14px', background: 'var(--bg-secondary)', borderRadius: 6, fontSize: '0.85rem', color: 'var(--text-muted)', display: 'flex', justifyContent: 'space-between', flexWrap: 'wrap', gap: 8 }}>
            <span><strong>Dispositivo:</strong> {virtualMic.node_name}</span>
            <span><strong>Formato:</strong> {virtualMic.format || 'F32LE'} @ {virtualMic.rate || 48000}Hz</span>
            <span><strong>Canais:</strong> {virtualMic.channels || 1} (Mono)</span>
            <span><strong>Latência:</strong> {virtualMic.quantum || 480} amostras (10ms)</span>
          </div>
        ) : (
          <div style={{ marginTop: 12, padding: '12px 14px', background: '#451a1a', border: '1px solid #7f1d1d', borderRadius: 6, color: '#fca5a5', fontSize: '0.85rem', lineHeight: 1.5 }}>
            <div style={{ fontWeight: 600, marginBottom: 4 }}>
              ⚠️ O microfone virtual não foi detectado no subsistema de áudio ({platformTitle}).
            </div>
            <div>
              {isWindows && (
                <span>
                  O driver WaveRT <code>Root\RealtimeNoise</code> não está ativo. Clique em <strong>"Verificar / Criar"</strong> ou execute como Administrador: <code>pnputil /add-driver platform\windows\driver\RealtimeNoise.inf /install</code>.
                </span>
              )}
              {isMac && (
                <span>
                  O bundle <code>RealtimeNoiseHAL.driver</code> não está em <code>/Library/Audio/Plug-Ins/HAL/</code>. Clique em <strong>"Verificar / Criar"</strong> para instalar e reiniciar o <code>coreaudiod</code>.
                </span>
              )}
              {!isWindows && !isMac && (
                <span>
                  O nó <code>realtime-noise-source</code> não foi criado no PipeWire. Clique em <strong>"Verificar / Criar"</strong> para registrar o nó e carregar o helper C nativo.
                </span>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Supervisor Status */}
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

      {/* Inicialização e Bandeja */}
      <div className="card">
        <h2 className="card-title">Configurações de Inicialização e Bandeja</h2>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', background: 'var(--bg-secondary)', padding: '12px 16px', borderRadius: 6, border: '1px solid var(--border-color)' }}>
          <div>
            <div style={{ fontWeight: 600, color: 'var(--text-main)', fontSize: '0.95rem' }}>
              Iniciar com o Sistema (Minimizado na Bandeja)
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 2 }}>
              Inicia o Clearcore silenciosamente na barra de tarefas ao ligar o computador ({platformTitle}).
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
          💡 Dica: Ao fechar ou minimizar esta janela, o Clearcore continuará ativo na bandeja do sistema. Clique com o botão direito no ícone da bandeja para trocar de modo instantaneamente, verificar o microfone virtual ou sair.
        </p>
      </div>

      <DiagnosticsPanel
        diagnostics={diagnostics}
        onRefresh={() => {
          fetchStatus();
          fetchVirtualMic();
          fetchDiagnostics();
        }}
      />
    </div>
  );
};

export default App;
