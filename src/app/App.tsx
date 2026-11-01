import { useCallback, useEffect, useRef, useState } from 'react';
import { Splitter, type LayoutCount } from './Splitter';
import { ExplorerPane, type PaneHandle } from '../features/explorer/ExplorerPane';
import { paneShortcut, targetPane } from '../features/explorer/shortcuts';
import { ImeShortcutGuard } from '../features/explorer/ime-shortcut-guard';
import { desktop } from '../shared/ipc';
import type { StartupConfig } from '../shared/startup';
import { ProjectToolbar } from '../features/projects/ProjectToolbar';
import { useProjectWorkspace } from '../features/projects/use-project-workspace';
import { captureWorkspace, restoreWorkspace } from '../features/projects/workspace-adapter';
import type { PaneState } from '../features/explorer/navigation';
import { OperationCloseDialog } from '../features/operations/OperationCloseDialog';
import { FileActionDialog } from '../features/operations/FileActionDialog';
import { RecycleDialog } from '../features/operations/RecycleDialog';
import { OperationTray } from '../features/operations/OperationTray';
import { useOperationState } from '../features/operations/use-operation-state';
import type { FileActionDraft } from '../shared/operations';
import { useFileTransfer } from '../features/operations/use-file-transfer';
import { clearDropFeedback, showDropFeedback } from '../features/operations/drop-feedback';
import { nativeDrop } from '../features/operations/native-drop';
import { bindFocusRefresh } from '../features/explorer/focus-refresh';
import { bindWatchRefresh } from '../features/explorer/watch-refresh';
import { pollWatch } from '../shared/watch-ipc';
import { listen } from '@tauri-apps/api/event';
import { FavoritesContext, useFavoritesStore } from '../features/favorites/use-favorites';
import { FavoritesDialog } from '../features/favorites/FavoritesDialog';
import { QuickPalette } from '../features/navigation/QuickPalette';
import { paletteItems, type PaletteItem, type PaletteAction } from '../features/navigation/palette';
import { UpdateControl } from '../features/updates/UpdateControl';

const warningMessages = {
  state_unavailable: '창 위치를 저장할 수 없습니다.',
  state_invalid: '저장된 창 위치를 읽지 못해 기본 위치로 시작했습니다.',
  log_unavailable: '진단 기록을 저장할 수 없습니다.',
  window_restore_failed: '창 위치를 복원하지 못해 기본 위치로 시작했습니다.',
};

