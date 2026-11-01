export const MAX_TABS = 32;
const MAX_HISTORY = 32;
const MAX_HISTORY_BYTES = 64 * 1024;
const selectionSizes = new WeakMap<ReadonlySet<string>, number>();

function selectionBytes(selected: ReadonlySet<string>): number {
  const cached = selectionSizes.get(selected);
  if (cached !== undefined) return cached;
  let bytes = 0;
  for (const id of selected) bytes += id.length * 2;
  selectionSizes.set(selected, bytes);
  return bytes;
}

export type ListPosition = {
  readonly selected: ReadonlySet<string>;
  readonly focusedId: string | null;
  readonly focusedIndex: number;
  readonly anchorId: string | null;
  readonly scrollId: string | null;
  readonly scrollIndex: number;
  readonly scrollTop: number;
};
export type PositionUpdate = (current: ListPosition) => ListPosition;
export type Location = { readonly path: string; readonly position: ListPosition };
export type TabView = {
  readonly sort: 'name' | 'type' | 'size' | 'modified';
  readonly direction: 'asc' | 'desc';
  readonly foldersFirst: boolean;
  readonly filter: string;
};
export type ExplorerTab = {
  readonly protected?: boolean;
  readonly id: string;
  readonly source: 'synthetic' | 'real';
  readonly draft: string;
  readonly history: readonly Location[];
  readonly cursor: number;
  readonly view: TabView;
};
export type ClosedTab = { readonly path: string; readonly view: TabView; readonly source: ExplorerTab['source'] };
export type PaneState = { readonly tabs: readonly ExplorerTab[]; readonly activeId: string; readonly selectionLimited?: boolean; readonly closedTabs?: readonly ClosedTab[] };

