import { useEffect, useId, useRef, useState } from 'react';
import type { FileActionDraft, OperationPreview } from '../../shared/operations';
import { discardFileAction, executeFileAction, operationError, prepareFileAction } from './operation-client';

export function RecycleDialog({ draft, projectId, close, submitted }: {
  readonly draft: FileActionDraft; readonly projectId: string | null; readonly close: () => void; readonly submitted: () => void;
}) {
  const [preview, setPreview] = useState<OperationPreview | null>(null);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const prepared = useRef<string | null>(null);
  const sending = useRef(false);
  const composing = useRef(false);
  const settling = useRef(false);
  const id = useId();
  useEffect(() => {
    let disposed = false;
    dialog.current?.showModal(); cancel.current?.focus();
    void prepareFileAction(draft, '', projectId).then(async result => {
      if (disposed) { await discardFileAction(result.id); return; }
      prepared.current = result.id; setPreview(result);
    }).catch(error => { if (!disposed) setError(operationError(error, 'recycle')); })
      .finally(() => { if (!disposed) setBusy(false); });
    return () => {
      disposed = true;
      if (prepared.current && !sending.current) void discardFileAction(prepared.current).catch(() => {
        // The dialog is unmounted; discard failure cannot be displayed here or execute the preview.
      });
    };
  }, [draft, projectId]);
  const dismiss = () => { if (!sending.current && !composing.current && !settling.current) close(); };
  const execute = async () => {
    if (!preview || busy || sending.current || composing.current || settling.current) return;
    sending.current = true; setBusy(true); setError(null);
    try {
      await executeFileAction(preview.id);
      prepared.current = null; submitted(); close();
    } catch (error) { setError(operationError(error, 'recycle')); }
    finally { sending.current = false; setBusy(false); }
  };
  return <dialog ref={dialog} className="operation-dialog" aria-labelledby={`${id}-title`} aria-describedby={`${id}-scope`}
    onCancel={event => { event.preventDefault(); dismiss(); }}
    onKeyDown={event => {
      event.stopPropagation();
      const guarded = composing.current || settling.current || event.repeat || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229;
      if ((['Enter', 'Escape', ' '].includes(event.key) && guarded)
        || (event.key === 'Enter' && !(event.target instanceof HTMLButtonElement))) event.preventDefault();
    }}
    onKeyUp={() => { if (!composing.current) settling.current = false; }}
    onPointerDown={() => { if (!composing.current) settling.current = false; }}
    onCompositionStart={() => { composing.current = true; }}
    onCompositionEnd={() => { composing.current = false; settling.current = true; }}>
    <h2 id={`${id}-title`}>휴지통</h2>
    <p id={`${id}-scope`}>고정 드라이브의 파일과 폴더를 휴지통으로 보냅니다. 폴더 안의 항목도 함께 이동합니다. 링크·이동식 및 네트워크 드라이브는 지원하지 않으며, 휴지통을 사용할 수 없으면 삭제하지 않습니다.</p>
    <p>한 번에 최대 1,024개를 선택할 수 있습니다. 경로 길이와 결과 기록 용량에 따라 더 적은 수에서 제한될 수 있습니다.</p>
    <p>확인부터 작업 완료까지 다른 앱에서 원본 폴더와 내용을 변경하지 마세요.</p>
    {preview ? <>
      <p>아래 원본을 확인한 뒤 휴지통으로 보내세요.</p>
      <dl className="operation-paths" tabIndex={0} aria-label="휴지통 원본 목록"><dt>원본 {preview.sources.length}개</dt>
        <dd>{preview.sources.map(source => <div key={source}>{source}</div>)}</dd></dl>
    </> : busy && <p role="status">원본 확인 중…</p>}
    {error && <p role="alert">{error}</p>}
    <div className="operation-dialog__actions">
      <button type="button" disabled={busy || !preview} onClick={() => void execute()}>휴지통으로 보내기</button>
      <button ref={cancel} type="button" disabled={sending.current} onClick={dismiss}>취소</button>
    </div>
  </dialog>;
}
