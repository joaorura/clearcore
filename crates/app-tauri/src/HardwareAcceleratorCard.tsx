import React, { useState, useEffect } from 'react';
import { useI18n } from './i18n';
import { invokeBridge, HardwareBackendItem, HardwareBackendsResponse } from './bridge';
import { logger } from './utils/logger';
import {
  interpretSelectionResult,
  isBackendSelectable,
  isPreviewBackend,
  getBackendHelpDetails,
  getBackendLlmPrompt,
} from './hardwareBackend';

export const HardwareAcceleratorCard: React.FC = () => {
  const { t } = useI18n();

  const [backends, setBackends] = useState<HardwareBackendItem[]>([]);
  const [activeBackendId, setActiveBackendId] = useState<string>('auto');
  const [autoResolvedBackend, setAutoResolvedBackend] = useState<{ id: string; name: string } | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [selectedHelpBackend, setSelectedHelpBackend] = useState<HardwareBackendItem | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [detectionError, setDetectionError] = useState<string | null>(null);
  const [modelCompilationNeeded, setModelCompilationNeeded] = useState<boolean>(false);
  const [isCompiling, setIsCompiling] = useState<boolean>(false);

  const fetchBackends = async () => {
    setIsLoading(true);
    try {
      const res = await invokeBridge<HardwareBackendsResponse>('get_hardware_backends');
      if (res && Array.isArray(res.backends)) {
        setBackends(res.backends);
        if (res.active_backend) {
          setActiveBackendId(res.active_backend);
        }
        if (res.auto_resolved_backend) {
          setAutoResolvedBackend(res.auto_resolved_backend);
        }
        setDetectionError(res.detection_error || null);
        setModelCompilationNeeded(Boolean(res.model_compilation_needed));
      }
    } catch (err) {
      console.warn('Failed to load hardware backends:', err);
    } finally {
      setIsLoading(false);
    }
  };

  const handleCompileTensorRt = async () => {
    setIsCompiling(true);
    setActionFeedback('Compilando modelos neurais para a sua GPU NVIDIA... Isso pode levar de 30 a 60 segundos.');
    try {
      const res = await invokeBridge<{ success: boolean; error?: string }>('compile_tensorrt_models');
      if (res && res.success) {
        setModelCompilationNeeded(false);
        setActionFeedback('✅ Modelos compilados com sucesso para sua GPU NVIDIA! O acelerador TensorRT agora está pronto.');
        await fetchBackends();
      } else {
        setActionError(`Falha na compilação do modelo: ${res?.error || 'Erro desconhecido'}`);
      }
    } catch (err: unknown) {
      setActionError(`Erro ao compilar: ${err instanceof Error ? err.message : String(err)}`);
    } finally {
      setIsCompiling(false);
    }
  };

  useEffect(() => {
    fetchBackends();
  }, []);

  const handleSelectBackend = async (backend: HardwareBackendItem) => {
    logger.info('ACCELERATOR_UI', `Backend card clicked: '${backend.name}' (id: ${backend.id})`, {
      detected: backend.hardware_detected,
      installed: backend.runtime_installed,
    });
    // Se o backend não for selecionável (hardware ausente ou runtime não instalada)
    if (!isBackendSelectable(backend)) {
      if (backend.hardware_detected && !backend.runtime_installed) {
        logger.warn('ACCELERATOR_UI', `Cannot select backend with missing runtime: ${backend.id}`);
        setSelectedHelpBackend(backend);
        setActionError(t('hardwareBackend.cannotSelectMissing'));
        setTimeout(() => setActionError(null), 5000);
      }
      return;
    }

    try {
      const res = await invokeBridge<{ success: boolean; reason?: string; active_backend: string }>(
        'set_hardware_backend',
        { backendId: backend.id }
      );
      const outcome = interpretSelectionResult(backend.id, res);
      if (outcome.kind === 'applied') {
        logger.info('ACCELERATOR_UI', `Backend '${backend.name}' successfully applied (activeId: ${outcome.activeId})`);
        setActiveBackendId(outcome.activeId);
        setActionFeedback(t('hardwareBackend.switchSuccess', { name: backend.name }));
        setTimeout(() => setActionFeedback(null), 4000);
      } else {
        logger.warn('ACCELERATOR_UI', `Backend selection fell back or was rejected: ${outcome.reason}`);
        setActionError(outcome.reason);
        setTimeout(() => setActionError(null), 5000);
      }
    } catch (err) {
      logger.error('ACCELERATOR_UI', `Error applying backend '${backend.id}': ${err}`);
      setActionError(String(err));
      setTimeout(() => setActionError(null), 5000);
    }
  };

  const handleCopyCommand = async (command: string, key?: string) => {
    if (!command) return;
    try {
      if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(command);
      } else {
        const textarea = document.createElement('textarea');
        textarea.value = command;
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
      }
      if (key) setCopiedKey(key);
      setTimeout(() => {
        setCopiedKey(null);
      }, 2500);
    } catch (err) {
      console.error('Failed to copy command:', err);
    }
  };

  const getTierBadge = (tier: string) => {
    switch (tier) {
      case 'Auto':
        return { label: 'Auto', color: '#38bdf8', bg: 'rgba(56, 189, 248, 0.15)' };
      case 'DedicatedGpu':
        return { label: 'GPU Dedicada', color: '#4ade80', bg: 'rgba(74, 222, 128, 0.15)' };
      case 'IntegratedGpu':
        return { label: 'iGPU Integrada', color: '#2dd4bf', bg: 'rgba(45, 212, 191, 0.15)' };
      case 'Npu':
        return { label: 'NPU Neural', color: '#c084fc', bg: 'rgba(192, 132, 252, 0.15)' };
      case 'Cpu':
        return { label: 'CPU Host', color: '#fb923c', bg: 'rgba(251, 146, 60, 0.15)' };
      default:
        return { label: tier, color: '#94a3b8', bg: 'rgba(148, 163, 184, 0.15)' };
    }
  };

  const getBackendIcon = (id: string) => {
    switch (id) {
      case 'auto':
        return '⚡';
      case 'nvidia_tensorrt':
        return '🟢';
      case 'openvino_npu':
        return '🧠';
      case 'openvino_gpu':
        return '🎮';
      case 'openvino_cpu':
        return '⚙️';
      case 'intel_openvino':
        return '🔷';
      case 'amd_ryzenai_npu':
        return '🧠';
      case 'amd_ryzenai_gpu':
        return '🎮';
      case 'amd_ryzenai':
        return '🔶';
      case 'apple_coreml':
        return '🍎';
      case 'cpu_tract':
        return '🔒';
      default:
        return '⚙️';
    }
  };

  const activeBackend = backends.find((b) => b.id === activeBackendId) || {
    id: activeBackendId,
    name: activeBackendId === 'auto' ? 'Automático' : activeBackendId,
  };

  const autoResolvedName =
    autoResolvedBackend?.name ||
    backends.find((b) => b.id === 'auto')?.auto_resolved_name ||
    (backends.find((b) => b.id !== 'auto' && b.hardware_detected && b.runtime_installed)?.name) ||
    'Automático';

  return (
    <div className="card hardware-card">
      <div
        style={{
          display: 'flex',
          justifyContent: 'space-between',
          alignItems: 'center',
          marginBottom: 12,
          flexWrap: 'wrap',
          gap: 10,
        }}
      >
        <div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <h2 className="card-title" style={{ margin: 0 }}>
              {t('hardwareBackend.title')}
            </h2>
            <span
              style={{
                fontSize: '0.75rem',
                background: '#1e293b',
                border: '1px solid #334155',
                borderRadius: 4,
                padding: '2px 8px',
                color: '#93c5fd',
              }}
            >
              {t('hardwareBackend.badge')}
            </span>
          </div>
          <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 4 }}>
            {t('hardwareBackend.description')}
          </div>
        </div>

        <button
          className="action-btn"
          disabled={isLoading}
          onClick={fetchBackends}
          style={{ fontSize: '0.85rem', padding: '6px 12px' }}
        >
          {isLoading
            ? t('hardwareBackend.refreshingBtn')
            : t('hardwareBackend.refreshBtn')}
        </button>
      </div>

      {detectionError && (
        <div
          style={{
            padding: '10px 14px',
            background: '#3b2f10',
            border: '1px solid #854d0e',
            borderRadius: 6,
            marginBottom: 16,
            color: '#fde68a',
            fontSize: '0.9rem',
          }}
        >
          ⚠️ {t('hardwareBackend.detectionError', { error: detectionError })}
        </div>
      )}

      {actionError && (
        <div
          style={{
            padding: '10px 14px',
            background: '#451a1a',
            border: '1px solid #7f1d1d',
            borderRadius: 6,
            marginBottom: 16,
            color: '#fca5a5',
            fontSize: '0.9rem',
          }}
        >
          ⚠️ {actionError}
        </div>
      )}

      {actionFeedback && (
        <div
          style={{
            padding: '10px 14px',
            background: '#142a1f',
            border: '1px solid #166534',
            borderRadius: 6,
            marginBottom: 16,
            color: '#86efac',
            fontSize: '0.9rem',
          }}
        >
          ✓ {actionFeedback}
        </div>
      )}

      {/* Banner de Compilação do Modelo TensorRT para GPU NVIDIA */}
      {modelCompilationNeeded && (
        <div
          style={{
            padding: '14px 16px',
            background: 'linear-gradient(135deg, rgba(30, 58, 138, 0.4) 0%, rgba(15, 23, 42, 0.6) 100%)',
            border: '1px solid #3b82f6',
            borderRadius: 8,
            marginBottom: 16,
            display: 'flex',
            flexDirection: 'column',
            gap: 10,
          }}
        >
          <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
            <span style={{ fontSize: '1.4rem' }}>⚡</span>
            <div>
              <div style={{ fontWeight: 600, color: '#93c5fd', fontSize: '0.95rem' }}>
                GPU NVIDIA Detectada: Compilação de Modelo Necessária
              </div>
              <div style={{ color: 'var(--text-muted)', fontSize: '0.85rem', marginTop: 2 }}>
                Para atingir a máxima performance e menor latência na sua GPU com TensorRT, os modelos neurais precisam ser compilados localmente para a arquitetura do seu hardware.
              </div>
            </div>
          </div>
          <div style={{ display: 'flex', justifyContent: 'flex-end', marginTop: 4 }}>
            <button
              className="action-btn"
              disabled={isCompiling}
              onClick={handleCompileTensorRt}
              style={{
                background: '#2563eb',
                color: '#ffffff',
                border: 'none',
                fontWeight: 600,
                fontSize: '0.85rem',
                padding: '8px 18px',
                borderRadius: 6,
                cursor: isCompiling ? 'not-allowed' : 'pointer',
              }}
            >
              {isCompiling ? '⏳ Compilando modelos para sua GPU (aguarde)...' : '⚙️ Compilar Modelos para esta GPU'}
            </button>
          </div>
        </div>
      )}

      {/* Active Backend Indicator */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          background: 'var(--bg-secondary)',
          padding: '10px 14px',
          borderRadius: 6,
          border: '1px solid var(--border-color)',
          marginBottom: 16,
          fontSize: '0.85rem',
        }}
      >
        <div>
          <span style={{ color: 'var(--text-muted)', marginRight: 6 }}>
            {t('hardwareBackend.activeLabel')}
          </span>
          <strong style={{ color: '#4ade80' }}>
            {activeBackendId === 'auto'
              ? t('hardwareBackend.autoResolvedActive', { name: autoResolvedName })
              : activeBackend.name}
          </strong>
        </div>
        {activeBackendId === 'auto' && (
          <span style={{ color: 'var(--text-muted)', fontSize: '0.8rem' }}>
            ℹ️ {t('hardwareBackend.autoRecommend')}
          </span>
        )}
      </div>

      {/* Backend Selection Grid */}
      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(auto-fit, minmax(250px, 1fr))',
          gap: 12,
        }}
      >
        {backends.map((backend) => {
          const isSelected = activeBackendId === backend.id;
          const tierInfo = getTierBadge(backend.tier);
          const icon = getBackendIcon(backend.id);
          const isMissingRuntime = backend.hardware_detected && !backend.runtime_installed;
          const isNotDetected = !backend.hardware_detected && backend.id !== 'auto';
          const isReady = backend.runtime_installed || backend.id === 'auto';
          const isPreview = isPreviewBackend(backend.id);

          let cardBorder = 'var(--border-color)';
          let cardBg = 'var(--bg-secondary)';

          if (isSelected) {
            cardBorder = '#22c55e';
            cardBg = 'rgba(34, 197, 94, 0.08)';
          } else if (isMissingRuntime) {
            cardBorder = 'rgba(234, 179, 8, 0.5)';
            cardBg = 'rgba(234, 179, 8, 0.04)';
          } else if (isNotDetected) {
            cardBorder = 'rgba(255, 255, 255, 0.05)';
            cardBg = 'rgba(15, 23, 42, 0.4)';
          }

          return (
            <div
              key={backend.id}
              onClick={() => handleSelectBackend(backend)}
              style={{
                border: `1px solid ${cardBorder}`,
                backgroundColor: cardBg,
                borderRadius: 8,
                padding: '14px 16px',
                cursor: isNotDetected ? 'not-allowed' : 'pointer',
                opacity: isNotDetected ? 0.45 : 1,
                display: 'flex',
                flexDirection: 'column',
                justifyContent: 'space-between',
                transition: 'all 0.15s ease-in-out',
                position: 'relative',
              }}
            >
              <div>
                {/* Header: Icon, Name and Tier */}
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'flex-start',
                    justifyContent: 'space-between',
                    gap: 8,
                    marginBottom: 8,
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <span style={{ fontSize: '1.25rem' }}>{icon}</span>
                    <strong
                      style={{
                        fontSize: '0.95rem',
                        color: isSelected ? '#4ade80' : 'var(--text-main)',
                      }}
                    >
                      {backend.name}
                    </strong>
                  </div>
                  <span
                    style={{
                      fontSize: '0.7rem',
                      fontWeight: 600,
                      color: tierInfo.color,
                      backgroundColor: tierInfo.bg,
                      padding: '2px 6px',
                      borderRadius: 4,
                      whiteSpace: 'nowrap',
                    }}
                  >
                    {tierInfo.label}
                  </span>
                </div>

                {/* Device Info */}
                <div
                  style={{
                    fontSize: '0.8rem',
                    color: 'var(--text-muted)',
                    marginBottom: 10,
                    lineHeight: 1.4,
                  }}
                >
                  {backend.device_info}
                </div>

                {/* Se for opcao AUTO, mostrar explicitamente qual mecanismo foi selecionado */}
                {backend.id === 'auto' && (
                  <div
                    style={{
                      background: 'rgba(56, 189, 248, 0.12)',
                      border: '1px solid rgba(56, 189, 248, 0.35)',
                      borderRadius: 6,
                      padding: '8px 10px',
                      marginBottom: 12,
                      fontSize: '0.82rem',
                      color: '#7dd3fc',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 6,
                      fontWeight: 600,
                    }}
                  >
                    <span>⚡</span>
                    <span>
                      {t('hardwareBackend.autoResolvedCurrent', {
                        name: autoResolvedName,
                      })}
                    </span>
                  </div>
                )}
              </div>

              {/* Status and Action Badge */}
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'space-between',
                  paddingTop: 8,
                  borderTop: '1px solid rgba(255, 255, 255, 0.06)',
                  flexWrap: 'wrap',
                  gap: 6,
                }}
              >
                {/* Runtime Status */}
                {isReady && (
                  <span
                    style={{
                      fontSize: '0.75rem',
                      color: '#4ade80',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                      fontWeight: 500,
                    }}
                  >
                    ● {t('hardwareBackend.runtimeReadyBadge')}
                  </span>
                )}

                {isMissingRuntime && (
                  <span
                    style={{
                      fontSize: '0.75rem',
                      color: '#fbbf24',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                      fontWeight: 600,
                    }}
                  >
                    ⚠️ {t('hardwareBackend.runtimeMissingBadge')}
                  </span>
                )}

                {isPreview && !isReady && !isMissingRuntime && (
                  <span
                    style={{
                      fontSize: '0.72rem',
                      color: '#fbbf24',
                      backgroundColor: 'rgba(234, 179, 8, 0.12)',
                      border: '1px solid rgba(234, 179, 8, 0.4)',
                      padding: '2px 6px',
                      borderRadius: 4,
                      fontWeight: 600,
                    }}
                  >
                    {t('hardwareBackend.previewBadge')}
                  </span>
                )}

                {isNotDetected && (
                  <span
                    style={{
                      fontSize: '0.75rem',
                      color: '#94a3b8',
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                    }}
                  >
                    ○ {t('hardwareBackend.notDetectedBadge')}
                  </span>
                )}

                {/* Interactive button or action badge */}
                {isMissingRuntime && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setSelectedHelpBackend(backend);
                    }}
                    style={{
                      background: 'rgba(234, 179, 8, 0.2)',
                      border: '1px solid rgba(234, 179, 8, 0.5)',
                      borderRadius: 4,
                      color: '#fef08a',
                      fontSize: '0.75rem',
                      padding: '3px 8px',
                      cursor: 'pointer',
                      fontWeight: 600,
                      display: 'flex',
                      alignItems: 'center',
                      gap: 4,
                    }}
                    title={t('hardwareBackend.clickForInstallHelp')}
                  >
                    💡 Instruções
                  </button>
                )}

                {isSelected && (
                  <span
                    style={{
                      fontSize: '0.75rem',
                      color: '#22c55e',
                      fontWeight: 700,
                      backgroundColor: 'rgba(34, 197, 94, 0.15)',
                      padding: '2px 6px',
                      borderRadius: 4,
                    }}
                  >
                    ✓ {t('hardwareBackend.statusActiveBadge')}
                  </span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Installation Help Modal */}
      {selectedHelpBackend && (() => {
        const helpDetails = getBackendHelpDetails(selectedHelpBackend.id);
        const userOsLabel =
          typeof navigator !== 'undefined' && /windows/i.test(navigator.userAgent)
            ? 'Windows'
            : typeof navigator !== 'undefined' && /mac/i.test(navigator.userAgent)
              ? 'macOS'
              : 'Linux';
        const llmPrompt =
          getBackendLlmPrompt(selectedHelpBackend.id, {
            os: userOsLabel,
            deviceInfo: selectedHelpBackend.device_info,
          }) || helpDetails?.llmPrompt;

        return (
          <div
            className="modal-overlay"
            onClick={() => setSelectedHelpBackend(null)}
            style={{
              position: 'fixed',
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              backgroundColor: 'rgba(0, 0, 0, 0.75)',
              backdropFilter: 'blur(4px)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              zIndex: 9999,
              padding: 16,
            }}
          >
            <div
              className="modal-content"
              onClick={(e) => e.stopPropagation()}
              style={{
                backgroundColor: '#1e2330',
                border: '1px solid #334155',
                borderRadius: 10,
                maxWidth: 680,
                width: '100%',
                maxHeight: '90vh',
                overflowY: 'auto',
                padding: 24,
                boxShadow: '0 20px 25px -5px rgba(0, 0, 0, 0.5), 0 8px 10px -6px rgba(0, 0, 0, 0.5)',
              }}
            >
              {/* Modal Header */}
              <div
                style={{
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  marginBottom: 16,
                  borderBottom: '1px solid var(--border-color)',
                  paddingBottom: 12,
                }}
              >
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <span style={{ fontSize: '1.4rem' }}>{getBackendIcon(selectedHelpBackend.id)}</span>
                  <h3 style={{ margin: 0, fontSize: '1.15rem', color: '#fef08a' }}>
                    {t('hardwareBackend.modalTitle')}
                  </h3>
                </div>
                <button
                  onClick={() => setSelectedHelpBackend(null)}
                  style={{
                    background: 'none',
                    border: 'none',
                    color: 'var(--text-muted)',
                    fontSize: '1.25rem',
                    cursor: 'pointer',
                    padding: 4,
                  }}
                >
                  ✕
                </button>
              </div>

              {/* Hardware & Runtime Summary */}
              <div
                style={{
                  background: 'rgba(234, 179, 8, 0.08)',
                  border: '1px solid rgba(234, 179, 8, 0.25)',
                  borderRadius: 6,
                  padding: '12px 16px',
                  marginBottom: 16,
                  fontSize: '0.88rem',
                  lineHeight: 1.5,
                }}
              >
                <div style={{ marginBottom: 6 }}>
                  <strong>{t('hardwareBackend.hardwareDetectedTitle')}</strong>
                  <div style={{ color: '#fef08a', marginTop: 2 }}>
                    🖥️ {selectedHelpBackend.device_info}
                  </div>
                </div>
                <div style={{ marginBottom: 6 }}>
                  <strong>{t('hardwareBackend.runtimeRequiredTitle')}</strong>
                  <div style={{ color: '#93c5fd', marginTop: 2 }}>
                    📦 {selectedHelpBackend.runtime_name}
                  </div>
                </div>
                <div>
                  <strong>{t('hardwareBackend.librarySearchedTitle')}</strong>
                  <div style={{ color: '#86efac', marginTop: 2, fontFamily: 'Consolas, monospace', fontSize: '0.82rem' }}>
                    🔍 {helpDetails
                      ? `${helpDetails.librarySearched.linux} (Linux) / ${helpDetails.librarySearched.windows} (Windows)`
                      : selectedHelpBackend.runtime_name}
                  </div>
                  {helpDetails?.librarySearched.description && (
                    <div style={{ color: 'var(--text-muted)', fontSize: '0.78rem', marginTop: 2 }}>
                      {helpDetails.librarySearched.description}
                    </div>
                  )}
                </div>
              </div>

              {/* Proven Method / Recommended Alert Card */}
              {(helpDetails?.provenMethodNote || selectedHelpBackend.id === 'nvidia_tensorrt') && (
                <div
                  style={{
                    background: 'linear-gradient(135deg, rgba(234, 179, 8, 0.14) 0%, rgba(34, 197, 94, 0.1) 100%)',
                    border: '1px solid rgba(234, 179, 8, 0.5)',
                    borderRadius: 8,
                    padding: '14px 16px',
                    marginBottom: 16,
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'flex-start', gap: 12 }}>
                    <div style={{ fontSize: '1.25rem', lineHeight: 1.2 }}>💡</div>
                    <div style={{ flex: 1 }}>
                      <strong style={{ color: '#fef08a', fontSize: '0.92rem', display: 'block', marginBottom: 4 }}>
                        {t('hardwareBackend.provenMethodTitle')}
                      </strong>
                      <div style={{ fontSize: '0.84rem', color: '#f1f5f9', lineHeight: 1.5 }}>
                        {helpDetails?.provenMethodNote || t('hardwareBackend.provenMethodDesc')}
                      </div>
                    </div>
                  </div>
                </div>
              )}

              {/* Official Download Link */}
              {helpDetails?.officialDownload && (
                <div
                  style={{
                    background: 'linear-gradient(135deg, rgba(16, 185, 129, 0.14) 0%, rgba(6, 182, 212, 0.09) 100%)',
                    border: '1px solid rgba(16, 185, 129, 0.5)',
                    borderRadius: 8,
                    padding: '14px 16px',
                    marginBottom: 16,
                  }}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 12 }}>
                    <div style={{ flex: '1 1 300px' }}>
                      <strong style={{ color: '#34d399', fontSize: '0.92rem', display: 'flex', alignItems: 'center', gap: 6 }}>
                        📥 {t('hardwareBackend.officialDownloadTitle')}
                      </strong>
                      <div style={{ fontSize: '0.82rem', color: '#cbd5e1', marginTop: 3 }}>
                        {t('hardwareBackend.officialDownloadDesc')}
                      </div>
                      <div style={{ marginTop: 6, fontSize: '0.78rem', color: '#6ee7b7', fontFamily: 'Consolas, Monaco, monospace', wordBreak: 'break-all' }}>
                        {helpDetails.officialDownload.url}
                      </div>
                    </div>
                    <a
                      href={helpDetails.officialDownload.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 6,
                        color: '#022c22',
                        backgroundColor: '#34d399',
                        textDecoration: 'none',
                        fontWeight: 700,
                        fontSize: '0.82rem',
                        padding: '8px 16px',
                        borderRadius: 6,
                        boxShadow: '0 2px 8px rgba(16, 185, 129, 0.3)',
                      }}
                    >
                      {t('hardwareBackend.openDownloadLink')} ↗
                    </a>
                  </div>
                </div>
              )}

              {/* Official Documentation Link */}
              {helpDetails?.officialDocs && (
                <div
                  style={{
                    background: 'linear-gradient(135deg, rgba(56, 189, 248, 0.12) 0%, rgba(34, 197, 94, 0.08) 100%)',
                    border: '1px solid rgba(56, 189, 248, 0.45)',
                    borderRadius: 8,
                    padding: '14px 16px',
                    marginBottom: 16,
                  }}
                >
                  <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 12 }}>
                    <div style={{ flex: '1 1 300px' }}>
                      <strong style={{ color: '#38bdf8', fontSize: '0.92rem', display: 'flex', alignItems: 'center', gap: 6 }}>
                        {t('hardwareBackend.officialDocsTitle')}
                      </strong>
                      <div style={{ fontSize: '0.82rem', color: '#cbd5e1', marginTop: 3 }}>
                        {t('hardwareBackend.officialDocsDesc')}
                      </div>
                      <div style={{ marginTop: 6, fontSize: '0.78rem', color: '#7dd3fc', fontFamily: 'Consolas, Monaco, monospace', wordBreak: 'break-all' }}>
                        {helpDetails.officialDocs.url}
                      </div>
                    </div>
                    <a
                      href={helpDetails.officialDocs.url}
                      target="_blank"
                      rel="noopener noreferrer"
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 6,
                        color: '#0f172a',
                        backgroundColor: '#38bdf8',
                        textDecoration: 'none',
                        fontWeight: 700,
                        fontSize: '0.82rem',
                        padding: '8px 16px',
                        borderRadius: 6,
                        boxShadow: '0 2px 8px rgba(56, 189, 248, 0.3)',
                      }}
                    >
                      {t('hardwareBackend.openDocLink')} ↗
                    </a>
                  </div>
                </div>
              )}

              {/* LLM Assistance Prompt */}
              {llmPrompt && (
                <div
                  style={{
                    background: 'rgba(99, 102, 241, 0.08)',
                    border: '1px solid rgba(129, 140, 248, 0.35)',
                    borderRadius: 8,
                    padding: '14px 16px',
                    marginBottom: 16,
                  }}
                >
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'flex-start',
                      marginBottom: 10,
                      gap: 12,
                      flexWrap: 'wrap',
                    }}
                  >
                    <div>
                      <strong
                        style={{
                          color: '#e0e7ff',
                          fontSize: '0.88rem',
                          display: 'flex',
                          alignItems: 'center',
                          gap: 6,
                          lineHeight: 1.4,
                        }}
                      >
                        {t('hardwareBackend.llmPromptTitle')}
                      </strong>
                      <div style={{ fontSize: '0.78rem', color: '#a5b4fc', marginTop: 3 }}>
                        {t('hardwareBackend.llmPromptDesc')}
                      </div>
                    </div>
                    <button
                      className="action-btn"
                      onClick={() => handleCopyCommand(llmPrompt, 'llm-prompt')}
                      style={{
                        fontSize: '0.78rem',
                        padding: '6px 14px',
                        backgroundColor: copiedKey === 'llm-prompt' ? '#15803d' : '#4f46e5',
                        color: '#ffffff',
                        border: 'none',
                        borderRadius: 6,
                        fontWeight: 600,
                        cursor: 'pointer',
                        display: 'inline-flex',
                        alignItems: 'center',
                        gap: 6,
                        transition: 'background-color 0.2s',
                        whiteSpace: 'nowrap',
                      }}
                    >
                      {copiedKey === 'llm-prompt'
                        ? t('hardwareBackend.llmPromptCopiedBtn')
                        : t('hardwareBackend.copyLlmPromptBtn')}
                    </button>
                  </div>
                  <pre
                    style={{
                      backgroundColor: '#0f172a',
                      border: '1px solid #334155',
                      borderRadius: 6,
                      padding: '10px 14px',
                      fontSize: '0.8rem',
                      color: '#cbd5e1',
                      overflowX: 'auto',
                      fontFamily: 'Consolas, Monaco, "Courier New", monospace',
                      margin: 0,
                      whiteSpace: 'pre-wrap',
                      wordBreak: 'break-word',
                      lineHeight: 1.5,
                    }}
                  >
                    {llmPrompt}
                  </pre>
                </div>
              )}



              {/* Fallback / Detector Suggested Command if available */}
              {selectedHelpBackend.install_command && (
                <div style={{ marginBottom: 16 }}>
                  <div
                    style={{
                      display: 'flex',
                      justifyContent: 'space-between',
                      alignItems: 'center',
                      marginBottom: 6,
                    }}
                  >
                    <span
                      style={{
                        fontSize: '0.8rem',
                        fontWeight: 600,
                        color: 'var(--text-muted)',
                      }}
                    >
                      ⚡ {t('hardwareBackend.commandToRun')}
                    </span>
                    <button
                      className="action-btn"
                      onClick={() => handleCopyCommand(selectedHelpBackend.install_command, 'detected-cmd')}
                      style={{
                        fontSize: '0.75rem',
                        padding: '3px 10px',
                        backgroundColor: copiedKey === 'detected-cmd' ? '#15803d' : '#2b3140',
                        color: copiedKey === 'detected-cmd' ? '#ffffff' : 'var(--text-main)',
                      }}
                    >
                      {copiedKey === 'detected-cmd' ? t('hardwareBackend.copiedBtn') : t('hardwareBackend.copyCommandBtn')}
                    </button>
                  </div>
                  <pre
                    style={{
                      backgroundColor: '#0f172a',
                      border: '1px solid #334155',
                      borderRadius: 6,
                      padding: '10px 14px',
                      fontSize: '0.82rem',
                      color: '#4ade80',
                      overflowX: 'auto',
                      fontFamily: 'Consolas, Monaco, "Courier New", monospace',
                      margin: 0,
                      whiteSpace: 'pre-wrap',
                      wordBreak: 'break-all',
                    }}
                  >
                    {selectedHelpBackend.install_command}
                  </pre>
                </div>
              )}

              {/* Diagnostic Guide */}
              {helpDetails?.diagnosticGuide && helpDetails.diagnosticGuide.length > 0 && (
                <div
                  style={{
                    background: 'rgba(15, 23, 42, 0.6)',
                    border: '1px solid #334155',
                    borderRadius: 6,
                    padding: '12px 16px',
                    marginBottom: 20,
                  }}
                >
                  <div style={{ fontWeight: 600, fontSize: '0.88rem', color: '#f8fafc', marginBottom: 4 }}>
                    🔍 {t('hardwareBackend.diagnosticGuideTitle')}
                  </div>
                  <div style={{ fontSize: '0.78rem', color: 'var(--text-muted)', marginBottom: 10 }}>
                    {t('hardwareBackend.diagnosticGuideDesc')}
                  </div>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
                    {helpDetails.diagnosticGuide.map((step, idx) => (
                      <div
                        key={idx}
                        style={{
                          background: '#1e293b',
                          border: '1px solid #334155',
                          borderRadius: 6,
                          padding: '8px 12px',
                          fontSize: '0.8rem',
                        }}
                      >
                        <div style={{ fontWeight: 600, color: '#e2e8f0', marginBottom: step.command ? 4 : 0 }}>
                          {idx + 1}. {step.title}
                        </div>
                        {step.command && (
                          <div
                            style={{
                              display: 'flex',
                              alignItems: 'center',
                              justifyContent: 'space-between',
                              background: '#0f172a',
                              padding: '5px 10px',
                              borderRadius: 4,
                              gap: 8,
                            }}
                          >
                            <code style={{ color: '#38bdf8', fontSize: '0.78rem', wordBreak: 'break-all' }}>
                              {step.command}
                            </code>
                            <button
                              onClick={() => handleCopyCommand(step.command!, `diag-${idx}`)}
                              style={{
                                background: copiedKey === `diag-${idx}` ? '#15803d' : '#334155',
                                border: 'none',
                                color: '#ffffff',
                                borderRadius: 4,
                                padding: '2px 8px',
                                fontSize: '0.72rem',
                                cursor: 'pointer',
                                whiteSpace: 'nowrap',
                              }}
                            >
                              {copiedKey === `diag-${idx}` ? t('hardwareBackend.copiedBtn') : t('hardwareBackend.copyCommandShort')}
                            </button>
                          </div>
                        )}
                        {step.tip && (
                          <div style={{ color: '#94a3b8', fontSize: '0.74rem', marginTop: 4 }}>
                            💡 {step.tip}
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </div>
              )}

              {/* Modal Footer */}
              <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 10, borderTop: '1px solid var(--border-color)', paddingTop: 14 }}>
                <button
                  className="action-btn"
                  disabled={isLoading}
                  onClick={async () => {
                    await fetchBackends();
                  }}
                  style={{
                    padding: '8px 16px',
                    fontSize: '0.85rem',
                    background: 'rgba(56, 189, 248, 0.15)',
                    border: '1px solid #38bdf8',
                    color: '#38bdf8',
                  }}
                >
                  {isLoading ? t('hardwareBackend.refreshingBtn') : t('hardwareBackend.recheckBtn')}
                </button>
                <button
                  className="action-btn"
                  onClick={() => setSelectedHelpBackend(null)}
                  style={{ padding: '8px 18px', fontSize: '0.85rem' }}
                >
                  {t('hardwareBackend.closeModalBtn')}
                </button>
              </div>
            </div>
          </div>
        );
      })()}
    </div>
  );
};
