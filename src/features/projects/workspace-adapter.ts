import { snapshotWorkspace, type Workspace, type WorkspaceInput } from '../../shared/projects';
import type { PaneHandle } from '../explorer/ExplorerPane';
export { restoreWorkspace } from '../../shared/projects';

export function captureWorkspace(root: string, layout: Omit<WorkspaceInput, 'panes'>,
  handles: readonly (PaneHandle | null)[]): Workspace | null {
  const panes = handles.map(handle => handle?.snapshot());
  if (panes.length !== 4 || panes.some(pane => !pane)) return null;
  return snapshotWorkspace(root, { ...layout, panes: panes.filter(pane => pane !== undefined) }).workspace;
}
