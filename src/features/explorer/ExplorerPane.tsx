import { useCallback, useEffect, useLayoutEffect, useImperativeHandle, useRef, useState, type Ref } from 'react';
import { desktop } from '../../shared/ipc';
import { faultMessage } from '../../shared/fault';
import { FileList } from './FileList';
import { ListingFeedback, listingStatus } from './ListingFeedback';
import { currentLocation, moveTab, parentPath, rememberPosition, updateTab, type PaneState } from './navigation';
import type { PaneShortcut } from './shortcuts';
import { TabBar } from './TabBar';
import { ViewToolbar, type ViewToolbarHandle } from './ViewToolbar';
import { FavoriteButtons } from '../favorites/FavoriteButtons';
import { ArrowLeft, ArrowRight, ArrowUp, RotateCw, CornerDownLeft } from 'lucide-react';
import { usePaneNavigation } from './use-pane-navigation';
import { PaneFileActions } from '../operations/PaneFileActions';
import type { FileActionDraft, FileActionKind } from '../../shared/operations';
import type { FileTransfer } from '../operations/use-file-transfer';
import { watchablePath, type WatchTarget } from './watch-refresh';

export type PaneHandle = {
  readonly initializeRoot?: (root: string) => void;
  readonly selectTab?: (id: string) => void;
  readonly openFolder?: (path: string) => void;
  readonly protectTab?: () => void;
  readonly transferTarget?: () => string | null;
  readonly watchTarget?: () => WatchTarget | null;
  readonly watchStatus?: (key: string, unavailable: boolean) => void;
  readonly refreshFiles?: () => void;
  readonly snapshot: () => PaneState;
  readonly restore: (state: PaneState) => void;
  readonly focusAddress: () => void;
  readonly cancelLoading: () => boolean;
  readonly shortcut: (shortcut: PaneShortcut) => boolean;
};

