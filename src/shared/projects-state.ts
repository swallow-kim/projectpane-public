import { currentLocation, emptyPosition, type PaneState, type ExplorerTab } from '../features/explorer/navigation';
import { ROW_HEIGHT } from '../features/explorer/fixed-list';
import { DEFAULT_VIEW } from './listing-view';
import { workspaceSchema, type Workspace } from './projects-schema';
import { normalizeAbsolutePath, toPathRef, resolvePathRef } from './projects-path';
import { ProjectContractError, MAX_PROJECT_BYTES, utf8Bytes } from './projects-error';

export type WorkspaceInput = {
  readonly layout: 1 | 2 | 4; readonly columns: number; readonly rows: number;
  readonly activePane: number; readonly panes: readonly PaneState[];
};
export type SnapshotWarning = 'synthetic_tabs_omitted' | 'empty_pane_replaced' | 'active_tab_reset';
export function snapshotWorkspace(root: string, input: WorkspaceInput): {
  readonly workspace: Workspace; readonly warnings: readonly SnapshotWarning[];
} {
  normalizeAbsolutePath(root);
  const warnings: SnapshotWarning[] = [];
  const ids = new Set(input.panes.flatMap(pane => pane.tabs.map(tab => tab.id)));
  const panes = input.panes.map((pane, index) => {
    const real = pane.tabs.filter(tab => tab.source === 'real');
    if (real.length !== pane.tabs.length) warnings.push('synthetic_tabs_omitted');
    const tabs = real.map(tab => {
      const location = currentLocation(tab);
      return { id: tab.id, path: toPathRef(root, location.path), view: { ...tab.view },
        scroll: { entryId: location.position.scrollId, index: location.position.scrollIndex,
          intraRowOffset: location.position.scrollTop % ROW_HEIGHT } };
    });
    if (!tabs.length) {
      let suffix = 0, id = `restored-${index}-${suffix}`;
      while (ids.has(id)) id = `restored-${index}-${++suffix}`;
      ids.add(id);
      tabs.push({ id, path: { kind: 'empty', value: '' }, view: { ...DEFAULT_VIEW },
        scroll: { entryId: null, index: 0, intraRowOffset: 0 } });
      warnings.push('empty_pane_replaced');
    }
    const activeTabId = tabs.some(tab => tab.id === pane.activeId) ? pane.activeId : tabs[0].id;
    if (activeTabId !== pane.activeId) warnings.push('active_tab_reset');
    return { id: `pane-${index}`, activeTabId, tabs };
  });
  const parsed = workspaceSchema.safeParse({ layout: input.layout, columns: input.columns,
    rows: input.rows, activePane: input.activePane, panes });
  if (!parsed.success) throw new ProjectContractError('invalid_document');
  if (utf8Bytes(JSON.stringify(parsed.data)) > MAX_PROJECT_BYTES) throw new ProjectContractError('payload_limit');
  return { workspace: parsed.data, warnings };
}
export function restoreWorkspace(root: string, workspace: Workspace): WorkspaceInput {
  normalizeAbsolutePath(root);
  const panes = workspace.panes.map(pane => ({
    activeId: pane.activeTabId,
    tabs: pane.tabs.map((tab): ExplorerTab => {
      const path = resolvePathRef(root, tab.path) ?? '';
      return { id: tab.id, source: 'real', cursor: 0, draft: path, view: { ...tab.view },
        history: [{ path, position: { ...emptyPosition(), scrollId: tab.scroll.entryId,
          scrollIndex: tab.scroll.index, scrollTop: tab.scroll.index * ROW_HEIGHT + Math.min(tab.scroll.intraRowOffset, ROW_HEIGHT - 1) } }] };
    }),
  }));
  return { layout: workspace.layout, columns: workspace.columns, rows: workspace.rows,
    activePane: workspace.activePane, panes };
}
