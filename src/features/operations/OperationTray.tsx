import { useEffect, useId, useRef, useState } from 'react';
import { toFaultCode } from '../../shared/fault';
import { operationStatusText, terminalOperation, removableOperation, type OperationReport, type OperationState } from '../../shared/operations';
import { cancelFileAction, loadFileActionReport, operationError } from './operation-client';
import { OperationReportDialog } from './OperationReportDialog';

export function OperationTray({ state, unavailable, refresh }: { readonly state: OperationState | null; readonly unavailable: boolean; readonly refresh: () => void }) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [report, setReport] = useState<{ id: string; value: OperationReport } | null>(null);
  const [page, setPage] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const contentId = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => { if (expanded) dialog.current?.showModal(); else dialog.current?.close(); }, [expanded]);
  const operations = state?.operations ?? [];
  const activeCount = operations.filter(item => !terminalOperation(item.status)).length;
  const attentionCount = operations.filter(item => item.counts.failed > 0 || item.counts.unknown > 0 || !item.journalSaved || item.status === 'failed' || item.status === 'interrupted_unknown').length;
  const lastPage = Math.max(0, Math.ceil(operations.length / 8) - 1);
  const currentPage = Math.min(page, lastPage);
  useEffect(() => { setPage(value => Math.min(value, lastPage)); }, [lastPage]);
  const run = async (id: string, cancel: boolean) => {
    if (busy) return;
    setBusy(true); setError(null);
    try {
      if (cancel) { await cancelFileAction(id); refresh(); }
      else { const result = await loadFileActionReport(id); if (result) setReport({ id, value: result }); else setError('완료 기록이 없어 실제 변경 여부를 확인할 수 없습니다. 원본과 대상을 확인하세요. 작업은 자동으로 다시 실행되지 않습니다.'); }
    } catch (error) {
      setError(!cancel && toFaultCode(error) === 'protocol_error'
        ? '결과 기록을 확인할 수 없어 표시하지 않았습니다. 원본과 대상 폴더를 확인하세요. 파일 작업은 자동으로 다시 실행되지 않습니다.'
        : operationError(error));
    }
    finally { setBusy(false); }
  };
  if (!unavailable && operations.length === 0) return null;
  return <section className="operation-tray" aria-label="파일 작업 기록" onKeyDown={event => event.stopPropagation()}>
    <span className="sr-only" aria-live={attentionCount || unavailable ? "assertive" : "polite"} aria-atomic="true">{activeCount ? `파일 작업 진행 ${activeCount}개. 작업 버튼에서 진행 상태와 취소를 확인하세요.` : attentionCount || unavailable ? "파일 작업 확인 필요. 작업 버튼에서 오류와 결과를 확인하세요." : "파일 작업 완료"}</span>
    <button type="button" className="operation-tray__toggle" aria-expanded={expanded} aria-controls={contentId} onClick={() => setExpanded(value => !value)}>
      파일 작업 {operations.length}개{activeCount > 0 && ` · 진행 ${activeCount}`}{attentionCount > 0 && ` · 확인 필요 ${attentionCount}`}{unavailable && ' · 서비스 확인 필요'}
    </button>
    <dialog ref={dialog} id={contentId} className="operation-dialog operation-history" aria-label="파일 작업 상태와 결과" onCancel={() => setExpanded(false)}>
      <h2>파일 작업 상태와 결과</h2>
      <p>중단된 작업은 자동으로 재실행하지 않습니다. 취소는 이미 완료된 변경을 되돌리지 않습니다.</p>
      {operations.length === 256 && <p role="status">기록 한도에 도달했습니다. 결과 보기에서 확인한 종료 기록을 정리하면 새 작업을 받을 수 있습니다. 결과 불명·저장 미확인 기록은 보존합니다.</p>}
      {unavailable && <p role="status">파일 작업 서비스를 사용할 수 없습니다. 폴더 조회는 계속할 수 있습니다. <button onClick={refresh}>작업 상태 다시 확인</button></p>}
      {error && <p role="alert">{error}</p>}
      <ol start={currentPage * 8 + 1}>{operations.slice().reverse().slice(currentPage * 8, (currentPage + 1) * 8).map(item => <li key={item.id}>
        <span title={item.id}>{operationStatusText[item.status]}</span><span>변경 {item.counts.changed} · 실패 {item.counts.failed} · 불명 {item.counts.unknown}</span>
        {!item.journalSaved && <span>기록 저장 미확인</span>}
        {['running', 'cancel_requested'].includes(item.status) && <span className="operation-progress">
          {state?.progress?.operationId === item.id
            ? <><progress aria-label="파일 작업 진행 추정치" max={state.progress.total} value={state.progress.completed} /> 추정 {Math.floor(state.progress.completed / state.progress.total * 100)}%</>
            : <><progress aria-label="파일 작업 진행 추정치" /> 처리 중</>}
        </span>}
        {!terminalOperation(item.status) ? <button disabled={busy || item.status === 'cancel_requested'} onClick={() => void run(item.id, true)}>취소 요청</button>
          : <button disabled={busy} onClick={() => void run(item.id, false)}>결과 보기</button>}
      </li>)}</ol>
      {operations.length > 8 && <div><button disabled={currentPage === 0} onClick={() => setPage(currentPage - 1)}>이전 작업</button><button disabled={currentPage === lastPage} onClick={() => setPage(currentPage + 1)}>다음 작업</button></div>}
      <button onClick={() => setExpanded(false)}>닫기</button>
    </dialog>
    {report && <OperationReportDialog report={report.value} operationId={report.id}
      status={operations.find(item => item.id === report.id)?.status}
      removable={!unavailable && operations.some(item => item.id === report.id && removableOperation(item))}
      removed={() => { setReport(null); refresh(); }} close={() => setReport(null)} />}
  </section>;
}
