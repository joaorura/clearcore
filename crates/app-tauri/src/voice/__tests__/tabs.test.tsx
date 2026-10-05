import { describe, it, expect, vi } from 'vitest';
import { renderToStaticMarkup as html } from 'react-dom/server';
import { Tabs, nextTabId, type TabDef } from '../Tabs';

const tabs: TabDef[] = [
  { id: 'a', label: 'Alpha' },
  { id: 'b', label: 'Beta', disabled: true },
  { id: 'c', label: 'Gamma', badge: 3 },
  { id: 'd', label: 'Delta' },
];

describe('nextTabId', () => {
  it('ArrowRight circulates and skips disabled', () => {
    expect(nextTabId(tabs, 'a', 'ArrowRight')).toBe('c');
    expect(nextTabId(tabs, 'c', 'ArrowRight')).toBe('d');
    expect(nextTabId(tabs, 'd', 'ArrowRight')).toBe('a');
  });
  it('ArrowLeft circulates and skips disabled', () => {
    expect(nextTabId(tabs, 'd', 'ArrowLeft')).toBe('c');
    expect(nextTabId(tabs, 'c', 'ArrowLeft')).toBe('a');
    expect(nextTabId(tabs, 'a', 'ArrowLeft')).toBe('d');
  });
  it('Home/End go to first/last enabled', () => {
    expect(nextTabId(tabs, 'c', 'Home')).toBe('a');
    expect(nextTabId(tabs, 'c', 'End')).toBe('d');
    const t2: TabDef[] = [{ id: 'x', label: 'X', disabled: true }, { id: 'y', label: 'Y' }, { id: 'z', label: 'Z', disabled: true }];
    expect(nextTabId(t2, 'y', 'Home')).toBe('y');
    expect(nextTabId(t2, 'y', 'End')).toBe('y');
  });
  it('unknown key returns null', () => {
    expect(nextTabId(tabs, 'a', 'Enter')).toBeNull();
    expect(nextTabId(tabs, 'a', 'ArrowDown')).toBeNull();
  });
  it('single enabled tab stays on itself', () => {
    const one: TabDef[] = [{ id: 'x', label: 'X' }, { id: 'y', label: 'Y', disabled: true }];
    expect(nextTabId(one, 'x', 'ArrowRight')).toBe('x');
    expect(nextTabId(one, 'x', 'ArrowLeft')).toBe('x');
  });
});

describe('Tabs markup', () => {
  const render = (activeId: string, renderPanel = (id: string) => <p>panel-{id}</p>) =>
    html(<Tabs tabs={tabs} activeId={activeId} onChange={() => {}} renderPanel={renderPanel} ariaLabel="Voz" idPrefix="vp" />);

  it('has tablist, tabs and a tabpanel', () => {
    const out = render('c');
    expect(out).toContain('role="tablist"');
    expect(out).toContain('aria-label="Voz"');
    expect((out.match(/role="tab"/g) ?? []).length).toBe(4);
    expect((out.match(/role="tabpanel"/g) ?? []).length).toBe(1);
  });
  it('aria-selected only on active, tabindex 0/-1', () => {
    const out = render('c');
    expect((out.match(/aria-selected="true"/g) ?? []).length).toBe(1);
    expect((out.match(/aria-selected="false"/g) ?? []).length).toBe(3);
    expect((out.match(/tabindex="0"/g) ?? []).length).toBe(1);
    expect((out.match(/tabindex="-1"/g) ?? []).length).toBe(3);
  });
  it('ids match between tab and panel', () => {
    const out = render('c');
    expect(out).toContain('id="vp-tab-c"');
    expect(out).toContain('aria-controls="vp-panel-c"');
    expect(out).toContain('id="vp-panel-c"');
    expect(out).toContain('aria-labelledby="vp-tab-c"');
  });
  it('renders only the active panel', () => {
    const spy = vi.fn((id: string) => <p>panel-{id}</p>);
    const out = render('d', spy);
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith('d');
    expect(out).toContain('<p>panel-d</p>');
    expect(out).not.toContain('<p>panel-a</p>');
  });
  it('renders badge', () => {
    expect(render('a')).toMatch(/Gamma[\s\S]*>3</);
  });
  it('disabled tab has aria-disabled and disabled', () => {
    const out = render('a');
    const btn = out.match(/<button[^>]*id="vp-tab-b"[^>]*>/)![0];
    expect(btn).toContain('aria-disabled="true"');
    expect(btn).toContain('disabled=""');
  });
  it('invalid activeId falls back to first enabled', () => {
    const out = render('nope');
    expect(out).toContain('id="vp-panel-a"');
    expect(out).toContain('<p>panel-a</p>');
    expect((out.match(/aria-selected="true"/g) ?? []).length).toBe(1);
  });
});
