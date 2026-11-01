import { expect, it } from 'vitest';
import { createPane, currentLocation, navigateTab, rememberPosition, type PaneState } from '../explorer/navigation';
import { captureWorkspace, restoreWorkspace } from './workspace-adapter';
import { workspaceSchema } from '../../shared/projects-schema';
const handle = (state: PaneState) => ({ snapshot: () => state, restore: () => {}, focusAddress: () => {}, cancelLoading: () => false, shortcut: () => false });
const geometry = { layout: 4, columns: 50, rows: 50, activePane: 0 } as const;

it('captures committed real tabs through pane handles without drafts, history or selection', () => {
  const initial = createPane('C:\\Root');
  const real = initial.tabs[1];
  const tab = { ...rememberPosition(navigateTab(real, 'C:\\Root\\child'), {
    ...currentLocation(real).position, selected: new Set(['secret-selection']), scrollId: 'entry-4', scrollIndex: 4, scrollTop: 4 * 26 + 7,
  }), draft: 'unfinished input' };
  const workspace = captureWorkspace('C:\\Root', geometry, [handle({ tabs: [initial.tabs[0], tab], activeId: initial.tabs[0].id }), ...[0, 1, 2].map(() => handle(createPane()))]);
  expect(workspace?.panes[0].tabs).toEqual([{ id: tab.id, path: { kind: 'relative', value: 'child' }, view: tab.view,
    scroll: { entryId: 'entry-4', index: 4, intraRowOffset: 7 } }]);
  expect(workspace?.panes[0].activeTabId).toBe(tab.id);
});
it('restores missing paths unchanged through the shared conversion', () => {
  const workspace = captureWorkspace('C:\\Root', geometry, [0, 1, 2, 3].map(() => handle(createPane('C:\\Root\\missing'))));
  if (!workspace) throw new Error('Missing fixture workspace');
  const restored = restoreWorkspace('D:\\Moved', workspace);
  expect(restored.panes[0].tabs[0].draft).toBe('D:\\Moved\\missing');
  expect(restored.panes[0].tabs[0].history).toHaveLength(1);
  expect(currentLocation(restored.panes[0].tabs[0]).position.selected.size).toBe(0);
});
it('does not capture a partially mounted workspace', () => {
  expect(captureWorkspace('C:\\Root', geometry, [handle(createPane()), null, null, null])).toBe(null);
});
it('keeps the same row when an older 30px session has a now-too-large intra-row offset', () => {
  const workspace = captureWorkspace('C:\\Root', geometry, [0, 1, 2, 3].map(() => handle(createPane('C:\\Root'))));
  if (!workspace) throw new Error('Missing fixture workspace');
  const legacy = workspaceSchema.parse({ ...workspace, panes: workspace.panes.map((pane, index) => index === 0
    ? { ...pane, tabs: pane.tabs.map((tab, index) => index === 0 ? { ...tab, scroll: { entryId: 'entry-4', index: 4, intraRowOffset: 29 } } : tab) } : pane) });
  const restored = restoreWorkspace('C:\\Root', legacy);
  const position = currentLocation(restored.panes[0].tabs[0]).position;
  expect(position.scrollIndex).toBe(4);
  expect(position.scrollTop).toBe(4 * 26 + 25);
});