export function App({ startup }: { readonly startup: StartupConfig }) {
  const favorites = useFavoritesStore();
  const [showFavorites, setShowFavorites] = useState(false);
  const [updatePreparing, setUpdatePreparing] = useState(false);
  const [palette, setPalette] = useState<readonly PaletteItem[] | null>(null);
  const panes = useRef<Array<PaneHandle | null>>([]);
  const active = useRef<number | null>(null);
  const composing = useRef(false);
  const [layout, setLayout] = useState<LayoutCount>(4);
  const [columns, setColumns] = useState(50);
  const [rows, setRows] = useState(50);
  const [fileAction, setFileAction] = useState<FileActionDraft | null>(null);
  const fileActionOpen = useRef(false);
  const pendingRefresh = useRef(false);
  const focusRefresh = useRef<ReturnType<typeof bindFocusRefresh> | null>(null);
  const refreshFiles = useCallback(() => {
    if (fileActionOpen.current) { pendingRefresh.current = true; return; }
    pendingRefresh.current = false;
    focusRefresh.current?.acknowledge();
    panes.current.forEach(pane => pane?.refreshFiles?.());
  }, []);
  const changeFileAction = useCallback((draft: FileActionDraft | null) => {
    fileActionOpen.current = draft !== null;
    setFileAction(draft);
    if (!draft && pendingRefresh.current) refreshFiles();
  }, [refreshFiles]);
  const operations = useOperationState(refreshFiles);
  const [restored, setRestored] = useState<readonly PaneState[] | undefined>();
  const projects = useProjectWorkspace(startup, root => captureWorkspace(root,
    { layout, columns, rows, activePane: Math.min(active.current ?? 0, layout - 1) }, panes.current), loaded => {
    if (!loaded) return;
    const next = restoreWorkspace(loaded.project.rootPath, loaded.workspace);
    active.current = next.activePane; setLayout(next.layout); setColumns(next.columns); setRows(next.rows);
    setRestored(next.panes);
    next.panes.forEach((state, index) => panes.current[index]?.restore(state));
  }, favorites.flush);
  const transferTarget = useCallback((x: number, y: number) => {
    if (document.querySelector('dialog[open]') || composing.current || !Number.isFinite(x) || !Number.isFinite(y)) return null;
    const list = document.elementFromPoint(x, y)?.closest('.file-list');
    const pane = list?.closest<HTMLElement>('[data-pane]');
    return pane ? panes.current[Number(pane.dataset.pane)]?.transferTarget?.() ?? null : null;
  }, []);
  const transfer = useFileTransfer(projects.toolbar.selectedId, changeFileAction, transferTarget, operations.refresh);
  useEffect(() => { if (desktop && projects.ready && projects.available) void favorites.reload(); }, [projects.ready, projects.available, favorites.reload]);
  const importNative = useRef(transfer.importNative);
  importNative.current = transfer.importNative;
  useEffect(() => {
    if (!desktop) return;
    let disposed = false;
    const unlisten: (() => void)[] = [];
    const bind = (name: string, drop = false) => listen<{ paths?: string[]; position?: { x: number; y: number } }>(name, event => {
      if (disposed) return;
      const payload = nativeDrop(event.payload, window.devicePixelRatio, drop);
      if (!payload) { clearDropFeedback(); return; }
      const { x, y, paths } = payload;
      const target = transferTarget(x, y);
      if (!target) { clearDropFeedback(); return; }
      if (drop) { clearDropFeedback(); if (Array.isArray(paths)) importNative.current(target, paths); }
      else showDropFeedback(x, y, 'copy');
    }).then(stop => { if (disposed) stop(); else unlisten.push(stop); });
    void Promise.all([bind('tauri://drag-enter'), bind('tauri://drag-over'), bind('tauri://drag-drop', true),
      listen('tauri://drag-leave', () => { if (!disposed) clearDropFeedback(); }).then(stop => { if (disposed) stop(); else unlisten.push(stop); })])
      .catch(() => { clearDropFeedback(); /* Keyboard/menu paste remains available. */ });
    return () => { disposed = true; unlisten.forEach(stop => stop()); clearDropFeedback(); };
  }, [transferTarget]);
  useEffect(() => { clearDropFeedback(); }, [projects.toolbar.selectedId, projects.busy, transfer.busy, fileAction]);
  const refreshBusy = useRef(false);
  const watchEnabled = useRef(false);
  const [watchFailed, setWatchFailed] = useState(false);
  const closing = operations.state !== null && operations.state.closeState !== 'open';
  const openPalette = () => {
    if (!projects.ready || projects.busy || closing || composing.current || document.querySelector('dialog[open]')) return;
    setPalette(paletteItems(projects.toolbar.projects, panes.current.map(pane => pane?.snapshot()), favorites.folders, active.current ?? 0));
  };
  const paletteOpen = useRef(openPalette); paletteOpen.current = openPalette;
  const runPalette = (action: PaletteAction) => {
    if (projects.busy || closing) return;
    const pane = panes.current[Math.min(active.current ?? 0, layout - 1)];
    if (action.kind === 'project') { projects.toolbar.onSelect(action.id); return; }
    if (action.kind === 'tab') {
      if (action.pane >= layout) setLayout(action.pane < 2 ? 2 : 4);
      active.current = action.pane; panes.current[action.pane]?.selectTab?.(action.id); return;
    }
    if (action.kind === 'folder') { pane?.openFolder?.(action.path); return; }
    if (action.command === 'protect') pane?.protectTab?.();
    else pane?.shortcut(action.command);
  };
  refreshBusy.current = !projects.ready || projects.busy || transfer.busy || closing;
  watchEnabled.current = projects.ready && !projects.busy && !closing;
  useEffect(() => {
    if (!desktop) return;
    const binding = bindWatchRefresh(() => panes.current, () => watchEnabled.current,
      () => !refreshBusy.current && !composing.current && !document.hidden && !document.querySelector('dialog[open]'),
      pollWatch, () => setWatchFailed(true));
    return () => binding.dispose();
  }, []);
  useEffect(() => {
    if (!desktop) return;
    const binding = bindFocusRefresh(window, document, refreshFiles, () => !refreshBusy.current && !composing.current && !document.querySelector('dialog[open]'));
    focusRefresh.current = binding;
    return () => { binding.dispose(); focusRefresh.current = null; };
  }, [refreshFiles]);
  useEffect(() => { focusRefresh.current?.resume(); }, [projects.ready, projects.busy, transfer.busy, closing, fileAction]);
  useEffect(() => { projects.capture(); }, [layout, columns, rows, projects.capture]);
  useEffect(() => { changeFileAction(null); }, [projects.toolbar.selectedId, changeFileAction]);
  useEffect(() => {
    const frame = requestAnimationFrame(() => performance.mark('projectpane-ui-ready'));
    return () => cancelAnimationFrame(frame);
  }, []);
  useEffect(() => {
    const guard = new ImeShortcutGuard();
    const captureKeydown = (event: KeyboardEvent) => {
      if (guard.shouldSuppress(event, composing.current)) { event.preventDefault(); return; }
      // The project toolbar isolates its own keyboard events. Only the global
      // palette command crosses that boundary; dialogs and IME still own input.
      if (paneShortcut(event, composing.current) === 'palette' && !document.querySelector('dialog[open]')) {
        event.preventDefault(); if (!event.repeat) paletteOpen.current();
      }
    };
    const keyup = (event: KeyboardEvent) => { guard.keyup(event.code); };
    const reset = () => { composing.current = false; guard.reset(); };
    window.addEventListener('keydown', captureKeydown, true);
    window.addEventListener('keyup', keyup, true);
    window.addEventListener('blur', reset);
    return () => {
      window.removeEventListener('keydown', captureKeydown, true);
      window.removeEventListener('keyup', keyup, true);
      window.removeEventListener('blur', reset);
    };
  }, []);
  useEffect(() => {
    const keydown = (event: KeyboardEvent) => {
      const shortcut = paneShortcut(event, composing.current);
      if (!shortcut || event.defaultPrevented || projects.busy) return;
      if (document.querySelector('dialog[open]')) return;
      const target = panes.current[targetPane(active.current, shortcut, layout)];
      if (!target) return;
      if (target.shortcut(shortcut)) event.preventDefault();
    };
    window.addEventListener('keydown', keydown);
    return () => {
      window.removeEventListener('keydown', keydown);
    };
  }, [layout, projects.busy]);

  const changeLayout = (count: LayoutCount) => {
    if (count > layout && projects.rootPath) {
      for (let index = layout; index < count; index++) panes.current[index]?.initializeRoot?.(projects.rootPath);
    }
    setLayout(count);
    if (active.current !== null && active.current >= count) {
      active.current = 0;
      panes.current[0]?.focusAddress();
    }
  };

  return <FavoritesContext.Provider value={favorites}><main onCompositionStartCapture={() => { composing.current = true; }} onCompositionEndCapture={() => { composing.current = false; }}
    onDragOver={event => { if (event.dataTransfer.types.includes('Files')) event.preventDefault(); }} onDrop={event => event.preventDefault()}>
    {desktop && <OperationCloseDialog updating={updatePreparing} />}
    {showFavorites && <FavoritesDialog close={() => setShowFavorites(false)} />}
    {palette && <QuickPalette items={palette} dismiss={() => setPalette(null)} run={runPalette} />}
    <header className="app-header"><h1>ProjectPane</h1>
      {desktop && <UpdateControl ready={projects.ready} persisted={Boolean(projects.toolbar.selectedId) && !projects.toolbar.readOnly} onPreparing={setUpdatePreparing} />}
      {desktop && <OperationTray state={operations.state} unavailable={operations.error || operations.state?.closeState === 'blocked'} refresh={operations.refresh} />}
      <button className="favorites-manage" disabled={!projects.ready || projects.busy || closing} title="프로젝트·탭·즐겨찾기·최근 폴더·명령 (Ctrl+Shift+P)" onClick={openPalette}>빠른 이동</button>
      {desktop && <button className="favorites-manage" onClick={() => setShowFavorites(true)}>즐겨찾기 관리</button>}
      <div inert={projects.busy} className="layout-toolbar" role="group" aria-label="패널 분할">
        {([1, 2, 4] as const).map(count => <button key={count} aria-pressed={layout === count} onClick={() => changeLayout(count)}>{count}패널</button>)}
      </div>
      <details className="app-help"><summary>도움말</summary><div>
        <p>라이선스·제3자 고지와 MPL 소스 안내: 앱 실행 파일 옆 licenses 폴더. 설치형 기본 위치는 %LOCALAPPDATA%\ProjectPane\licenses입니다.</p>
        <p>Windows 복사/붙여넣기: Ctrl+C / V · 앱 안 잘라내기: Ctrl+X</p>
        <p>앱 보관 복사/붙여넣기: Ctrl+Shift+C / V · 외부 끌기: 복사만</p>
        <p>WSL·네트워크는 복사, 파일 이동은 로컬 고정 드라이브를 지원합니다. 다른 드라이브의 폴더 이동은 로컬 NTFS의 일반 폴더에 한정합니다. 휴지통은 고정 드라이브 일반 파일만 지원합니다.</p>
        {!desktop && <p>브라우저 합성 목록 시험 · 실제 파일 조회는 데스크톱 앱에서 실행하세요.</p>}
      </div></details>
    </header>
    {startup.error && <div className="notice" role="alert">시작 경로를 적용하지 못했습니다. --folder 옵션을 확인하거나 경로를 직접 입력하세요.</div>}
    {startup.warnings.map(warning => <div className="notice" role="alert" key={warning}>{warningMessages[warning]}</div>)}
    {watchFailed && <div className="notice" role="status">자동 갱신을 사용할 수 없습니다. F5 또는 앱으로 돌아와 목록을 갱신하세요.</div>}
    {desktop && projects.available && <ProjectToolbar {...projects.toolbar} />}
    {desktop && projects.ready && !projects.available && <div className="notice" role="status">프로젝트 저장 서비스를 사용할 수 없습니다. <button onClick={projects.toolbar.onRetry}>프로젝트 다시 시도</button></div>}
    {projects.toolbar.readOnly && <div className="notice"><button disabled={projects.busy} onClick={projects.recoverAsNew}>현재 상태를 새 프로젝트로 저장</button></div>}
    {projects.warnings.length > 0 && <div className="notice" role="alert">저장된 상태를 복구하거나 일부 상태를 적용하지 못했습니다. {projects.toolbar.readOnly ? "원본 보호를 위해 저장을 차단했습니다." : "현재 상태를 확인하세요."}</div>}
    {!projects.ready && <div className="notice" role="status">프로젝트 불러오는 중…</div>}
    {fileAction && (fileAction.kind === 'recycle'
      ? <RecycleDialog draft={fileAction} projectId={projects.toolbar.selectedId} close={() => changeFileAction(null)} submitted={operations.refresh} />
      : <FileActionDialog draft={fileAction} projectId={projects.toolbar.selectedId} close={() => changeFileAction(null)} submitted={() => { transfer.submitted(fileAction); operations.refresh(); }} />)}
    <div className="explorer-workspace" id="project-workspace" role={desktop && projects.available ? 'tabpanel' : undefined} aria-label="현재 프로젝트 탐색기">
    <div inert={projects.busy} className={`panes layout-${layout}`} style={{ gridTemplateColumns: layout === 1 ? 'minmax(0, 1fr)' : `minmax(0, ${columns}fr) 0px minmax(0, ${100 - columns}fr)`, gridTemplateRows: layout === 4 ? `minmax(min-content, ${rows}fr) 0px minmax(min-content, ${100 - rows}fr)` : 'minmax(min-content, 1fr)' }}>
      {projects.ready && [0, 1, 2, 3].map(pane => <ExplorerPane key={pane} pane={pane} visible={pane < layout} initialPath={projects.rootPath ?? startup.folders[pane]} projectRoot={projects.rootPath}
        restored={restored?.[pane]} onWorkspaceChange={projects.capture} fileActionsAvailable={operations.available && !projects.busy && !transfer.busy} onFileAction={changeFileAction} transfer={transfer}
        ref={handle => { panes.current[pane] = handle; }} activate={() => { active.current = pane; projects.capture(); }} />)}
      {layout > 1 && <Splitter axis="columns" value={columns} change={setColumns} />}
      {layout === 4 && <Splitter axis="rows" value={rows} change={setRows} />}
    </div>
    </div>
    {desktop && <div className="app-status">
      {favorites.error && <div className="notice" role="alert">{favorites.error} <button disabled={favorites.busy} onClick={() => void favorites.reload()}>즐겨찾기 다시 불러오기</button></div>}
      <div className="transfer-status" role="status">{transfer.busy ? '파일 요청 처리 중…' : transfer.notice ? transfer.notice : transfer.clipboard ? `앱 안에 ${transfer.clipboard.mode === 'copy' ? '복사' : '잘라내기'} ${transfer.clipboard.count}개 담음 · ${transfer.clipboard.mode === 'move' ? '앱 안에서 Ctrl+V (Windows 잘라내기 아님)' : 'Ctrl+Shift+V'}` : '준비됨'}</div>
      {transfer.error && <div className="notice" role="alert">{transfer.error}</div>}

    </div>}
  </main></FavoritesContext.Provider>;
}
