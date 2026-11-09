import { useEffect, useRef, useState } from 'react';
import { desktop } from '../../shared/ipc';
import { toFaultCode, type FaultCode } from '../../shared/fault';
import { addTab, closeTab, createPane, currentLocation, emptyPosition, navigateTab, parentPath, rememberPosition, travelTab, updateTab, type ExplorerTab, type PaneState, type TabView } from './navigation';
import { useListing } from './use-listing';
import { reopenTab, protectTab, initializePaneRoot } from './navigation';

export function usePaneNavigation(pane: number, initialPath: string | undefined, visible: boolean, restored?: PaneState, projectRoot?: string) {
  const [state, setState] = useState(() => restored ?? createPane(initialPath));
  const [refresh, setRefresh] = useState(0);
  const generation = useRef(0);
  const epoch = generation.current;
  const [actionError, setActionError] = useState<FaultCode | null>(null);
  const listing = useListing(pane);
  const tab = state.tabs.find(value => value.id === state.activeId) ?? state.tabs[0];
  if (!tab) throw new Error('Pane must contain a tab');
  const location = currentLocation(tab);
  const current = useRef({ tab, visible });
  current.current = { tab, visible };
  const view = useRef(tab.view);
  view.current = tab.view;
  const path = location.path;
  const id = tab.id;
  const source = tab.source;
  useEffect(() => {
    setActionError(null);
    if (visible && desktop && source === 'real' && path) listing.navigate(path, id, view.current);
    else listing.stop();
    return () => listing.stop();
  }, [listing, visible, source, path, id, refresh]);

  const update = (change: (value: ExplorerTab) => ExplorerTab) => setState(value => {
    const active = value.tabs.find(item => item.id === value.activeId);
    return active ? updateTab(value, change(active)) : value;
  });
  const navigate = (nextPath: string) => {
    if (!desktop || source !== 'real' || !nextPath) return;
    if (nextPath === path) { listing.refresh(path, id, view.current); return; }
    listing.stop();
    update(value => navigateTab(value, nextPath));
  };
  const viewRequest = useRef(0);
  const changeView = (next: TabView) => {
    const request = ++viewRequest.current;
    update(value => ({ ...(value.view.filter !== next.filter ? rememberPosition(value, emptyPosition()) : value), view: next }));
    if (!desktop || source !== 'real' || !visible || !path) return;
    setActionError(null);
    void listing.setView(next).catch((error: unknown) => {
      const code = toFaultCode(error);
      if (code !== 'cancelled' && code !== 'stale_request' && request === viewRequest.current && generation.current === epoch && current.current.tab.id === id && currentLocation(current.current.tab).path === path && current.current.visible) setActionError(code);
    });
  };
  const activate = (entryId: string) => {
    if (!desktop || source !== 'real' || !visible) return;
    setActionError(null);
    void listing.activate(entryId).then(result => {
      if (generation.current !== epoch || current.current.tab.id !== id || currentLocation(current.current.tab).path !== path || !current.current.visible) return;
      if (result.kind === 'folder' && result.path !== null) navigate(result.path);
    }).catch((error: unknown) => {
      const code = toFaultCode(error);
      if (code !== 'cancelled' && code !== 'stale_request' && generation.current === epoch && current.current.tab.id === id && currentLocation(current.current.tab).path === path && current.current.visible) setActionError(code);
    });
  };
  return {
    state, setState, tab, location, listing, actionError, update, navigate, changeView, activate,
    generation: epoch,
    restore: (next: PaneState) => { generation.current += 1; listing.stop(); setActionError(null); setState(next); setRefresh(value => value + 1); },
    add: (path?: string) => setState(value => addTab(value, path ?? projectRoot)),
    initializeRoot: (root: string) => setState(value => initializePaneRoot(value, root)),
    close: (tabId: string) => setState(value => closeTab(value, tabId)),
    reopen: () => setState(reopenTab),
    protect: () => setState(value => protectTab(value, value.activeId)),
    travel: (delta: -1 | 1) => update(value => travelTab(value, delta)),
    up: () => { const parent = parentPath(path); if (parent) navigate(parent); },
    reload: () => { if (desktop && source === 'real' && path && visible) listing.refresh(path, id, view.current); },
  };
}
