import React, { useState, useEffect } from 'react';
import { DiagnosticsPanel, DiagnosticsData } from './diagnostics';
import { AudioTestCard } from './AudioTestCard';
import { HardwareAcceleratorCard } from './HardwareAcceleratorCard';
import { useI18n } from './i18n';
import { invokeBridge } from './bridge';
import type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo } from './types';

export type { DenoiseMode, EngineStatus, VirtualMicStatus, InputDeviceInfo };


export const App: React.FC = () => {
  const { t, locale, setLocale, availableLocales } = useI18n();
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

  const [inputDevices, setInputDevices] = useState<InputDeviceInfo[]>([]);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string>('');
  const [isLoadingDevices, setIsLoadingDevices] = useState<boolean>(false);

  const fetchInputDevices = async () => {
    setIsLoadingDevices(true);
    let devList: InputDeviceInfo[] = [];

    // 1. Try native backend enumeration via IPC bridge
    try {
      const res = await invokeBridge<InputDeviceInfo[]>('get_input_devices');
      if (Array.isArray(res) && res.length > 0) {
        devList = res;
      }
    } catch {
      // Ignore
    }

    // 2. Supplement or fallback with browser navigator.mediaDevices.enumerateDevices
    if (typeof navigator !== 'undefined' && navigator.mediaDevices?.enumerateDevices) {
      try {
        const mediaDevs = await navigator.mediaDevices.enumerateDevices();
        const audioInputs = mediaDevs.filter(
          (d) =>
            d.kind === 'audioinput' &&
            !d.label.toLowerCase().includes('realtime') &&
            !d.label.toLowerCase().includes('clearcore')
        );
        if (audioInputs.length > 0) {
          audioInputs.forEach((d, idx) => {
            const label = d.label || `Microfone ${idx + 1}`;
            // Avoid duplicate by name
            if (!devList.some((existing) => existing.name === label)) {
              devList.push({
                id: d.deviceId || `device-${idx}`,
                name: label,
                is_default: idx === 0 && devList.length === 0,
              });
            }
          });
        }
      } catch {
        // Ignore
      }
    }

    // Filter out any virtual mics that might match
    const filtered = devList.filter(
      (d) =>
        !d.name.toLowerCase().includes('realtime') &&
        !d.name.toLowerCase().includes('clearcore')
    );

    setInputDevices(filtered);

    // Restore selected device from localStorage or pick the default
    const saved = typeof localStorage !== 'undefined' ? localStorage.getItem('clearcore_selected_input_device') : null;
    if (saved && filtered.some((d) => d.id === saved)) {
      setSelectedDeviceId(saved);
    } else if (filtered.length > 0) {
      const defaultDev = filtered.find((d) => d.is_default) || filtered[0];
      setSelectedDeviceId(defaultDev.id);
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem('clearcore_selected_input_device', defaultDev.id);
      }
    }
    setIsLoadingDevices(false);
  };

  const handleDeviceChange = async (e: React.ChangeEvent<HTMLSelectElement>) => {
    const newId = e.target.value;
    setSelectedDeviceId(newId);
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem('clearcore_selected_input_device', newId);
    }
    const dev = inputDevices.find((d) => d.id === newId);
    try {
      await invokeBridge('set_input_device', { deviceId: newId });
      setMicActionMessage(t('inputDevice.deviceSelectedFeedback', { name: dev ? dev.name : newId }));
      setTimeout(() => setMicActionMessage(null), 4000);
    } catch (err) {
      setErrorMessage(t('inputDevice.selectionFailed', { error: String(err) }));
    }
  };

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
      setErrorMessage(t('app.daemonUnreachable', { error: String(err) }));
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
    fetchInputDevices();

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

    let cleanupDevicesListener: (() => void) | undefined;
    if (window.clearcoreApi?.onInputDevicesUpdate) {
      cleanupDevicesListener = window.clearcoreApi.onInputDevicesUpdate((data) => {
        if (data.devices) {
          setInputDevices(data.devices);
        }
        if (data.selectedId) {
          setSelectedDeviceId(data.selectedId);
        }
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
      if (cleanupDevicesListener) cleanupDevicesListener();
    };
  }, []);

  const handleModeChange = async (newMode: DenoiseMode) => {
    try {
      await invokeBridge('set_mode', { mode: newMode });
      setMode(newMode);
    } catch (err) {
      setErrorMessage(t('modes.switchFailed', { error: String(err) }));
    }
  };

  const handleRestartGeneration = async () => {
    try {
      await invokeBridge('restart_generation');
      await fetchStatus();
      await fetchDiagnostics();
    } catch (err) {
      setErrorMessage(t('supervisor.restartFailed', { error: String(err) }));
    }
  };

  const handleRecreateVirtualMic = async () => {
    setIsCheckingMic(true);
    const platLabel = virtualMic?.platform_label || t('virtualMic.systemDefault');
    setMicActionMessage(t('virtualMic.actionVerifying', { platform: platLabel }));
    try {
      const res = await invokeBridge<VirtualMicStatus>('recreate_virtual_mic');
      setVirtualMic(res);
      if (res.present) {
        setMicActionMessage(t('virtualMic.actionSuccess', { id: String(res.node_id ?? t('virtualMic.activeStatus')) }));
      } else {
        setMicActionMessage(t('virtualMic.actionFailed', { error: res.error ?? t('virtualMic.permissionHint') }));
      }
    } catch (err) {
      setMicActionMessage(t('virtualMic.createError', { error: String(err) }));
    } finally {
      setIsCheckingMic(false);
      setTimeout(() => setMicActionMessage(null), 5000);
    }
  };

  const handleSetDefaultVirtualMic = async () => {
    setIsCheckingMic(true);
    setMicActionMessage(t('virtualMic.actionSettingDefault'));
    try {
      const res = await invokeBridge<VirtualMicStatus>('set_default_virtual_mic');
      setVirtualMic(res);
      if (res.is_default) {
        setMicActionMessage(t('virtualMic.actionDefaultSuccess'));
      } else {
        setMicActionMessage(t('virtualMic.actionDefaultSent'));
      }
    } catch (err) {
      setMicActionMessage(t('virtualMic.setDefaultError', { error: String(err) }));
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
      setErrorMessage(t('autostart.changeFailed', { error: String(err) }));
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
          <h1>{t('app.title')}</h1>
          <p style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
            {t('app.subtitle')}
          </p>
        </div>
        <div style={{ display: 'flex', gap: 10, alignItems: 'center' }}>
          <select
            className="lang-select"
            value={locale}
            onChange={(e) => setLocale(e.target.value)}
            title={t('app.language')}
            aria-label={t('app.language')}
          >
            {availableLocales.map((loc) => (
              <option key={loc.code} value={loc.code}>
                🌐 {loc.name}
              </option>
            ))}
          </select>
          <span className={`service-badge ${isConnected ? 'online' : 'offline'}`}>
            ● {isConnected ? t('app.daemonOnline') : t('app.daemonOffline')}
          </span>
          <button
            className="action-btn"
            style={{ fontSize: '0.8rem', padding: '6px 12px' }}
            title={t('app.minimizeToTray')}
            onClick={handleMinimizeToTray}
          >
            {t('app.minimizeToTray')}
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
        <h2 className="card-title">{t('modes.title')}</h2>
        <div className="mode-group">
          <button
            className={`mode-btn ${mode === 'Active' ? 'active-mode' : ''}`}
            onClick={() => handleModeChange('Active')}
          >
            {t('modes.active')}
          </button>
          <button
            className={`mode-btn ${mode === 'Bypass' ? 'bypass-mode' : ''}`}
            onClick={() => handleModeChange('Bypass')}
          >
            {t('modes.bypass')}
          </button>
          <button
            className={`mode-btn ${mode === 'Mute' ? 'mute-mode' : ''}`}
            onClick={() => handleModeChange('Mute')}
          >
            {t('modes.mute')}
          </button>
        </div>
      </div>

      {/* Aceleração de Hardware & Runtimes de IA */}
      <HardwareAcceleratorCard />

      {/* Dispositivo de Entrada de Áudio (Microfone Físico) */}
      <div className="card">

        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12, flexWrap: 'wrap', gap: 10 }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <h2 className="card-title" style={{ margin: 0 }}>{t('inputDevice.title')}</h2>
              <span style={{ fontSize: '0.75rem', background: '#1e293b', border: '1px solid #334155', borderRadius: 4, padding: '2px 8px', color: '#93c5fd' }}>
                {t('inputDevice.badge')}
              </span>
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
              {t('inputDevice.description')}
            </div>
          </div>
          <button
            className="action-btn"
            disabled={isLoadingDevices}
            onClick={fetchInputDevices}
            title={t('inputDevice.refreshTitle')}
            style={{ fontSize: '0.85rem', padding: '6px 12px' }}
          >
            {isLoadingDevices ? t('inputDevice.refreshing') : t('inputDevice.refresh')}
          </button>
        </div>

        <div style={{ marginTop: 8 }}>
          {inputDevices.length > 0 ? (
            <select
              className="device-select"
              value={selectedDeviceId}
              onChange={handleDeviceChange}
              aria-label={t('inputDevice.selectAria')}
            >
              {inputDevices.map((dev) => (
                <option key={dev.id} value={dev.id}>
                  🎙 {dev.name} {dev.is_default ? t('inputDevice.defaultSuffix') : ''}
                </option>
              ))}
            </select>
          ) : (
            <div style={{ padding: '10px 14px', background: 'var(--bg-secondary)', borderRadius: 6, fontSize: '0.85rem', color: 'var(--text-muted)' }}>
              {t('inputDevice.noDevices')}
            </div>
          )}
        </div>

        {selectedDeviceId && (
          <div style={{ marginTop: 8, fontSize: '0.8rem', color: '#4ade80' }}>
            {t('inputDevice.activeDevice', {
              name: inputDevices.find((d) => d.id === selectedDeviceId)?.name || selectedDeviceId,
            })}
          </div>
        )}
      </div>

      {/* Microfone Virtual Multi-Plataforma */}
      <div className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16, flexWrap: 'wrap', gap: 10 }}>
          <div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
              <h2 className="card-title" style={{ margin: 0 }}>{t('virtualMic.title')}</h2>
              <span style={{ fontSize: '0.75rem', background: '#1e293b', border: '1px solid #334155', borderRadius: 4, padding: '2px 8px', color: '#93c5fd' }}>
                {platformTitle}
              </span>
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
              {t('virtualMic.description')}
            </div>
          </div>
          <div style={{ display: 'flex', gap: 8 }}>
            <button
              className="action-btn"
              disabled={isCheckingMic}
              onClick={handleRecreateVirtualMic}
              title={t('virtualMic.verifyTitle')}
            >
              {isCheckingMic ? t('virtualMic.verifying') : t('virtualMic.verifyCreate')}
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
                {virtualMic.is_default ? t('virtualMic.isDefault') : t('virtualMic.makeDefault')}
              </button>
            )}
          </div>
        </div>

        <div className="grid-cols-2">
          <div className="metric-box">
            <div className="metric-label">
              {isWindows ? t('virtualMic.statusWaveRt') : isMac ? t('virtualMic.statusCoreAudio') : t('virtualMic.statusPipeWire')}
            </div>
            <div className="metric-value status-badge" style={{ color: virtualMic?.present ? '#4ade80' : '#f87171' }}>
              {virtualMic?.present
                ? t('virtualMic.statusActive', { id: String(virtualMic.node_id ?? 'OK') })
                : t('virtualMic.statusNotDetected')}
            </div>
          </div>
          <div className="metric-box">
            <div className="metric-label">{t('virtualMic.systemDefaultLabel')}</div>
            <div className="metric-value" style={{ color: virtualMic?.is_default ? '#4ade80' : 'var(--text-muted)' }}>
              {virtualMic?.is_default ? t('virtualMic.systemDefaultYes') : t('virtualMic.systemDefaultNo')}
            </div>
          </div>
        </div>

        {virtualMic?.present ? (
          <div style={{ marginTop: 12, padding: '10px 14px', background: 'var(--bg-secondary)', borderRadius: 6, fontSize: '0.85rem', color: 'var(--text-muted)', display: 'flex', justifyContent: 'space-between', flexWrap: 'wrap', gap: 8 }}>
            <span><strong>{t('virtualMic.deviceLabel')}</strong> {virtualMic.node_name}</span>
            <span><strong>{t('virtualMic.formatLabel')}</strong> {virtualMic.format || 'F32LE'} @ {virtualMic.rate || 48000}Hz</span>
            <span><strong>{t('virtualMic.channelsLabel')}</strong> {virtualMic.channels || 1} ({t('virtualMic.channelMono')})</span>
            <span><strong>{t('virtualMic.latencyLabel')}</strong> {t('virtualMic.latencySamples', { quantum: String(virtualMic.quantum || 480) })}</span>
          </div>
        ) : (
          <div style={{ marginTop: 12, padding: '12px 14px', background: '#451a1a', border: '1px solid #7f1d1d', borderRadius: 6, color: '#fca5a5', fontSize: '0.85rem', lineHeight: 1.5 }}>
            <div style={{ fontWeight: 600, marginBottom: 4 }}>
              {t('virtualMic.notDetectedWarning', { platform: platformTitle })}
            </div>
            <div>
              {isWindows && <span>{t('virtualMic.windowsInstruction')}</span>}
              {isMac && <span>{t('virtualMic.macosInstruction')}</span>}
              {!isWindows && !isMac && <span>{t('virtualMic.linuxInstruction')}</span>}
            </div>
          </div>
        )}
      </div>

      {/* Teste de Gravação e Reprodução de Áudio Filtrado */}
      <AudioTestCard
        virtualMicPresent={Boolean(virtualMic?.present)}
        selectedInputId={selectedDeviceId}
        inputDevices={inputDevices}
      />

      {/* Supervisor Status */}
      <div className="card">
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
          <h2 className="card-title" style={{ margin: 0 }}>{t('supervisor.title')}</h2>
          <button
            className="action-btn"
            disabled={!status?.can_restart}
            onClick={handleRestartGeneration}
          >
            {t('supervisor.restartBtn')}
          </button>
        </div>

        <div className="grid-cols-2">
          <div className="metric-box">
            <div className="metric-label">{t('supervisor.stateLabel')}</div>
            <div className="metric-value status-badge" style={{ color: status?.state === 'Running' ? '#4ade80' : '#f87171' }}>
              {status?.state ?? 'Unknown'}
            </div>
          </div>
          <div className="metric-box">
            <div className="metric-label">{t('supervisor.crashesLabel')}</div>
            <div className="metric-value">
              {status?.crash_count_15m ?? 0} / {status?.total_crashes ?? 0}
            </div>
          </div>
        </div>
      </div>

      {/* Inicialização e Bandeja */}
      <div className="card">
        <h2 className="card-title">{t('autostart.title')}</h2>
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', background: 'var(--bg-secondary)', padding: '12px 16px', borderRadius: 6, border: '1px solid var(--border-color)' }}>
          <div>
            <div style={{ fontWeight: 600, color: 'var(--text-main)', fontSize: '0.95rem' }}>
              {t('autostart.itemTitle')}
            </div>
            <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 2 }}>
              {t('autostart.itemDesc', { platform: platformTitle })}
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
              {autostartEnabled ? t('autostart.enabled') : t('autostart.disabled')}
            </span>
          </label>
        </div>
        <p style={{ color: 'var(--text-muted)', fontSize: '0.8rem', marginTop: 10 }}>
          {t('autostart.tip')}
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
