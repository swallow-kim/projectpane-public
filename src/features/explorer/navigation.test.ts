import { describe, expect, it } from 'vitest';
import { addTab, closeTab, createPane, currentLocation, moveTab, navigateTab, parentPath, rememberPosition, travelTab, updateTab, protectTab, reopenTab } from './navigation';

describe('independent navigation state', () => {
  it('protects closing only and reopens fresh state with view, never selection or old history', () => {
    let pane = createPane('C:\\A');
    const old = pane.tabs[1]!;
    const selected = rememberPosition(navigateTab(old, 'C:\\B'), { ...currentLocation(old).position, selected: new Set(['old-id']) });
    pane = updateTab(pane, { ...selected, view: { ...selected.view, filter: '한글', sort: 'size' } });
    pane = protectTab(pane, old.id);
    expect(closeTab(pane, old.id)).toBe(pane);
    expect(currentLocation(navigateTab(pane.tabs[1]!, 'C:\\C')).path).toBe('C:\\C');
    pane = closeTab(protectTab(pane, old.id), old.id);
    expect(Object.keys(pane.closedTabs![0]!)).toEqual(['path', 'view', 'source']);
    const restored = reopenTab(pane).tabs.at(-1)!;
    expect(restored.id).not.toBe(old.id);
    expect(restored.history).toHaveLength(1);
    expect(currentLocation(restored).path).toBe('C:\\B');
    expect(currentLocation(restored).position.selected.size).toBe(0);
    expect(restored.view).toMatchObject({ filter: '한글', sort: 'size' });
    expect(restored.protected).toBeUndefined();
  });
  it('bounds closed metadata by count and bytes, and retains it when the open tab limit is reached', () => {
    let pane = createPane('C:\\A');
    for (let i = 0; i < 20; i++) { pane = addTab(pane, `C:\\${i}`); pane = closeTab(pane, pane.activeId); }
    expect(pane.closedTabs).toHaveLength(10);
    for (let i = pane.tabs.length; i < 32; i++) pane = addTab(pane);
    expect(reopenTab(pane)).toBe(pane);
    pane = closeTab(pane, pane.activeId);
    pane = addTab(pane, `C:\\${'한'.repeat(30000)}`);
    pane = closeTab(pane, pane.activeId);
    expect(new TextEncoder().encode(JSON.stringify(pane.closedTabs)).length).toBeLessThanOrEqual(64 * 1024);
  });
  it('rejects excess retained selection without silently dropping existing tab state', () => {
    const pane = createPane('C:\\A');
    const tab = pane.tabs.find(value => value.id === pane.activeId);
    if (!tab) throw new Error('fixture tab missing');
    const selected = new Set(Array.from({ length: 8193 }, (_, index) => `${index.toString().padStart(8, '0')}${'a'.repeat(1016)}`));
    const next = updateTab(pane, rememberPosition(tab, { ...currentLocation(tab).position, selected }));
    expect(next.selectionLimited).toBe(true);
    expect(next.tabs).toBe(pane.tabs);
  });
  it('keeps history and selection with their tab while branching truncates forward history', () => {
    const pane = createPane('C:\\A');
    const active = pane.tabs.find(tab => tab.id === pane.activeId);
    if (!active) throw new Error('fixture active tab missing');
    const b = navigateTab(active, 'C:\\B');
    const c = navigateTab(b, 'C:\\C');
    const back = travelTab(c, -1);
    const branch = navigateTab(back, 'C:\\D');
    expect(branch.history.map(location => location.path)).toEqual(['C:\\A', 'C:\\B', 'C:\\D']);
    const nextPane = addTab(updateTab(pane, branch), 'C:\\Other');
    expect(nextPane.tabs.find(tab => tab.id === branch.id)?.history).toEqual(branch.history);
    expect(closeTab(nextPane, nextPane.activeId).tabs.some(tab => tab.id === branch.id)).toBe(true);
  });

  it('reorders without changing active identity and bounds tab and history retention', () => {
    let pane = createPane();
    for (let index = 0; index < 50; index++) pane = addTab(pane, `C:\\${index}`);
    expect(pane.tabs).toHaveLength(32);
    const moved = moveTab(pane, pane.activeId, -1);
    expect(moved.activeId).toBe(pane.activeId);
    let tab = moved.tabs.find(value => value.id === moved.activeId);
    if (!tab) throw new Error('fixture tab missing');
    for (let index = 0; index < 50; index++) tab = navigateTab(tab, `C:\\history-${index}`);
    expect(tab.history).toHaveLength(32);
    const last = closeTab({ activeId: tab.id, tabs: [tab] }, tab.id);
    expect(last.tabs).toHaveLength(1);
    expect(last.tabs[0]?.id).not.toBe(tab.id);
  });

  it('keeps drive and UNC roots intact and handles extended paths without guessing URLs', () => {
    expect(parentPath('C:\\')).toBe(null);
    expect(parentPath('C:\\한글 폴더\\child')).toBe('C:\\한글 폴더');
    expect(parentPath('C:\\child')).toBe('C:\\');
    expect(parentPath('\\\\server\\share')).toBe(null);
    expect(parentPath('\\\\server\\share\\child')).toBe('\\\\server\\share\\');
    expect(parentPath('\\\\?\\C:\\child')).toBe('\\\\?\\C:\\');
    expect(parentPath('\\\\?\\UNC\\server\\share\\child')).toBe('\\\\?\\UNC\\server\\share\\');
    expect(parentPath('https://example.com/path')).toBe(null);
    expect(parentPath('/tmp/child')).toBe('/tmp');
  });
});
