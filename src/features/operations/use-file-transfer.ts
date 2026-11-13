import { useOperationMessage } from './use-operation-message';
import { t, locale, useLanguage } from '../../shared/i18n';
import { beginCopyJob, captureCopyClipboard, discardCopyCapture, exportCopyCapture, stageCopy } from './copy-job-client';
import { useEffect, useRef, useState } from 'react';
import type { FileActionDraft, OperationSelection, TransferMode, TransferPreview } from '../../shared/operations';
import { captureTransfer, clipboardSequence, exportToExplorer, operationError, type ExplorerExportAction } from './operation-client';
import { trackPointerTransfer } from './pointer-transfer';
import { canFallbackToCopy } from './cut-fallback';
import { IpcFault, toFaultCode } from '../../shared/fault';

function stageLabel(stage: string | null): string {
  switch (stage) {
    case '복사 작업 시작': return t('복사 작업 시작', 'Starting copy job');
    case '선택 항목 수집': return t('선택 항목 수집', 'Collecting selected items');
    case 'Windows로 선택 항목 전달': return t('Windows로 선택 항목 전달', 'Sending selected items to Windows');
    case 'Windows 클립보드 읽기': return t('Windows 클립보드 읽기', 'Reading Windows clipboard');
    case '끌어온 항목 수집': return t('끌어온 항목 수집', 'Collecting dropped items');
    default: return t('파일 요청', 'File request');
  }
}
export const INTERNAL_TRANSFER = 'application/x-projectpane-transfer';

