import { z } from 'zod';
import { workspaceSchema, type Workspace } from './projects-schema';
import { ProjectContractError } from './projects-error';

export type RecoveryWarning = 'columns_reset' | 'rows_reset' | 'active_pane_reset' | 'active_tab_reset';
const record = z.record(z.string(), z.unknown());
export function recoverWorkspace(original: unknown): {
  readonly original: unknown; readonly workspace: Workspace; readonly warnings: readonly RecoveryWarning[];
} {
  const object = record.safeParse(original);
  if (!object.success) throw new ProjectContractError('invalid_document');
  const value = { ...object.data };
  const warnings: RecoveryWarning[] = [];
  for (const key of ['columns', 'rows'] as const) {
    const ratio = value[key];
    if (typeof ratio === 'number' && Number.isFinite(ratio) && (ratio < 20 || ratio > 80)) {
      value[key] = 50;
      warnings.push(key === 'columns' ? 'columns_reset' : 'rows_reset');
    }
  }
  const active = value['activePane'], layout = value['layout'];
  if (typeof active === 'number' && Number.isSafeInteger(active) && (layout === 1 || layout === 2 || layout === 4)
    && (active < 0 || active >= layout)) {
    value['activePane'] = 0;
    warnings.push('active_pane_reset');
  }
  if (Array.isArray(value['panes'])) {
    value['panes'] = value['panes'].map((input: unknown) => {
      const pane = record.safeParse(input);
      if (!pane.success) return input;
      const activeId = pane.data['activeTabId'], tabs: unknown = pane.data['tabs'];
      if (typeof activeId !== 'string' || !/^[A-Za-z0-9-]{1,80}$/.test(activeId) || !Array.isArray(tabs)) return input;
      const identities = tabs.map((tab: unknown) => record.safeParse(tab)).map(tab => tab.success ? tab.data['id'] : undefined);
      if (identities.length && !identities.includes(activeId)) {
        warnings.push('active_tab_reset');
        return { ...pane.data, activeTabId: identities[0] };
      }
      return input;
    });
  }
  const parsed = workspaceSchema.safeParse(value);
  if (!parsed.success) throw new ProjectContractError('invalid_document');
  return { original, workspace: parsed.data, warnings };
}
