import { useOperationMessage } from './use-operation-message';
import { t, useLanguage } from '../../shared/i18n';
import { useEffect, useRef, useState } from 'react';
import { cancelCopyJob, copyJobActive, listCopyJobs, readCopyOutcomes, type CopyJob, type CopyOutcomePage } from './copy-job-client';
import { operationError } from './operation-client';
import { nativeResultReason } from './native-result';
import { operationStatusText } from '../../shared/operations';


export function CopyJobTray({ revision, refresh }: { readonly revision: number; readonly refresh: () => void }) {
  useLanguage();
  const outcomeLabels = { changed: t("복사 완료", "Copied"), skipped: t("건너뜀", "Skipped"), failed: t("실패", "Failed"), unknown: t("확인 필요", "Needs review") };
  const [jobs, setJobs] = useState<CopyJob[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [error, setError] = useOperationMessage();
  const [pollError, setPollError] = useOperationMessage();
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
      } catch (error) { if (!disposed) { setPollError(() => t('복사 작업 상태 조회: {error}', 'Reading copy job status: {error}', { error: operationError(error) })); timer = setTimeout(() => void poll(), 5000); } }
    };
    void poll(); return () => { disposed = true; clearTimeout(timer); };
  }, [revision]);
  const load = async (id: string, offset: number) => {
    if (pending.current) return; pending.current = true; setBusy(true);
    try { const value = await readCopyOutcomes(id, offset); setPage({ id, offset, value }); setError(null); }
    catch (error) { setError(() => t('복사 결과 조회: {error}', 'Reading copy results: {error}', { error: operationError(error) })); }
    finally { pending.current = false; setBusy(false); }
  };
  const cancel = async (id: string) => {
    if (pending.current) return; pending.current = true; setBusy(true); actionRevision.current++;
    try { const next = await cancelCopyJob(id); actionRevision.current++; setJobs(values => values.map(job => job.id === id ? next : job)); setError(null); }
    catch (error) { setError(() => t('복사 취소 요청: {error} 상태를 확인한 뒤 다시 시도하세요.', 'Requesting copy cancellation: {error} Check the status before trying again.', { error: operationError(error) })); }
    finally { pending.current = false; setBusy(false); }
  };
  if (!jobs.length && !error && !pollError) return null;
  return <section className="operation-tray" aria-label={t("복사 작업", "Copy jobs")} onKeyDown={event => event.stopPropagation()}>
    <button onClick={() => setExpanded(true)}>{t("복사 작업 ", "Copy jobs ")}{jobs.length}{t("개 · 진행 ", " · Active: ")}{jobs.filter(copyJobActive).length}{(error || pollError) && t(" · 확인 필요", " · Needs review")}</button>
    <dialog ref={dialog} className="operation-dialog operation-history" aria-label={t("복사 진행과 결과", "Copy progress and results")} onCancel={event => { event.preventDefault(); setExpanded(false); }}>
      <h2>{t("복사 진행과 결과", "Copy progress and results")}</h2>
      <p>{t("취소해도 이미 복사한 항목은 유지됩니다. 실패·확인 필요 항목의 원본과 대상을 확인하세요. 자동 재시도나 덮어쓰기를 하지 않습니다.", "Cancelling keeps items already copied. Check the sources and destinations of failed or uncertain items. Nothing is retried or overwritten automatically.")}</p>
      {pollError && <p role="alert">{pollError?.()}</p>}
      {error && <p role="alert">{error?.()}</p>}
      <ol>{jobs.slice().reverse().map(job => <li key={job.id}>
        <strong>{job.status === 'failed' && job.counts.changed > 0 ? t("일부 완료 · 실패", "Partly completed · Failed") : operationStatusText[job.status]}</strong> {t(" 선택 항목 ", " Selected items: ")}{job.rootsCompleted}/{job.rootsTotal}
        <span>{t("처리 기록: 완료 ", "Results: Copied ")}{job.counts.changed} {t(" · 건너뜀 ", " · Skipped ")}{job.counts.skipped} {t(" · 실패 ", " · Failed ")}{job.counts.failed} {t(" · 확인 필요 ", " · Needs review ")}{job.counts.unknown}</span>
        {copyJobActive(job) && <progress aria-label={t("복사 진행", "Copy progress")} value={job.rootsCompleted} max={Math.max(1, job.rootsTotal)} />}
        {!job.journalSaved && <span>{t("결과 저장 여부 확인 필요", "Result storage needs review")}</span>}
        {job.failure && <span>{operationError(job.failure)}</span>}
        {copyJobActive(job) && <button disabled={busy || job.status === 'cancel_requested'} onClick={() => void cancel(job.id)}>{t("복사 취소", "Cancel copy")}</button>}
        <button disabled={busy} onClick={() => void load(job.id, 0)}>{t("복사 결과 보기", "View copy results")}</button>
      </li>)}</ol>
      {page && <section aria-label={t("복사 결과 페이지", "Copy result page")}>{page.value.failure && <p role="alert">{t("복사 결과 기록이 불완전합니다. 확인 가능한 기록만 표시합니다. 원본과 대상 파일을 확인하세요. 자동으로 다시 복사하지 않습니다.", "The copy result log is incomplete. Only verified records are shown. Check the source and destination files. Copying will not restart automatically.")}</p>}<ol>{page.value.items.map((item, index) => <li key={index}>{outcomeLabels[item.status]}: {item.source}<div>→ {item.destination}</div>{item.status === 'skipped' ? <div>{t("기존 항목을 보존하고 건너뛰었습니다.", "Skipped; the existing item was preserved.")}</div> : item.status !== 'changed' && <div>{nativeResultReason(item.hresult)}</div>}</li>)}</ol>
        <button disabled={busy || page.offset === 0} onClick={() => void load(page.id, 0)}>{t("처음 결과", "First results")}</button>
        <button disabled={busy || page.value.done} onClick={() => void load(page.id, page.value.nextOffset)}>{!page.value.done && page.value.nextOffset === page.offset ? t("복사 결과 새로 고침", "Refresh copy results") : t("다음 복사 결과", "Next copy results")}</button>
      </section>}
      <button onClick={() => setExpanded(false)}>{t("닫기", "Close")}</button>
    </dialog>
  </section>;
}
