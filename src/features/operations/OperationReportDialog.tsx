import { useEffect, useId, useRef, useState } from 'react';
import { operationStatusText, type OperationReport, type OperationSnapshot } from '../../shared/operations';
import { forgetFileAction, operationError } from './operation-client';
import { nativeResultCode, nativeResultReason } from './native-result';

export function OperationReportDialog({ report, status, operationId, removable, removed, close }: {
  readonly report: OperationReport; readonly status: OperationSnapshot['status'] | undefined; readonly operationId: string; readonly removable: boolean; readonly removed: () => void; readonly close: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const [page, setPage] = useState(0);
  const [confirm, setConfirm] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef(false);
  const composing = useRef(false);
  const settling = useRef(false);
  const id = useId();
  const uncertain = status === undefined || status === 'interrupted_unknown';
  useEffect(() => { dialog.current?.showModal(); }, []);
  const labels = { changed: '변경됨', skipped: '건너뜀', failed: '실패', unknown: '확인 필요' } as const;
  const entries = [...report.items.map(item => ({ origin: 'native' as const, item })), ...(report.userSkipped ?? []).map(item => ({ origin: 'user' as const, item }))];
  const forget = async () => {
    if (pending.current || !removable || !confirm || composing.current || settling.current) return;
    pending.current = true; setBusy(true); setError(null);
    try { await forgetFileAction(operationId); removed(); }
    catch { setError('기록을 정리하지 못했습니다. 상태를 다시 확인하거나 앱을 다시 시작하세요. 파일 작업은 자동으로 재실행하지 않습니다.'); }
    finally { pending.current = false; setBusy(false); }
  };
  return <dialog ref={dialog} className="operation-dialog operation-report" aria-labelledby={id}
    onKeyDown={event => { event.stopPropagation(); if ((event.key === 'Enter' || event.key === 'Escape') && (composing.current || settling.current || event.repeat || event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229)) event.preventDefault(); }}
    onKeyUp={() => { if (!composing.current) settling.current = false; }} onPointerDown={() => { if (!composing.current) settling.current = false; }}
    onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; settling.current = true; }}
    onCancel={event => { event.preventDefault(); if (!pending.current && !composing.current && !settling.current) { if (confirm) setConfirm(false); else close(); } }}>
    <h2 id={id}>파일 작업 결과</h2>
    <p>전체 작업: {status === undefined ? '상태 확인 필요' : operationStatusText[status]}</p>
    <p>변경 {report.counts.changed} · 건너뜀 {report.counts.skipped} · 실패 {report.counts.failed} · 불명 {report.counts.unknown}</p>
    {report.failure && <p>{operationError(report.failure)}</p>}
    {(uncertain || report.counts.failed > 0 || report.counts.unknown > 0 || report.evidence.cancellationConfirmed) && <div className="operation-recovery" aria-label="수동 확인 안내">
      <p>취소는 실행 취소(Undo)가 아닙니다. 아래 원본·대상 경로에서 실제 남은 파일을 확인하세요.</p>
      <p>이미 완료된 항목은 그대로 두고, 실패·확인 필요 항목의 내용과 크기를 비교한 뒤 필요한 항목만 새 작업으로 요청하세요. 이 화면을 닫거나 앱을 다시 시작해도 자동 재시도·삭제·원상복구하지 않습니다.</p>
    </div>}
    {report.userDecisionsOnly && <p>선택한 항목을 모두 건너뛰었습니다. 기존 파일을 유지했고 실제 복사는 실행하지 않았습니다.</p>}
    {!report.failure && !report.userDecisionsOnly && report.evidence.outcomesComplete && !report.evidence.performSucceeded && !report.evidence.cancellationConfirmed && report.items.length === 0 && <p>작업을 시작하지 못했습니다. 경로·권한·대상 이름을 확인하세요.</p>}
    {(uncertain || !report.evidence.outcomesComplete || report.counts.unknown > 0) && <p role="alert">{uncertain ? '작업 전체의 완료·취소 여부를 확인하지 못했습니다.' : '일부 결과를 확인하지 못했습니다.'} 원본과 대상 폴더를 확인하세요. 자동으로 재실행하지 않습니다.</p>}
    {report.evidence.cancellationConfirmed && report.counts.changed > 0 && <p>취소 신호가 있어도 이미 변경된 항목은 자동으로 되돌리지 않습니다.</p>}
    <ol className="operation-result-items" start={page * 16 + 1}>{entries.slice(page * 16, (page + 1) * 16).map((entry, index) => <li key={`${page}-${index}`}>
      <strong>{entry.origin === 'user' ? '건너뜀 · 사용자 선택' : labels[entry.item.status]}</strong><div>{entry.item.source}</div>{entry.item.destination && <div>→ {entry.item.destination}</div>}
      {entry.origin === 'user' ? <div>같은 이름의 기존 파일 유지</div> : entry.item.status !== 'changed' && <div>{nativeResultReason(entry.item.hresult)} <span>Windows 코드 {nativeResultCode(entry.item.hresult)}</span></div>}
    </li>)}</ol>
    {error && <p role="alert">{error}</p>}
    {confirm ? <><p>이 작업의 앱 기록만 영구히 지웁니다. 실제 파일은 그대로 유지됩니다. 작업을 되돌리는 기능은 아닙니다.</p>
      <div className="operation-dialog__actions"><button disabled={busy || !removable} onClick={() => void forget()}>{busy ? '기록 정리 중…' : '기록만 지우기'}</button><button disabled={busy} onClick={() => setConfirm(false)}>정리 취소</button></div></>
      : <div className="operation-dialog__actions"><button disabled={page === 0} onClick={() => setPage(value => value - 1)}>이전 결과</button><button disabled={(page + 1) * 16 >= entries.length} onClick={() => setPage(value => value + 1)}>다음 결과</button>
        <button disabled={!removable} title="종료·저장이 확인된 기록만 정리할 수 있습니다" onClick={() => setConfirm(true)}>기록 정리</button><button onClick={close}>닫기</button></div>}
  </dialog>;
}
