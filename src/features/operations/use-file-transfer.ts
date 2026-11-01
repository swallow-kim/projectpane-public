import { useEffect, useRef, useState } from 'react';
import type { FileActionDraft, OperationSelection, TransferMode, TransferPreview } from '../../shared/operations';
import { captureTransfer, clipboardSequence, discardFileAction, executeFileAction, exportToExplorer, importFromExplorer, operationError, prepareFileAction, type ExplorerExportAction } from './operation-client';
import { trackPointerTransfer } from './pointer-transfer';
import { toFaultCode } from '../../shared/fault';

export const INTERNAL_TRANSFER = 'application/x-projectpane-transfer';

export function useFileTransfer(projectId: string | null, open: (draft: FileActionDraft) => void, target: (x: number, y: number) => string | null, refresh: () => void) {
  const [clipboard, setClipboard] = useState<TransferPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const pending = useRef(false);
  const generation = useRef(0);
  const drag = useRef<{ token: string; selection: OperationSelection } | null>(null);
  const cancelPointer = useRef<(() => void) | null>(null);
  const cutSequence = useRef<number | null>(null);
  useEffect(() => {
    generation.current++; setClipboard(null); setError(null); setNotice(null); drag.current = null;
    cutSequence.current = null;
    return () => { generation.current++; cancelPointer.current?.(); };
  }, [projectId]);
  const copyPrepared = async (draft: FileActionDraft, destination: string, epoch: number, copyName?: string, skipExisting = false) => {
    let dispatching = false;
    try {
      const preview = await prepareFileAction(draft, destination, projectId, copyName, skipExisting);
      if (epoch !== generation.current) { await discardFileAction(preview.id); return; }
      // Keep the native prepare/revalidate/execute contract. Only the redundant
      // user confirmation is removed for non-overwriting copy operations.
      dispatching = true;
      await executeFileAction(preview.id);
      if (epoch === generation.current) setNotice('복사 요청을 접수했습니다 · 상단 파일 작업에서 진행과 결과를 확인하세요');
    } catch (error) {
      if (epoch === generation.current && !dispatching && toFaultCode(error) === 'destination_exists') {
        open({ ...draft, destination, copyConflict: true });
        return;
      }
      if (epoch === generation.current) setError(dispatching
        ? '복사 접수 상태를 확인하지 못했습니다. 상단 파일 작업 버튼을 확인하세요. 자동으로 다시 실행하지 않습니다.'
        : operationError(error, skipExisting ? 'copy_skip_existing' : copyName !== undefined ? 'copy_as' : 'copy'));
    } finally { refresh(); }
  };
  const copyNow = async (draft: FileActionDraft, destination: string, copyName?: string, skipExisting = false) => {
    if (pending.current || draft.kind !== 'copy') return;
    pending.current = true; setBusy(true); setError(null); setNotice(null);
    try { await copyPrepared(draft, destination, generation.current, copyName, skipExisting); }
    finally { pending.current = false; setBusy(false); }
  };
  const capture = async (mode: TransferMode, selection: OperationSelection, destination?: string) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null); setNotice(null); setClipboard(null);
    const epoch = generation.current;
    cutSequence.current = null;
    try {
      const sequence = mode === 'move' && !destination ? await clipboardSequence() : null;
      const result = await captureTransfer(mode, selection);
      if (epoch !== generation.current) return;
      if (sequence !== null) {
        const current = await clipboardSequence();
        if (epoch !== generation.current) return;
        if (sequence !== current) { setError('Windows 클립보드가 바뀌어 앱 잘라내기를 취소했습니다.'); return; }
      }
      cutSequence.current = sequence;
      setClipboard(result);
      if (destination) {
        const draft = { kind: mode, root: destination, selection: null, transfer: result };
        if (mode === 'copy') await copyPrepared(draft, destination, epoch); else open(draft);
      }
    } catch (error) { if (epoch === generation.current) setError(operationError(error)); }
    finally { pending.current = false; setBusy(false); }
  };
  const exportNative = async (selection: OperationSelection, action: ExplorerExportAction, basePath: string | null = null) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null); setNotice(null);
    if (action === 'clipboard') { setClipboard(null); cutSequence.current = null; }
    const epoch = generation.current;
    try {
      const accepted = await exportToExplorer(selection, action, basePath);
      if (action === 'absolute_path' || action === 'relative_path') {
        if (!accepted) throw new Error('Path clipboard write not confirmed');
        if (epoch === generation.current) {
          setClipboard(null); cutSequence.current = null;
          setNotice(action === 'absolute_path'
            ? `절대경로 ${selection.entryIds.length}개를 텍스트로 복사했습니다`
            : `상대경로 ${selection.entryIds.length}개를 텍스트로 복사했습니다 · 기준: ${basePath}`);
        }
        return;
      }
      if (epoch === generation.current) setNotice(action === 'reveal' ? '탐색기에 선택 항목 표시를 요청했습니다'
        : action === 'context_menu' ? accepted ? 'Windows 메뉴의 명령을 전달했습니다 · Windows가 처리하며 앱 작업 기록에 남지 않습니다' : 'Windows 메뉴를 취소했습니다'
        : action === 'properties' ? 'Windows 속성 창을 요청했습니다'
        : action === 'clipboard' ? 'Windows에 복사했습니다 · 탐색기에서 Ctrl+V'
        : accepted ? '외부 앱에 복사 요청을 전달했습니다' : '외부 끌기를 취소했습니다');
    }
    catch (error) { if (epoch === generation.current) setError(action === 'absolute_path' || action === 'relative_path'
      ? '경로를 클립보드에 복사하지 못했습니다. 선택 항목과 현재 패널의 기준 폴더를 확인하고 다시 시도하세요.'
      : action === 'context_menu'
      ? `Windows 메뉴를 처리하지 못했습니다. 명령이 실행됐는지 확인한 뒤, 기본 메뉴 또는 ‘탐색기에서 선택 항목 보기’를 사용하세요. ${operationError(error)}`
      : operationError(error)); }
    finally { pending.current = false; setBusy(false); }
  };
  const importNative = async (root: string, paths: readonly string[] | null) => {
    if (pending.current) return;
    pending.current = true; setBusy(true); setError(null); setNotice(null); setClipboard(null);
    const epoch = generation.current;
    cutSequence.current = null;
    try {
      const result = await importFromExplorer(paths);
      if (epoch === generation.current) await copyPrepared({ kind: 'copy', root, selection: null, transfer: result }, root, epoch);
    } catch (error) {
      if (epoch === generation.current) setError(`파일 복사를 가져오지 못했습니다. WSL/네트워크 연결과 접근 권한을 확인하세요. 외부 잘라내기는 지원하지 않으므로 복사를 사용하세요. ${operationError(error)}`);
    } finally { pending.current = false; setBusy(false); }
  };
  const pasteDefault = async (root: string) => {
    if (pending.current) return;
    if (clipboard?.mode !== 'move') { await importNative(root, null); return; }
    pending.current = true; setBusy(true); setError(null); setNotice(null);
    const epoch = generation.current;
    const captured = clipboard;
    try {
      const current = await clipboardSequence();
      if (epoch !== generation.current) return;
      if (cutSequence.current === null || cutSequence.current !== current) {
        cutSequence.current = null; setClipboard(null);
        setError('Windows 클립보드가 바뀌어 앱 잘라내기를 취소했습니다. Ctrl+V를 다시 누르면 Windows에서 붙여넣습니다.');
        return;
      }
      open({ kind: 'move', root, selection: null, transfer: captured });
    } catch (error) { if (epoch === generation.current) setError(operationError(error)); }
    finally { pending.current = false; setBusy(false); }
  };
  return {
    clipboard, busy, error, notice,
    clear: () => {
      if (pending.current || !clipboard) return false;
      setClipboard(null); cutSequence.current = null; setError(null); setNotice('앱 보관을 비웠습니다');
      return true;
    },
    exportNative: (selection: OperationSelection, action: Exclude<ExplorerExportAction, 'drag'>, basePath: string | null = null) => { void exportNative(selection, action, basePath); },
    importNative: (root: string, paths: readonly string[] | null = null) => { void importNative(root, paths); },
    pasteDefault: (root: string) => { void pasteDefault(root); },
    beginPointerDrag: (selection: OperationSelection, event: PointerEvent, started: () => void) => {
      if (pending.current || event.button !== 0) return;
      cancelPointer.current?.();
      cancelPointer.current = trackPointerTransfer(event, {
        started,
        target,
        internal: (root, move) => { void capture(move ? 'move' : 'copy', selection, root); },
        external: () => { void exportNative(selection, 'drag'); },
      });
    },
    capture: (mode: TransferMode, selection: OperationSelection) => { void capture(mode, selection); },
    paste: (root: string) => {
      if (!clipboard || pending.current) return;
      const draft = { kind: clipboard.mode, root, selection: null, transfer: clipboard };
      if (clipboard.mode === 'copy') void copyNow(draft, root); else open(draft);
    },
    submitted: (draft: FileActionDraft) => {
      if (draft.transfer?.mode === 'move') setClipboard(current => current?.token === draft.transfer?.token ? null : current);
    },
    beginDrag: (selection: OperationSelection) => {
      if (pending.current) return null;
      const token = crypto.randomUUID();
      drag.current = { token, selection };
      return token;
    },
    canDrop: () => drag.current !== null && !pending.current,
    endDrag: () => { drag.current = null; },
    drop: (token: string, mode: TransferMode, root: string) => {
      const source = drag.current;
      drag.current = null;
      if (!source || token !== source.token) return;
      void capture(mode, source.selection, root);
    },
  };
}
export type FileTransfer = ReturnType<typeof useFileTransfer>;