export function emptyPosition(): ListPosition {
  return { selected: new Set(), focusedId: null, focusedIndex: 0, anchorId: null, scrollId: null, scrollIndex: 0, scrollTop: 0 };
}
export function createTab(path = '', source: ExplorerTab['source'] = 'real'): ExplorerTab {
  return {
    id: crypto.randomUUID(), source, draft: path, history: [{ path, position: emptyPosition() }], cursor: 0,
    view: { sort: 'name', direction: 'asc', foldersFirst: true, filter: '' },
  };
}
export function createPane(path?: string): PaneState {
  const synthetic = createTab('', 'synthetic'), real = createTab(path);
  return { tabs: [synthetic, real], activeId: path ? real.id : synthetic.id };
}
export function initializePaneRoot(pane: PaneState, root: string): PaneState {
  if (!root || pane.tabs.some(tab => tab.source === 'real' && currentLocation(tab).path)) return pane;
  const tabs = pane.tabs.map(tab => tab.source === 'real' ? { ...tab, draft: root, cursor: 0, history: [{ path: root, position: emptyPosition() }] } : tab);
  const activeId = tabs.find(tab => tab.source === 'real')?.id ?? pane.activeId;
  return { ...pane, tabs, activeId };
}
export function currentLocation(tab: ExplorerTab): Location {
  return tab.history[tab.cursor] ?? { path: '', position: emptyPosition() };
}
export function tabLabel(tab: ExplorerTab): string {
  if (tab.source === 'synthetic') return '10만 항목';
  const path = currentLocation(tab).path;
  return path.replace(/[\\/]+$/, '').split(/[\\/]/).at(-1) || path || '실제 폴더';
}
export function updateTab(pane: PaneState, tab: ExplorerTab): PaneState {
  const tabs = pane.tabs.map(value => value.id === tab.id ? tab : value);
  let bytes = 0;
  for (const value of tabs) for (const location of value.history) {
    bytes += selectionBytes(location.position.selected);
    if (bytes > 16 * 1024 * 1024) return { ...pane, selectionLimited: true };
  }
  return { ...pane, tabs, selectionLimited: false };
}
export function addTab(pane: PaneState, path = ''): PaneState {
  if (pane.tabs.length >= MAX_TABS) return pane;
  const tab = createTab(path);
  return { ...pane, tabs: [...pane.tabs, tab], activeId: tab.id };
}
export function closeTab(pane: PaneState, id: string): PaneState {
  const index = pane.tabs.findIndex(tab => tab.id === id);
  if (index < 0 || pane.tabs[index].protected) return pane;
  const closed = pane.tabs[index];
  let closedTabs: readonly ClosedTab[] = [{ path: currentLocation(closed).path, view: { ...closed.view }, source: closed.source }, ...(pane.closedTabs ?? [])].slice(0, 10);
  while (closedTabs.length && new TextEncoder().encode(JSON.stringify(closedTabs)).length > 64 * 1024) closedTabs = closedTabs.slice(0, -1);
  const tabs = pane.tabs.filter(tab => tab.id !== id);
  if (!tabs.length) { const tab = createTab(); return { ...pane, tabs: [tab], activeId: tab.id, closedTabs }; }
  const activeId = pane.activeId === id ? (tabs[Math.min(index, tabs.length - 1)]?.id ?? pane.activeId) : pane.activeId;
  return { ...pane, tabs, activeId, closedTabs };
}
export function reopenTab(pane: PaneState): PaneState {
  const closed = pane.closedTabs?.[0];
  if (!closed || pane.tabs.length >= MAX_TABS) return pane;
  const tab = { ...createTab(closed.path, closed.source), view: { ...closed.view } };
  return { ...pane, tabs: [...pane.tabs, tab], activeId: tab.id, closedTabs: pane.closedTabs!.slice(1) };
}
export function protectTab(pane: PaneState, id: string): PaneState {
  return { ...pane, tabs: pane.tabs.map(tab => tab.id === id ? { ...tab, protected: !tab.protected } : tab) };
}
export function moveTab(pane: PaneState, id: string, direction: -1 | 1): PaneState {
  const index = pane.tabs.findIndex(tab => tab.id === id), target = index + direction;
  const tab = pane.tabs[index];
  if (!tab || target < 0 || target >= pane.tabs.length) return pane;
  const tabs = [...pane.tabs];
  tabs.splice(index, 1);
  tabs.splice(target, 0, tab);
  return { ...pane, tabs };
}
export function rememberPosition(tab: ExplorerTab, position: ListPosition): ExplorerTab {
  return { ...tab, history: tab.history.map((location, index) => index === tab.cursor ? { ...location, position } : location) };
}
export function navigateTab(tab: ExplorerTab, path: string): ExplorerTab {
  if (currentLocation(tab).path === path) return { ...tab, draft: path };
  let history = [...tab.history.slice(0, tab.cursor + 1), { path, position: emptyPosition() }];
  const bytes = (locations: readonly Location[]) => locations.reduce((sum, item) => sum + new TextEncoder().encode(item.path).byteLength, 0);
  while (history.length > 1 && (history.length > MAX_HISTORY || bytes(history) > MAX_HISTORY_BYTES)) history = history.slice(1);
  return { ...tab, draft: path, history, cursor: history.length - 1, view: { ...tab.view, filter: '' } };
}
export function travelTab(tab: ExplorerTab, delta: -1 | 1): ExplorerTab {
  const cursor = tab.cursor + delta, location = tab.history[cursor];
  return location ? { ...tab, cursor, draft: location.path, view: { ...tab.view, filter: '' } } : tab;
}

export function parentPath(path: string): string | null {
  if (/^[a-z]+:\/\//i.test(path)) return null;
  if (path.startsWith('/') && !path.startsWith('//')) {
    const clean = path.replace(/\/+$/, '');
    if (!clean) return null;
    const parent = clean.slice(0, clean.lastIndexOf('/'));
    return parent || '/';
  }
  const normalized = path.replaceAll('/', '\\');
  const match = /^(?:\\\\\?\\)?[a-z]:\\/i.exec(normalized)
    ?? /^\\\\\?\\UNC\\[^\\]+\\[^\\]+(?:\\|$)/i.exec(normalized)
    ?? /^\\\\(?![?.]\\)[^\\]+\\[^\\]+(?:\\|$)/.exec(normalized);
  if (!match) return null;
  const root = match[0].replace(/\\?$/, '\\'), clean = normalized.replace(/\\+$/, '');
  if (clean.length < root.length) return null;
  const parent = clean.slice(0, clean.lastIndexOf('\\'));
  return parent.length < root.length ? root : parent;
}