export function useFileTransfer(projectId: string | null, open: (draft: FileActionDraft) => void, target: (x: number, y: number) => string | null, refresh: () => void) {
  useLanguage();
  const [clipboard, setClipboard] = useState<TransferPreview | null>(null);
  const [busy, setBusy] = useState(false);
  const [copyRevision, setCopyRevision] = useState(0);
  const [stage, updateStage] = useState<string | null>(null);
  const stageRef = useRef<string | null>(null);
  const setStage = (value: string | null) => { stageRef.current = value; updateStage(value); };
  const copyCapture = useRef<string | null>(null);
  const copyCount = useRef(0);
  const dispatchUncertain = useRef(false);
  const startCopy = async (id: string, destination: string) => {
    const epoch = generation.current;
    setStage('복사 작업 시작'); dispatchUncertain.current = false;
    let job;
    try { job = await beginCopyJob(id, destination, projectId); }
    catch (error) {
      if (epoch !== generation.current) return true;
      if (toFaultCode(error) === 'destination_exists') {
        open({ kind: 'copy', root: destination, destination, selection: null,
          transfer: { token: id, mode: 'copy', count: copyCount.current }, copyConflict: true });
        return false;
      }
      dispatchUncertain.current = ['ipc_error', 'io_error', 'protocol_error', 'timeout'].includes(toFaultCode(error));
      if (dispatchUncertain.current) setCopyRevision(value => value + 1);
      throw error;
    }
    setCopyRevision(value => value + 1);
    if (epoch !== generation.current) return true;
    setNotice(() => t('복사 작업 {count}개 시작. 복사 작업에서 진행 상황과 결과를 확인하세요.', 'Started copying {count} items. Check Copy jobs for progress and results.', { count: job.rootsTotal.toLocaleString(locale()) }));
    return true;
  };
  const [error, setError] = useOperationMessage();
  const [notice, setNotice] = useOperationMessage();
  const pending = useRef(false);
  const generation = useRef(0);
  const drag = useRef<{ token: string; selection: OperationSelection } | null>(null);
  const cancelPointer = useRef<(() => void) | null>(null);
  const cutSequence = useRef<number | null>(null);
  const copyFallback = useRef<TransferPreview | undefined>(undefined);
  useEffect(() => {
    generation.current++; if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {}); copyCapture.current = null; setClipboard(null); setError(null); setNotice(null); drag.current = null;
    cutSequence.current = null; copyFallback.current = undefined;
    return () => { generation.current++; cancelPointer.current?.(); if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {}); copyCapture.current = null; };
  }, [projectId]);
  const copyFailure = (error: unknown) => { const failedStage = stageRef.current; const uncertain = dispatchUncertain.current; return () => uncertain
    ? t("복사 실행 상태를 확인하지 못했습니다. 복사 작업에서 결과를 확인하세요. 자동으로 다시 실행하지 않습니다.", "The copy execution status could not be confirmed. Check Copy jobs for results. It will not run again automatically.")
    : stageLabel(failedStage) + ': ' + operationError(error); };
  const capture = async (mode: TransferMode, selection: OperationSelection, destination?: string) => {
    if (pending.current) { setError(() => t('현재 {stage} 처리 중입니다. 완료 후 다시 시도하거나 항목 수집을 취소하세요.', '{stage} is in progress. Try again when it finishes, or cancel item collection.', { stage: stageLabel(stage) })); return; }
    pending.current = true; dispatchUncertain.current = false; setBusy(true); setError(null); setNotice(null); setClipboard(null);
    const epoch = generation.current;
    cutSequence.current = null; copyFallback.current = undefined;
    try {
      const sequence = mode === 'move' && !destination ? await clipboardSequence() : null;
      setStage('선택 항목 수집');
      if (mode === 'copy') {
        const result = await stageCopy(selection, () => epoch === generation.current);
        if (epoch !== generation.current) { await discardCopyCapture(result.id); return; }
        if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {});
        copyCapture.current = result.id; copyCount.current = result.count;
        setClipboard({ token: result.id, mode: 'copy', count: result.count });
        if (destination) await startCopy(result.id, destination);
        return;
      }
      let result: TransferPreview;
      try {
        if (selection.entryIds.length > 1024) throw new IpcFault('payload_limit');
        result = await captureTransfer(mode, selection);
      } catch (failure) {
        if (epoch !== generation.current) return;
        if (!canFallbackToCopy(failure)) throw failure;
        const copied = await stageCopy(selection, () => epoch === generation.current);
        if (epoch !== generation.current) { await discardCopyCapture(copied.id); return; }
        if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {});
        copyCapture.current = copied.id; copyCount.current = copied.count;
        const transfer = { token: copied.id, mode: 'copy' as const, count: copied.count };
        setClipboard(transfer);
        open({ kind: 'copy', root: destination ?? '', destination: destination ?? '', selection: null, transfer, copyFallbackCode: toFaultCode(failure) });
        return;
      }
      // Retain a separately validated, copy-only snapshot while the source
      // listing is alive. Navigation must not turn fallback into a new selection.
      try {
        const copied = await stageCopy(selection, () => epoch === generation.current);
        if (epoch !== generation.current) { await discardCopyCapture(copied.id); return; }
        if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {});
        copyCapture.current = copied.id; copyCount.current = copied.count;
        copyFallback.current = { token: copied.id, mode: 'copy', count: copied.count };
      } catch { /* A failed copy snapshot does not disable a supported move. */ }
      if (epoch !== generation.current) return;
      if (sequence !== null) {
        const current = await clipboardSequence();
        if (epoch !== generation.current) return;
        if (sequence !== current) { setError(() => t("Windows 클립보드가 바뀌어 앱 잘라내기를 취소했습니다.", "The Windows clipboard changed, so the in-app cut was cancelled.")); return; }
      }
      cutSequence.current = sequence;
      setClipboard(result);
      if (destination) {
        const draft = { kind: mode, root: destination, selection: null, transfer: result, copyFallback: copyFallback.current };
        open(draft);
      }
    } catch (error) { if (epoch === generation.current) setError(copyFailure(error)); }
    finally { pending.current = false; setBusy(false); setStage(null); }
  };
  const exportNative = async (selection: OperationSelection, action: ExplorerExportAction, basePath: string | null = null) => {
    if (pending.current) { setError(() => t('현재 {stage} 처리 중입니다. 완료 후 다시 시도하거나 항목 수집을 취소하세요.', '{stage} is in progress. Try again when it finishes, or cancel item collection.', { stage: stageLabel(stage) })); return; }
    pending.current = true; dispatchUncertain.current = false; setBusy(true); setError(null); setNotice(null);
    if (action === 'clipboard') { setClipboard(null); cutSequence.current = null; }
    const epoch = generation.current;
    try {
      setStage('Windows로 선택 항목 전달');
      let accepted: boolean;
      if (action === 'clipboard' || action === 'drag') {
        setStage('선택 항목 수집');
        const captured = await stageCopy(selection, () => epoch === generation.current);
        try {
          if (epoch !== generation.current) return;
          setStage('Windows로 선택 항목 전달');
          accepted = await exportCopyCapture(captured.id, action);
          if (action === 'clipboard' && !accepted) throw new Error('Clipboard write not confirmed');
        } finally { await discardCopyCapture(captured.id).catch(() => {}); }
      } else accepted = await exportToExplorer(selection, action, basePath);
      if (action === 'absolute_path' || action === 'relative_path') {
        if (!accepted) throw new Error('Path clipboard write not confirmed');
        if (epoch === generation.current) {
          setClipboard(null); cutSequence.current = null;
          setNotice(() => action === 'absolute_path'
            ? t('절대경로 {count}개를 텍스트로 복사했습니다', 'Copied {count} absolute paths as text', { count: selection.entryIds.length })
            : t('상대경로 {count}개를 텍스트로 복사했습니다 · 기준: {base}', 'Copied {count} relative paths as text · Base: {base}', { count: selection.entryIds.length, base: basePath ?? '' }));
        }
        return;
      }
      if (epoch === generation.current) setNotice(() => action === 'reveal' ? t("탐색기에 선택 항목 표시를 요청했습니다", "Requested selected items in File Explorer")
        : action === 'context_menu' ? accepted ? t("Windows 메뉴의 명령을 전달했습니다 · Windows가 처리하며 앱 작업 기록에 남지 않습니다", "Windows menu command sent · Handled by Windows; not recorded in the app log") : t("Windows 메뉴를 취소했습니다", "Windows menu cancelled")
        : action === 'properties' ? t("Windows 속성 창을 요청했습니다", "Requested Windows properties")
        : action === 'clipboard' ? t("Windows에 복사했습니다 · 탐색기에서 Ctrl+V", "Copied to Windows · Press Ctrl+V in File Explorer")
        : accepted ? t("외부 앱에 복사 요청을 전달했습니다", "Copy request sent to the external app") : t("외부 끌기를 취소했습니다", "External drag cancelled"));
    }
    catch (error) { if (epoch === generation.current) setError(() => action === 'absolute_path' || action === 'relative_path'
      ? t("경로를 클립보드에 복사하지 못했습니다. 선택 항목과 현재 패널의 기준 폴더를 확인하고 다시 시도하세요.", "Paths could not be copied to the clipboard. Check the selection and the current panel’s base folder, then try again.")
      : action === 'context_menu'
      ? t('Windows 메뉴를 처리하지 못했습니다. 명령이 실행됐는지 확인한 뒤, 기본 메뉴 또는 ‘탐색기에서 선택 항목 보기’를 사용하세요. {error}', 'The Windows menu could not be processed. Check whether the command ran, then use the standard menu or Show selected items in File Explorer. {error}', { error: operationError(error) })
      : operationError(error)); }
    finally { pending.current = false; setBusy(false); setStage(null); }
  };
  const importNative = async (root: string, paths: readonly string[] | null) => {
    if (pending.current) { setError(() => t('현재 {stage} 처리 중입니다. 완료 후 다시 시도하거나 항목 수집을 취소하세요.', '{stage} is in progress. Try again when it finishes, or cancel item collection.', { stage: stageLabel(stage) })); return; }
    pending.current = true; dispatchUncertain.current = false; setBusy(true); setError(null); setNotice(null); setClipboard(null);
    const epoch = generation.current;
    cutSequence.current = null;
    try {
      setStage(paths === null ? 'Windows 클립보드 읽기' : '끌어온 항목 수집');
      const result = paths === null ? await captureCopyClipboard() : await stageCopy(paths, () => epoch === generation.current);
      let retained = false;
      try {
        if (epoch === generation.current) {
          copyCount.current = result.count;
          retained = !await startCopy(result.id, root);
          if (retained) { if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {}); copyCapture.current = result.id; }
        }
      } finally { if (!retained) await discardCopyCapture(result.id).catch(() => {}); }
    } catch (error) {
      if (epoch === generation.current) setError(paths === null && stageRef.current === 'Windows 클립보드 읽기' && toFaultCode(error) === 'invalid_request'
        ? () => t("Windows 클립보드 읽기: 외부 잘라내기는 지원하지 않으므로 복사를 사용하세요. 파일을 선택하고 Ctrl+C로 다시 복사하세요.", "Reading Windows clipboard: External cut is not supported. Select the files and copy them again with Ctrl+C.")
        : copyFailure(error));
    } finally { pending.current = false; setBusy(false); setStage(null); }
  };
  const pasteDefault = async (root: string) => {
    if (pending.current) { setError(() => t('현재 {stage} 처리 중입니다. 완료 후 다시 시도하거나 항목 수집을 취소하세요.', '{stage} is in progress. Try again when it finishes, or cancel item collection.', { stage: stageLabel(stage) })); return; }
    if (clipboard?.mode !== 'move') { await importNative(root, null); return; }
    pending.current = true; dispatchUncertain.current = false; setBusy(true); setError(null); setNotice(null);
    const epoch = generation.current;
    const captured = clipboard;
    try {
      const current = await clipboardSequence();
      if (epoch !== generation.current) return;
      if (cutSequence.current === null || cutSequence.current !== current) {
        cutSequence.current = null; setClipboard(null);
        setError(() => t("Windows 클립보드가 바뀌어 앱 잘라내기를 취소했습니다. Ctrl+V를 다시 누르면 Windows에서 붙여넣습니다.", "The Windows clipboard changed, so the in-app cut was cancelled. Press Ctrl+V again to paste from Windows."));
        return;
      }
      open({ kind: 'move', root, selection: null, transfer: captured, copyFallback: copyFallback.current });
    } catch (error) { if (epoch === generation.current) setError(copyFailure(error)); }
    finally { pending.current = false; setBusy(false); setStage(null); }
  };
  return {
    clipboard, busy, error: error?.() ?? null, notice: notice?.() ?? null, stage: stage === null ? null : stageLabel(stage), copyRevision,
    canCancelPreparing: busy && ['선택 항목 수집', '끌어온 항목 수집', 'Windows 클립보드 읽기'].includes(stage ?? ''),
    cancelPreparing: () => { generation.current++; setNotice(() => t("항목 수집 취소 요청. 이미 실행한 복사는 복사 작업에서 취소하세요.", "Requested cancellation of item collection. Cancel copies already running in Copy jobs.")); },
    clear: () => {
      if (pending.current || !clipboard) return false;
      if (copyCapture.current) void discardCopyCapture(copyCapture.current).catch(() => {}); copyCapture.current = null;
      setClipboard(null); cutSequence.current = null; setError(null); setNotice(() => t("앱 보관을 비웠습니다", "In-app clipboard cleared"));
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
      const draft = { kind: clipboard.mode, root, selection: null, transfer: clipboard, copyFallback: copyFallback.current };
      if (clipboard.mode === 'copy' && copyCapture.current) {
        pending.current = true; setBusy(true); setError(null);
        void startCopy(copyCapture.current, root).catch(error => setError(copyFailure(error)))
          .finally(() => { pending.current = false; setBusy(false); setStage(null); });
      } else if (clipboard.mode === 'copy') setError(() => t("복사 원본을 다시 선택한 뒤 복사하세요.", "Select the source items again before copying.")); else open(draft);
    },
    submitted: (draft: FileActionDraft) => {
      if (draft.kind === 'copy' || draft.kind === 'move') setCopyRevision(value => value + 1);
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

