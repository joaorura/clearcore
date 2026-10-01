import React, { useState, useEffect } from 'react';
import { useI18n } from './i18n';
import { invokeBridge, HardwareBackendItem, HardwareBackendsResponse } from './bridge';

export const HardwareAcceleratorCard: React.FC = () => {
  const { t } = useI18n();

  const [backends, setBackends] = useState<HardwareBackendItem[]>([]);
  const [activeBackendId, setActiveBackendId] = useState<string>('auto');
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [selectedHelpBackend, setSelectedHelpBackend] = useState<HardwareBackendItem | null>(null);
  const [copied, setCopied] = useState<boolean>(false);
  const [actionFeedback, setActionFeedback] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);

  const fetchBackends = async () => {
    setIsLoading(true);
    try {
      const res = await invokeBridge<HardwareBackendsResponse>('get_hardware_backends');
      if (res && Array.isArray(res.backends)) {
        setBackends(res.backends);
        if (res.active_backend) {
          setActiveBackendId(res.active_backend);
        }
      }
    } catch (err) {
      console.warn('Failed to load hardware backends:', err);
    } finally {
      setIsLoading(false);
    }
  };

  useEffect(() => {
    fetchBackends();
  }, []);

  const handleSelectBackend = async (backend: HardwareBackendItem) => {
    // If hardware is not detected, do not allow selection
    if (!backend.hardware_detected && backend.id !== 'auto') {
      return;
    }

    // If hardware is detected but runtime is missing, show installation modal & warning
    if (!backend.runtime_installed && backend.id !== 'auto') {
      setSelectedHelpBackend(backend);
      setActionError(t('hardwareBackend.cannotSelectMissing'));
      setTimeout(() => setActionError(null), 5000);
      return;
    }

    try {
      const res = await invokeBridge<{ success: boolean; active_backend: string }>(
        'set_hardware_backend',
        { backendId: backend.id }
      );
      if (res && res.active_backend) {
        setActiveBackendId(res.active_backend);
      } else {
        setActiveBackendId(backend.id);
      }
      setActionFeedback(t('hardwareBackend.switchSuccess', { name: backend.name }));
      setTimeout(() => setActionFeedback(null), 4000);
    } catch (err) {
      setActionError(String(err));
      setTimeout(() => setActionError(null), 5000);
    }
  };

  const handleCopyCommand = async (command: string) => {
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
      setCopied(true);
      setTimeout(() => setCopied(false), 2500);
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
          <strong style={{ color: '#4ade80' }}>{activeBackend.name}</strong>
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
                    marginBottom: 12,
                    lineHeight: 1.4,
                  }}
                >
                  {backend.device_info}
                </div>
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
                    ✓ Ativo
                  </span>
                )}
              </div>
            </div>
          );
        })}
      </div>

      {/* Installation Help Modal */}
      {selectedHelpBackend && (
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
              maxWidth: 580,
              width: '100%',
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
                <span style={{ fontSize: '1.4rem' }}>⚠️</span>
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
                background: 'rgba(234, 179, 8, 0.1)',
                border: '1px solid rgba(234, 179, 8, 0.3)',
                borderRadius: 6,
                padding: '12px 16px',
                marginBottom: 16,
                fontSize: '0.9rem',
                lineHeight: 1.5,
              }}
            >
              <div style={{ marginBottom: 6 }}>
                <strong>{t('hardwareBackend.hardwareDetectedTitle')}</strong>
                <div style={{ color: '#fef08a', marginTop: 2 }}>
                  🖥️ {selectedHelpBackend.device_info}
                </div>
              </div>
              <div>
                <strong>{t('hardwareBackend.runtimeRequiredTitle')}</strong>
                <div style={{ color: '#93c5fd', marginTop: 2 }}>
                  📦 {selectedHelpBackend.runtime_name}
                </div>
              </div>
            </div>

            {/* Step-by-Step Instructions */}
            <div style={{ marginBottom: 16 }}>
              <div
                style={{
                  fontWeight: 600,
                  fontSize: '0.9rem',
                  marginBottom: 6,
                  color: 'var(--text-main)',
                }}
              >
                {t('hardwareBackend.installGuideTitle')}
              </div>
              <p
                style={{
                  fontSize: '0.85rem',
                  color: 'var(--text-muted)',
                  lineHeight: 1.5,
                }}
              >
                {selectedHelpBackend.install_instruction}
              </p>
            </div>

            {/* Run Command Section */}
            {selectedHelpBackend.install_command && (
              <div style={{ marginBottom: 20 }}>
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
                      textTransform: 'uppercase',
                    }}
                  >
                    {t('hardwareBackend.commandToRun')}
                  </span>
                  <button
                    className="action-btn"
                    onClick={() => handleCopyCommand(selectedHelpBackend.install_command)}
                    style={{
                      fontSize: '0.75rem',
                      padding: '4px 10px',
                      backgroundColor: copied ? '#15803d' : '#2b3140',
                      color: copied ? '#ffffff' : 'var(--text-main)',
                    }}
                  >
                    {copied ? t('hardwareBackend.copiedBtn') : t('hardwareBackend.copyCommandBtn')}
                  </button>
                </div>
                <pre
                  style={{
                    backgroundColor: '#0f172a',
                    border: '1px solid #334155',
                    borderRadius: 6,
                    padding: '10px 14px',
                    fontSize: '0.85rem',
                    color: '#38bdf8',
                    overflowX: 'auto',
                    fontFamily: 'Consolas, Monaco, "Courier New", monospace',
                  }}
                >
                  {selectedHelpBackend.install_command}
                </pre>
              </div>
            )}

            {/* Modal Footer */}
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: 10 }}>
              <button
                className="action-btn"
                onClick={() => setSelectedHelpBackend(null)}
                style={{ padding: '8px 18px', fontSize: '0.9rem' }}
              >
                {t('hardwareBackend.closeModalBtn')}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
};
