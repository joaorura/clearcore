import React, { useState, useEffect } from 'react';
import { invokeBridge } from './bridge';

interface UpdateAsset {
  name: string;
  downloadUrl: string;
  size: number;
  format: string;
  installerType: string;
}

interface UpdateInfo {
  success: boolean;
  currentVersion: string;
  latestVersion: string;
  updateAvailable: boolean;
  releaseName: string;
  releaseNotes: string;
  releaseUrl: string;
  distro: string | null;
  asset: UpdateAsset | null;
}

interface DownloadProgress {
  receivedBytes: number;
  totalBytes: number;
  percent: number;
}

export const UpdateBanner: React.FC = () => {
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isUpdating, setIsUpdating] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [dismissed, setDismissed] = useState(false);
  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  useEffect(() => {
    // 1. Initial check via bridge
    invokeBridge<UpdateInfo>('updater:check')
      .then((info) => {
        if (info && info.updateAvailable) {
          setUpdateInfo(info);
        }
      })
      .catch(() => {});

    // 2. Listen to electron push events if in electron
    const electron = (window as unknown as { electronAPI?: { on?: (event: string, cb: (data: unknown) => void) => void } }).electronAPI;
    if (electron && typeof electron.on === 'function') {
      electron.on('updater:available', (data: unknown) => {
        const info = data as UpdateInfo;
        if (info && info.updateAvailable) {
          setUpdateInfo(info);
        }
      });

      electron.on('updater:progress', (data: unknown) => {
        setProgress(data as DownloadProgress);
      });
    }
  }, []);

  if (!updateInfo || !updateInfo.updateAvailable || dismissed) {
    return null;
  }

  const formatBytes = (bytes: number) => {
    if (!bytes) return '0 MB';
    return (bytes / (1024 * 1024)).toFixed(1) + ' MB';
  };

  const handleUpdateClick = async () => {
    if (!updateInfo.asset) {
      if (updateInfo.releaseUrl) {
        window.open(updateInfo.releaseUrl, '_blank');
      }
      return;
    }

    setIsUpdating(true);
    setStatusMessage('Baixando pacote oficial de atualização...');

    try {
      const res = await invokeBridge<{ success: boolean; status?: string; error?: string }>(
        'updater:download-and-install',
        updateInfo.asset as unknown as Record<string, unknown>
      );

      if (res && res.success) {
        setStatusMessage('Instalador iniciado! Conclua a atualização para reiniciar o Clearcore.');
      } else {
        setStatusMessage(`Falha ao iniciar atualização: ${res?.error || 'Erro desconhecido'}`);
        setIsUpdating(false);
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      setStatusMessage(`Erro: ${msg}`);
      setIsUpdating(false);
    }
  };

  const getPackageBadge = () => {
    if (!updateInfo.asset) return null;
    const fmt = updateInfo.asset.format.toUpperCase();
    const color = fmt === 'RPM' ? '#294172' : fmt === 'DEB' ? '#a80030' : '#2b5c8f';
    return (
      <span
        style={{
          backgroundColor: color,
          color: '#fff',
          padding: '2px 8px',
          borderRadius: '4px',
          fontSize: '11px',
          fontWeight: 'bold',
          marginLeft: '6px',
        }}
      >
        {fmt} {updateInfo.distro ? `(${updateInfo.distro})` : ''}
      </span>
    );
  };

  return (
    <div
      style={{
        background: 'linear-gradient(90deg, #1e293b 0%, #0f172a 100%)',
        borderBottom: '2px solid #3b82f6',
        color: '#f8fafc',
        padding: '10px 16px',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        flexWrap: 'wrap',
        gap: '8px',
        fontSize: '13px',
      }}
    >
      <div style={{ display: 'flex', alignItems: 'center', gap: '8px', flex: '1 1 auto' }}>
        <span style={{ fontSize: '18px' }}>🚀</span>
        <div>
          <strong>Nova versão disponível: {updateInfo.latestVersion}</strong>
          {getPackageBadge()}
          <span style={{ opacity: 0.8, marginLeft: '8px', fontSize: '12px' }}>
            (Versão atual: {updateInfo.currentVersion})
          </span>
          {updateInfo.asset && (
            <span style={{ opacity: 0.6, marginLeft: '8px', fontSize: '11px' }}>
              • {formatBytes(updateInfo.asset.size)}
            </span>
          )}
          {statusMessage && (
            <div style={{ color: '#60a5fa', fontSize: '12px', marginTop: '2px' }}>
              {statusMessage} {progress && `(${progress.percent}%)`}
            </div>
          )}
          {progress && isUpdating && (
            <div
              style={{
                width: '200px',
                height: '6px',
                backgroundColor: '#334155',
                borderRadius: '3px',
                marginTop: '4px',
                overflow: 'hidden',
              }}
            >
              <div
                style={{
                  width: `${progress.percent}%`,
                  height: '100%',
                  backgroundColor: '#3b82f6',
                  transition: 'width 0.2s',
                }}
              />
            </div>
          )}
        </div>
      </div>

      <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
        <button
          onClick={handleUpdateClick}
          disabled={isUpdating}
          style={{
            backgroundColor: isUpdating ? '#475569' : '#2563eb',
            color: '#fff',
            border: 'none',
            borderRadius: '6px',
            padding: '6px 14px',
            fontSize: '12px',
            fontWeight: 600,
            cursor: isUpdating ? 'wait' : 'pointer',
            transition: 'background-color 0.2s',
          }}
        >
          {isUpdating
            ? `Baixando... ${progress ? `${progress.percent}%` : ''}`
            : updateInfo.asset
            ? `Instalar Direto (${updateInfo.asset.format.toUpperCase()})`
            : 'Atualizar'}
        </button>

        {updateInfo.releaseUrl && (
          <button
            onClick={() => window.open(updateInfo.releaseUrl, '_blank')}
            style={{
              backgroundColor: 'transparent',
              color: '#94a3b8',
              border: '1px solid #475569',
              borderRadius: '6px',
              padding: '6px 10px',
              fontSize: '12px',
              cursor: 'pointer',
            }}
          >
            Notas
          </button>
        )}

        <button
          onClick={() => setDismissed(true)}
          style={{
            background: 'none',
            border: 'none',
            color: '#64748b',
            cursor: 'pointer',
            fontSize: '16px',
            padding: '4px',
            marginLeft: '4px',
          }}
          title="Dispensar"
        >
          ✕
        </button>
      </div>
    </div>
  );
};
