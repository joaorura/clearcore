import type { KeyboardEvent, ReactNode } from 'react';

export interface TabDef {
  id: string;
  label: string;
  badge?: string | number;
  disabled?: boolean;
}

export function nextTabId(tabs: TabDef[], currentId: string, key: string): string | null {
  const enabled = tabs.filter((t) => !t.disabled);
  if (enabled.length === 0) return null;
  if (key === 'Home') return enabled[0].id;
  if (key === 'End') return enabled[enabled.length - 1].id;
  if (key !== 'ArrowRight' && key !== 'ArrowLeft') return null;
  const step = key === 'ArrowRight' ? 1 : -1;
  const start = tabs.findIndex((t) => t.id === currentId);
  if (start < 0) return enabled[step > 0 ? 0 : enabled.length - 1].id;
  for (let i = 1; i <= tabs.length; i++) {
    const cand = tabs[(start + step * i + tabs.length * i) % tabs.length];
    if (!cand.disabled) return cand.id;
  }
  return null;
}

export function Tabs(p: {
  tabs: TabDef[];
  activeId: string;
  onChange: (id: string) => void;
  renderPanel: (id: string) => ReactNode;
  ariaLabel: string;
  idPrefix?: string;
}) {
  const prefix = p.idPrefix ?? 'tabs';
  const active =
    p.tabs.find((t) => t.id === p.activeId) ?? p.tabs.find((t) => !t.disabled);
  const activeId = active?.id;

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const next = nextTabId(p.tabs, activeId ?? p.activeId, e.key);
    if (next === null) return;
    e.preventDefault();
    if (next !== activeId) p.onChange(next);
    document.getElementById(`${prefix}-tab-${next}`)?.focus();
  };

  return (
    <>
      <div className="profile-subtabs-nav" role="tablist" aria-label={p.ariaLabel} onKeyDown={onKeyDown}>
        {p.tabs.map((t) => {
          const selected = t.id === activeId;
          return (
            <button
              key={t.id}
              type="button"
              role="tab"
              id={`${prefix}-tab-${t.id}`}
              aria-selected={selected}
              aria-controls={`${prefix}-panel-${t.id}`}
              aria-disabled={t.disabled ? true : undefined}
              disabled={t.disabled}
              tabIndex={selected ? 0 : -1}
              className={`subtab-btn ${selected ? 'subtab-active' : ''}`}
              onClick={() => { if (!t.disabled && !selected) p.onChange(t.id); }}
            >
              {t.label}
              {t.badge !== undefined && <span className="tab-count-badge">{t.badge}</span>}
            </button>
          );
        })}
      </div>
      {activeId !== undefined && (
        <div
          role="tabpanel"
          id={`${prefix}-panel-${activeId}`}
          aria-labelledby={`${prefix}-tab-${activeId}`}
          className="tab-pane-content"
        >
          {p.renderPanel(activeId)}
        </div>
      )}
    </>
  );
}
