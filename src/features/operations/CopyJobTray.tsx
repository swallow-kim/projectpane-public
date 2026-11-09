import { useEffect, useRef, useState } from 'react';
import { cancelCopyJob, copyJobActive, listCopyJobs, readCopyOutcomes, type CopyJob, type CopyOutcomePage } from './copy-job-client';
import { operationError } from './operation-client';
import { nativeResultReason } from './native-result';
import { operationStatusText } from '../../shared/operations';

const outcomeLabels = { changed: '복사 완료', skipped: '건너뜀', failed: '실패', unknown: '확인 필요' };

export function CopyJobTray({ revision, refresh }: { readonly revision: number; readonly refresh: () => void }) {
  const [jobs, setJobs] = useState<CopyJob[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [pollError, setPollError] = useState<string | null>(null);
  const actionRevision = useRef(0);
  const [page, setPage] = useState<{ id: string; offset: number; value: CopyOutcomePage } | null>(null);
  const [busy, setBusy] = useState(false);
  const pending = useRef(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const previous = useRef(new Map<string, string>());
  const refreshRef = useRef(refresh); refreshRef.current = refresh;
  useEffect(() => { if (expanded) dialog.current?.showModal(); else dialog.current?.close(); }, [expanded]);
  useEffect(() => {
    let disposed = false; let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const version = actionRevision.current;
        const next = await listCopyJobs();
        if (disposed) return;
        if (next.some(job => !copyJobActive(job) && previous.current.get(job.id) !== job.status)) refreshRef.current();
        previous.current = new Map(next.map(job => [job.id, job.status]));
        if (version === actionRevision.current && !pending.current) setJobs(next);
        setPollError(null);
        timer = setTimeout(() => void poll(), next.some(copyJobActive) ? 500 : 5000);
      } catch (error) { if (!disposed) { setPollError(`복사 작업 상태 조회: ${operationError(error)}`); timer = setTimeout(() => void poll(), 5000); } }
    };
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, [revision]);
  const load = async (id: string, offset: number) => {
    if (pending.current) return; pending.current = true; setBusy(true);
    try { const value = await readCopyOutcomes(id, offset); setPage({ id, offset, value }); setError(null); }
    catch (error) { setError(`복사 결과 조회: ${operationError(error)}`); }
    finally { pending.current = false; setBusy(false); }
  };
  const cancel = async (id: string) => {
    if (pending.current) return; pending.current = true; setBusy(true); actionRevision.current++;
    try { const next = await cancelCopyJob(id); actionRevision.current++; setJobs(values => values.map(job => job.id === id ? next : job)); setError(null); }
    catch (error) { setError(`복사 취소 요청: ${operationError(error)} 상태를 확인한 뒤 다시 시도하세요.`); }
    finally { pending.current = false; setBusy(false); }
  };
  if (!jobs.length && !error && !pollError) return null;
  return <section className="operation-tray" aria-label="복사 작업" onKeyDown={event => event.stopPropagation()}>
    <button onClick={() => setExpanded(true)}>복사 작업 {jobs.length}개 · 진행 {jobs.filter(copyJobActive).length}{(error || pollError) && ' · 확인 필요'}</button>
    <dialog ref={dialog} className="operation-dialog operation-history" aria-label="복사 진행과 결과" onCancel={event => { event.preventDefault(); setExpanded(false); }}>
      <h2>복사 진행과 결과</h2>
      <p>취소해도 이미 복사한 항목은 유지됩니다. 실패·확인 필요 항목의 원본과 대상을 확인하세요. 자동 재시도나 덮어쓰기를 하지 않습니다.</p>
      {pollError && <p role="alert">{pollError}</p>}
      {error && <p role="alert">{error}</p>}
      <ol>{jobs.slice().reverse().map(job => <li key={job.id}>
        <strong>{job.status === 'failed' && job.counts.changed > 0 ? '일부 완료 · 실패' : operationStatusText[job.status]}</strong> 선택 항목 {job.rootsCompleted}/{job.rootsTotal}
        <span>처리 기록: 완료 {job.counts.changed} · 건너뜀 {job.counts.skipped} · 실패 {job.counts.failed} · 확인 필요 {job.counts.unknown}</span>
        {copyJobActive(job) && <progress aria-label="복사 진행" value={job.rootsCompleted} max={Math.max(1, job.rootsTotal)} />}
        {!job.journalSaved && <span>결과 저장 여부 확인 필요</span>}
        {job.failure && <span>{operationError(job.failure)}</span>}
        {copyJobActive(job) && <button disabled={busy || job.status === 'cancel_requested'} onClick={() => void cancel(job.id)}>복사 취소</button>}
        <button disabled={busy} onClick={() => void load(job.id, 0)}>복사 결과 보기</button>
      </li>)}</ol>
      {page && <section aria-label="복사 결과 페이지">{page.value.failure && <p role="alert">복사 결과 기록이 불완전합니다. 확인 가능한 기록만 표시합니다. 원본과 대상 파일을 확인하세요. 자동으로 다시 복사하지 않습니다.</p>}<ol>{page.value.items.map((item, index) => <li key={index}>{outcomeLabels[item.status]}: {item.source}<div>→ {item.destination}</div>{item.status === 'skipped' ? <div>기존 항목을 보존하고 건너뛰었습니다.</div> : item.status !== 'changed' && <div>{nativeResultReason(item.hresult)}</div>}</li>)}</ol>
        <button disabled={busy || page.offset === 0} onClick={() => void load(page.id, 0)}>처음 결과</button>
        <button disabled={busy || page.value.done} onClick={() => void load(page.id, page.value.nextOffset)}>{!page.value.done && page.value.nextOffset === page.offset ? '복사 결과 새로 고침' : '다음 복사 결과'}</button>
      </section>}
      <button onClick={() => setExpanded(false)}>닫기</button>
    </dialog>
  </section>;
}