export function ExplorerPane({ pane, initialPath, projectRoot, visible, ref, activate, restored, onWorkspaceChange, fileActionsAvailable = false, onFileAction, transfer }: {
  readonly transfer?: FileTransfer;
  readonly fileActionsAvailable?: boolean;
  readonly onFileAction?: (draft: FileActionDraft) => void;
  readonly restored?: PaneState;
  readonly onWorkspaceChange?: () => void;
  readonly pane: number;
  readonly initialPath: string | undefined;
  readonly projectRoot?: string;
  readonly visible: boolean;
  readonly ref: Ref<PaneHandle>;
  readonly activate: () => void;
}) {
  const nav = usePaneNavigation(pane, initialPath, visible, restored, projectRoot);
  useEffect(() => { onWorkspaceChange?.(); }, [nav.state, onWorkspaceChange]);
  const { tab, location, listing } = nav;
  const watchKey = JSON.stringify([nav.generation, tab.id, location.path]);
  const [unavailableWatch, setUnavailableWatch] = useState<string | null>(null);
  const [composing, setComposing] = useState(false);
  const [selectionPending, setSelectionPending] = useState(false);
  const [tabNotice, setTabNotice] = useState<string | null>(null);
  useEffect(() => { setTabNotice(null); }, [tab.id, nav.generation]);
  const selectionPendingRef = useRef(false);
  const pendingRefresh = useRef<string | null>(null);
  const changeSelectionPending = useCallback((pending: boolean) => {
    selectionPendingRef.current = pending;
    setSelectionPending(pending);
  }, []);
  useEffect(() => {
    if (pendingRefresh.current !== watchKey || !visible) { pendingRefresh.current = null; return; }
    if (selectionPending) return;
    // Automatic revalidation must not cancel a keyboard selection's page read.
    // Resume after its position update commits, and never on a different folder.
    pendingRefresh.current = null;
    nav.reload();
  }, [selectionPending, watchKey, visible]);
  const composition = useRef(false);
  const address = useRef<HTMLInputElement>(null);
  const viewToolbar = useRef<ViewToolbarHandle>(null);
  const content = useRef<HTMLDivElement>(null);
  const [tabFocusRequest, setTabFocusRequest] = useState(0);
  const fulfilledFocusRequest = useRef(0);
  useLayoutEffect(() => {
    if (!visible || tabFocusRequest === fulfilledFocusRequest.current) return;
    fulfilledFocusRequest.current = tabFocusRequest;
    address.current?.focus(); address.current?.select();
  }, [visible, tabFocusRequest]);
  useEffect(() => {
    const reset = () => { composition.current = false; setComposing(false); };
    window.addEventListener('blur', reset);
    return () => window.removeEventListener('blur', reset);
  }, []);
  const focusAddress = () => { address.current?.focus(); address.current?.select(); };
  const cancelLoading = () => {
    if (tab.source !== 'real' || listing.state.status !== 'loading') return false;
    listing.stop(); return true;
  };
  useImperativeHandle(ref, () => ({
    initializeRoot: nav.initializeRoot,
    selectTab: id => { nav.setState(value => value.tabs.some(item => item.id === id) ? { ...value, activeId: id } : value); setTabFocusRequest(value => value + 1); },
    openFolder: path => {
      if (tab.source === 'synthetic') {
        if (nav.state.tabs.length >= 32) { setTabNotice('탭이 32개입니다. 실제 폴더 탭에서 열거나 탭을 닫으세요.'); return; }
        nav.add(path);
      } else nav.navigate(path);
      requestAnimationFrame(focusAddress);
    },
    protectTab: nav.protect,
    transferTarget: () => visible && filesReady && !composition.current ? location.path : null,
    watchTarget: () => visible && tab.source === 'real' && watchablePath(location.path)
      ? { key: watchKey, path: location.path,
        ready: !composition.current && !selectionPending && !listing.state.viewPending && ['complete', 'capped'].includes(listing.state.status) }
      : null,
    watchStatus: (key, unavailable) => { if (key === watchKey) setUnavailableWatch(unavailable ? key : null); },
    focusAddress, cancelLoading, snapshot: () => nav.state, restore: nav.restore,
    refreshFiles: () => {
      if (selectionPendingRef.current) { pendingRefresh.current = watchKey; return; }
      nav.reload();
    },
    shortcut(shortcut) {
      if (!visible || composition.current) return false;
      switch (shortcut) {
        case 'cancel': return cancelLoading();
        case 'new-tab': nav.add(); requestAnimationFrame(focusAddress); break;
        case 'close-tab':
          if (tab.protected) { setTabNotice('닫기 보호된 탭입니다. 자물쇠 버튼으로 보호를 해제하세요.'); break; }
          nav.close(tab.id); requestAnimationFrame(focusAddress); break;
        case 'reopen-tab': nav.reopen(); requestAnimationFrame(focusAddress); break;
        case 'palette': return false;
        case 'back': nav.travel(-1); break;
        case 'forward': nav.travel(1); break;
        case 'up': nav.up(); break;
        case 'refresh': nav.reload(); break;
        default: focusAddress();
      }
      return true;
    },
  }));
  const synthetic = tab.source === 'synthetic';
  const canNavigate = desktop && !synthetic;
  const filesReady = canNavigate && fileActionsAvailable && !composing && !selectionPending && !listing.state.viewPending && ['complete', 'capped'].includes(listing.state.status);
  const focusedEntry = listing.cache.row(location.position.focusedIndex);
  const recycleEligible = !focusedEntry || !location.position.selected.has(focusedEntry.id) || focusedEntry.kind === 'file';
  const captureSelection = (draggedId?: string) => {
    const selected = draggedId && !location.position.selected.has(draggedId) ? [draggedId] : [...location.position.selected];
    if (!filesReady || composition.current || !selected.length) return null;
    try { return { identity: listing.operationIdentity(tab.id, location.path), entryIds: selected }; }
    catch { setTabNotice('선택 항목을 확인하지 못했습니다. 목록을 새로 고친 뒤 다시 선택하세요.'); return null; }
  };
  const transferAction = (action: 'copy' | 'move' | 'paste') => {
    if (!filesReady || composition.current) return;
    if (action === 'paste') { transfer?.paste(location.path); return; }
    const selection = captureSelection();
    if (selection) transfer?.capture(action, selection);
  };
  const nativeAction = (action: 'reveal' | 'properties' | 'context_menu' | 'clipboard' | 'absolute_path' | 'relative_path' | 'paste' | 'system-paste') => {
    if (!filesReady || composition.current) return;
    if (action === 'paste') { transfer?.pasteDefault(location.path); return; }
    if (action === 'system-paste') { transfer?.importNative(location.path); return; }
    const selection = captureSelection();
    if (selection) transfer?.exportNative(selection, action, action === 'relative_path' ? projectRoot ?? location.path : null);
  };
  const requestFileAction = (kind: FileActionKind) => {
    if (!filesReady || composition.current || (kind === 'recycle' && !recycleEligible)) return;
    try {
      if (kind !== 'copy' && kind !== 'new_folder' && location.position.selected.size > 1024) { setTabNotice('이 작업은 한 번에 최대 1,024개를 지원합니다. 선택을 줄여 다시 시도하세요.'); return; }
      if (kind === 'new_folder') listing.operationIdentity(tab.id, location.path);
      const selection = kind === 'new_folder' ? null : captureSelection();
      if (kind !== 'new_folder' && !selection) return;
      if (kind === 'compress' || kind === 'extract') {
        const entry = listing.cache.row(location.position.focusedIndex);
        if (!entry || (kind === 'extract' && (selection?.entryIds.length !== 1 || entry.kind !== 'file'))) return;
        onFileAction?.({ kind, root: location.path, selection,
          archiveName: kind === 'compress' ? `${selection?.entryIds.length === 1 ? entry.name : '압축 파일'}.zip` : entry.name.replace(/\.(zip|7z|rar|tar|gz|bz2|xz|tgz|egg|alz)$/i, '') + ' (압축 해제)',
          archiveFormat: entry.name.split('.').at(-1)?.toLowerCase() });
      } else if (kind === 'rename') {
        if (selection?.entryIds.length !== 1) return;
        const entry = listing.cache.row(location.position.focusedIndex);
        if (!entry || entry.id !== selection.entryIds[0] || !['file', 'folder'].includes(entry.kind)) return;
        onFileAction?.({ kind, root: location.path, selection, renameEntry: { name: entry.name, kind: entry.kind as 'file' | 'folder' } });
      } else onFileAction?.({ kind, root: location.path, selection });
    } catch { /* A view transition disables file actions until its current snapshot is ready. */ }
  };
  return <section className="pane" data-pane={pane} hidden={!visible} aria-label={`패널 ${pane + 1}`} onFocusCapture={activate} onPointerDownCapture={activate}
    onCompositionStartCapture={() => { composition.current = true; setComposing(true); }}
    onCompositionEndCapture={() => { composition.current = false; setComposing(false); }}>
    <div className="pane-header"><TabBar pane={pane} state={nav.state} composing={() => composition.current} select={id => nav.setState(value => ({ ...value, activeId: id }))}
      add={() => { nav.add(); requestAnimationFrame(focusAddress); }} close={id => { nav.close(id); if (id === tab.id) requestAnimationFrame(focusAddress); }}
      reopen={() => { nav.reopen(); requestAnimationFrame(focusAddress); }} protect={() => { setTabNotice(null); nav.protect(); }}
      move={direction => nav.setState(value => moveTab(value, value.activeId, direction))} />
      <form className="navigation-toolbar" onSubmit={event => {
        event.preventDefault();
        if (!composition.current) nav.navigate(tab.draft);
      }}>
        <button type="button" aria-label="뒤로" title="뒤로 (Alt+←)" disabled={!canNavigate || tab.cursor === 0} onClick={() => nav.travel(-1)}><ArrowLeft size={15} aria-hidden="true" /></button>
        <button type="button" aria-label="앞으로" title="앞으로 (Alt+→)" disabled={!canNavigate || tab.cursor === tab.history.length - 1} onClick={() => nav.travel(1)}><ArrowRight size={15} aria-hidden="true" /></button>
        <button type="button" aria-label="상위 폴더" title="상위 폴더 (Alt+↑)" disabled={!canNavigate || parentPath(location.path) === null} onClick={nav.up}><ArrowUp size={15} aria-hidden="true" /></button>
        <button type="button" aria-label="새로 고침" title="새로 고침 (F5)" disabled={!canNavigate || !location.path} onClick={nav.reload}><RotateCw size={14} aria-hidden="true" /></button>
        <input title={location.path || tab.draft} ref={address} aria-label={`패널 ${pane + 1} 주소`} placeholder={synthetic ? '한글 IME 입력 시험' : '절대 경로 (예: C:\\Work)'}
          value={tab.draft} onChange={event => nav.update(value => ({ ...value, draft: event.target.value }))}
          onKeyDown={event => {
            if (event.key === 'Enter' && (composition.current || event.nativeEvent.isComposing || event.keyCode === 229)) event.preventDefault();
          }} />
        <button type="submit" aria-label="열기" title="입력한 폴더 열기 (Enter)" disabled={!canNavigate || !tab.draft || composing}><CornerDownLeft size={15} aria-hidden="true" /></button>
        {!synthetic && listing.state.status === 'loading' && <button type="button" onClick={() => listing.stop()}>취소</button>}
      {!synthetic && <FavoriteButtons path={location.path} canNewTab={nav.state.tabs.length < 32} disabled={!canNavigate || composing} open={(path, newTab) => { if (newTab) nav.add(path); else nav.navigate(path); }} />}
      </form></div>
    <div ref={content} className="pane-content" id={`pane-content-${pane}`} role="tabpanel" aria-labelledby={`tab-${tab.id}`}>
      <div className="pane-tools">
      <ViewToolbar ref={viewToolbar} key={`${nav.generation}/view/${tab.id}/${location.path}`} pane={pane} view={tab.view} disabled={!canNavigate || listing.state.refreshing === true} change={nav.changeView}
        returnToList={() => content.current?.querySelector<HTMLElement>('[role="listbox"]')?.focus({ preventScroll: true })} />
      {desktop && <PaneFileActions pane={pane} enabled={filesReady} selected={location.position.selected.size} recycleEligible={recycleEligible} request={requestFileAction}
        transferEnabled={Boolean(transfer) && !transfer?.busy} canPaste={Boolean(transfer?.clipboard)} transfer={transferAction} />}
      </div>
      {tab.history.length >= 32 && <div className="history-limit" role="status">탐색 기록 최대 32개 · 이전 기록은 자동 정리됩니다.</div>}
      {nav.actionError && <div className="listing-feedback" role="alert">{faultMessage(nav.actionError)}</div>}
      {tabNotice && <div className="listing-feedback" role="status">{tabNotice}</div>}
      {nav.state.selectionLimited && <div className="listing-feedback" role="alert">선택 상태 한도에 도달했습니다. 기존 선택을 해제하거나 탭을 닫은 뒤 다시 시도하세요.</div>}
      {!synthetic && <ListingFeedback state={listing.state} retry={nav.reload} />}
      {!synthetic && location.path && (!watchablePath(location.path) || unavailableWatch === watchKey)
        && <div className="listing-feedback" role="status">이 폴더는 자동 갱신 없이 표시됩니다. F5 또는 앱으로 돌아와 목록을 갱신하세요.</div>}
      <FileList key={`${nav.generation}/${tab.id}/${location.path}`} pane={pane} visible={visible} synthetic={synthetic} listing={listing} composing={() => composition.current}
        typeFilter={key => viewToolbar.current?.typeFilter(key) ?? false}
        menuActions={canNavigate ? [
          { label: '열기', disabled: !filesReady || location.position.selected.size !== 1, run: () => { if (location.position.focusedId) nav.activate(location.position.focusedId); } },
          { label: '탐색기에서 선택 항목 보기', disabled: !filesReady || !location.position.selected.size || location.position.selected.size > 128, run: () => nativeAction('reveal') },
          { label: 'Windows 메뉴 (시험)', description: 'Windows가 처리하는 메뉴입니다. 앱 작업 기록과 복구 기능은 적용되지 않습니다.', disabled: !filesReady || transfer?.busy || !location.position.selected.size || location.position.selected.size > 128, run: () => nativeAction('context_menu') },
          { label: '절대경로 복사', description: '선택 항목의 전체 경로를 텍스트로 복사합니다. 여러 항목은 한 줄씩 복사합니다.', disabled: !filesReady || !transfer || transfer.busy || !location.position.selected.size || location.position.selected.size > 128, run: () => nativeAction('absolute_path') },
          { label: projectRoot ? '상대경로 복사 (프로젝트 루트 기준)' : '상대경로 복사 (현재 폴더 기준)', description: `${projectRoot ? '프로젝트 루트' : '현재 패널 폴더'} 기준: ${projectRoot ?? location.path} · 여러 항목은 한 줄씩 복사합니다.`, disabled: !filesReady || !location.path || !transfer || transfer.busy || !location.position.selected.size || location.position.selected.size > 128, run: () => nativeAction('relative_path') },
          { label: 'Windows에 복사', disabled: !filesReady || !location.position.selected.size, run: () => nativeAction('clipboard') },
          { label: 'Windows에서 붙여넣기', disabled: !filesReady, run: () => nativeAction('system-paste') },
          { label: '복사 담기', disabled: !filesReady || !transfer || transfer.busy || !location.position.selected.size, run: () => transferAction('copy') },
          { label: '잘라내기', disabled: !filesReady || !transfer || transfer.busy || !location.position.selected.size || location.position.selected.size > 1024, run: () => transferAction('move') },
          { label: '앱에 붙여넣기', disabled: !filesReady || !transfer?.clipboard || transfer.busy, run: () => transferAction('paste') },
          { label: '압축하기…', disabled: !filesReady || !location.position.selected.size || location.position.selected.size > 1024, run: () => requestFileAction('compress') },
          ...(/\.(zip|7z|rar|tar|gz|bz2|xz|tgz|egg|alz)$/i.test(listing.cache.row(location.position.focusedIndex)?.name ?? '') && location.position.selected.size === 1 ? [{ label: '압축 해제…', disabled: !filesReady, run: () => requestFileAction('extract') }] : []),
          { label: '이름 변경', shortcut: 'F2', disabled: !filesReady || location.position.selected.size !== 1, run: () => requestFileAction('rename') },
          { label: '휴지통으로 이동', shortcut: 'Delete', disabled: !filesReady || !recycleEligible || !location.position.selected.size, run: () => requestFileAction('recycle') },
          { label: 'Windows 속성', shortcut: 'Alt+Enter', disabled: !filesReady || location.position.selected.size !== 1, run: () => nativeAction('properties') },
          { label: '새 폴더', disabled: !filesReady, run: () => requestFileAction('new_folder') },
          { label: '새로 고침', run: nav.reload },
        ] : undefined}
        onSelectionPending={changeSelectionPending}
        cancelTransfer={transfer?.clear}
        transfer={filesReady ? transferAction : undefined}
        nativeTransfer={filesReady ? nativeAction : undefined}
        beginPointerDrag={(event, id, started) => { const selection = captureSelection(id); if (selection) transfer?.beginPointerDrag(selection, event, started); }}
        beginDrag={() => { const selection = captureSelection(); return selection ? transfer?.beginDrag(selection) ?? null : null; }}
        canDrop={() => filesReady && !composition.current && (transfer?.canDrop() ?? false)} endDrag={() => transfer?.endDrag()}
        drop={(token, mode) => { if (filesReady && !composition.current) transfer?.drop(token, mode, location.path); }}
        position={location.position} onPosition={change => nav.setState(state => {
          if (!visible || state.activeId !== tab.id) return state;
          const target = state.tabs.find(value => value.id === tab.id);
          if (!target || currentLocation(target).path !== location.path) return state;
          const previous = currentLocation(target).position;
          const next = change(previous);
          return next === previous ? state : updateTab(state, rememberPosition(target, next));
        })} onActivate={nav.activate} />
    </div>
    <footer><span role="status">{synthetic ? '합성 데이터' : listingStatus(listing.state)} · {(synthetic ? 100_000 : listing.state.total).toLocaleString()}개</span><span className="selection-status" aria-live="polite">{location.position.selected.size.toLocaleString()}개 선택</span><span>{synthetic ? '가상 목록' : listing.state.firstBatchMs !== undefined ? `첫 배치 ${listing.state.firstBatchMs.toFixed(1)}ms` : '읽기 전용'}</span></footer>
  </section>;
}
