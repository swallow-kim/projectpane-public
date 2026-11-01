import { describe, expect, it } from 'vitest';
import { createPane, currentLocation, navigateTab, rememberPosition } from '../features/explorer/navigation';
import fixture from '../../tests/fixtures/projects/project-v1.json';
import { parseProject, snapshotWorkspace, restoreWorkspace, recoverWorkspace } from './projects';

describe('pure workspace conversion', () => {
  it('drops synthetic tabs and ephemeral state while preserving current location and scroll', () => {
    const pane = createPane('C:\\Example\\before');
    const real = pane.tabs[1];
    const moved = navigateTab(real, 'C:\\Example\\after');
    const tab = { ...rememberPosition(moved, { ...currentLocation(moved).position,
      selected: new Set(['selected']), focusedId: 'selected', scrollId: 'row', scrollIndex: 4, scrollTop: 4 * 26 + 7 }), draft: 'unsaved draft' };
    const input = { layout: 4 as const, columns: 50, rows: 50, activePane: 0,
      panes: [{ tabs: [pane.tabs[0], tab], activeId: tab.id }, createPane(), createPane(), createPane()] };
    const saved = snapshotWorkspace('C:\\Example', input);
    const restored = restoreWorkspace('C:\\Example', saved.workspace);
    expect(saved.workspace.panes[0].tabs).toHaveLength(1);
    expect(saved.workspace.panes[0].tabs[0].path).toEqual({ kind: 'relative', value: 'after' });
    expect(saved.workspace.panes[0].tabs[0].scroll).toEqual({ entryId: 'row', index: 4, intraRowOffset: 7 });
    expect(restored.panes[0].tabs[0]).toMatchObject({ source: 'real', cursor: 0, draft: 'C:\\Example\\after' });
    expect(restored.panes[0].tabs[0].history).toHaveLength(1);
    expect(currentLocation(restored.panes[0].tabs[0]).position.selected.size).toBe(0);
    expect(JSON.stringify(saved)).not.toContain('unsaved draft');
  });
  it('replaces synthetic-only panes with deterministic safe real tabs', () => {
    const panes = [0, 1, 2, 3].map(() => {
      const synthetic = createPane().tabs[0];
      return { tabs: [synthetic], activeId: synthetic.id };
    });
    const input = { layout: 1 as const, columns: 50, rows: 50, activePane: 0, panes };
    const saved = snapshotWorkspace('C:\\Example', input);
    expect(snapshotWorkspace('C:\\Example', input)).toEqual(saved);
    expect(saved.workspace.panes.every(p => p.tabs.length === 1 && p.tabs[0].path.kind === 'empty')).toBe(true);
    expect(saved.warnings).toContain('synthetic_tabs_omitted');
  });
});
describe('explicit recovery', () => {
  it('preserves original and warns for every repair', () => {
    const input = { ...fixture.preset, columns: 200, activePane: 9,
      panes: fixture.preset.panes.map((pane, index) => index === 0 ? { ...pane, activeTabId: 'missing' } : pane) };
    const before = JSON.stringify(input);
    const result = recoverWorkspace(input);
    expect(result.original).toEqual(input);
    expect(JSON.stringify(input)).toBe(before);
    expect(result.workspace).toEqual(parseProject(fixture).preset);
    expect(result.warnings).toEqual(['columns_reset', 'active_pane_reset', 'active_tab_reset']);
  });
  it('refuses ambiguous duplicate IDs', () => {
    const input = { ...fixture.preset, panes: fixture.preset.panes.map(pane => ({ ...pane, id: 'duplicate' })) };
    expect(() => recoverWorkspace(input)).toThrow('invalid_document');
  });
});
