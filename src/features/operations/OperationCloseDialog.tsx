import { t, useLanguage } from '../../shared/i18n';
import { useEffect, useId, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { closeNoticeSchema, parseOperationState, type CloseChoice, type OperationCloseState } from '../../shared/operations';
import './operations.css';

const afterChoice = { wait: 'waiting', cancel: 'waiting', continue: 'open', exit_without_results: 'stopping' } as const satisfies Record<CloseChoice, OperationCloseState>;

export function OperationCloseDialog({ updating = false }: { readonly updating?: boolean }) {
  useLanguage();
  const [state, setState] = useState<OperationCloseState>('open');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const keep = useRef<HTMLButtonElement>(null);
  const revision = useRef(0);
  const id = useId();
  useEffect(() => {
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<unknown>('projectpane-operation-close', event => {
      const parsed = closeNoticeSchema.safeParse(event.payload);
      if (!disposed && parsed.success) { revision.current += 1; setState(parsed.data.closeState); }
    }, { target: { kind: 'WebviewWindow', label: 'main' } }).then(async unlisten => {
      if (disposed) { unlisten(); return; }
      stop = unlisten;
      const expected = revision.current;
      const result = parseOperationState(await invoke<unknown>('get_operation_state'));
      if (!disposed && revision.current === expected) setState(result.closeState);
    }).catch(() => { /* Unsupported operation service leaves browsing available. */ });
    return () => { disposed = true; stop?.(); };
  }, []);
  const visible = state !== 'open' && state !== 'stopped';
  useEffect(() => {
    if (visible && !dialog.current?.open) { dialog.current?.showModal(); keep.current?.focus(); }
    if (!visible && dialog.current?.open) dialog.current.close();
  }, [visible]);
  const choose = async (choice: CloseChoice) => {
    if (busy) return;
    const expected = revision.current;
    setBusy(true); setError(false);
    try {
      await invoke('operation_close_choice', { choice });
      if (revision.current === expected) {
        revision.current += 1;
        setState(afterChoice[choice]);
      }
    } catch { setError(true); }
    finally { setBusy(false); }
  };
  const deciding = state === 'deciding';
  return <dialog ref={dialog} className="operation-dialog" aria-labelledby={`${id}-title`} aria-describedby={`${id}-hint`}
    onKeyDown={event => event.stopPropagation()} onCancel={event => { event.preventDefault(); if (!busy && state !== 'stopping') void choose('continue'); }}>
    <h2 id={`${id}-title`}>{t("파일 작업을 마친 뒤 닫기", "Close after file operations finish")}</h2>
    <p id={`${id}-hint`}>{state === 'blocked'
      ? t("파일 작업 종료 또는 결과 저장을 확인하지 못했습니다. 창을 유지하고 작업 결과와 저장 상태를 확인하세요.", "Operation completion or result storage could not be confirmed. Keep the window open and check the results and storage status.")
      : state === 'results_unsaved' ? t("실행 중인 파일 작업은 없습니다. 결과 일부를 저장하지 못했습니다. 기록 없이 닫으면 다음 실행에서 실제 변경 여부를 확인해야 합니다. 남은 작업은 자동으로 실행하지 않습니다.", "No file operations are running, but some results could not be saved. If you close without records, check the actual changes next time you open the app. Remaining operations will not run automatically.")
      : deciding ? t("접수된 파일 작업이 남아 있습니다. 어떻게 진행할까요?", "Accepted file operations are still pending. How would you like to proceed?")
      : t("작업 종료를 기다리고 있습니다. 취소 요청 후에도 확인이 끝날 때까지 창을 유지합니다.", "Waiting for operations to stop. The window stays open until this is confirmed, even after a cancellation request.")}</p>
    {error && <p role="alert">{t("종료 선택을 적용하지 못했습니다. 작업 상태를 확인한 뒤 다시 시도하세요.", "Your close option could not be applied. Check the operation status and try again.")}</p>}
    <div className="operation-dialog__actions">
      {deciding && <button disabled={busy} onClick={() => void choose('wait')}>{t("완료까지 대기", "Wait until complete")}</button>}
      {(deciding || state === 'waiting') && <button disabled={busy} onClick={() => void choose('cancel')}>{t("취소 요청 후 닫기", "Cancel operations, then close")}</button>}
      {state === 'results_unsaved' && !updating && <button disabled={busy} onClick={() => void choose('exit_without_results')}>{t("기록 미저장 상태로 닫기", "Close without saving records")}</button>}
      <button ref={keep} disabled={busy || state === 'stopping'} onClick={() => void choose('continue')}>{t("창 유지", "Keep window open")}</button>
    </div>
  </dialog>;
}
