import { describe, expect, it } from 'vitest';
import { parseOperationState, operationStatusText, operationReportSchema } from './operations';

const row = { id: 'op-1', projectId: null, status: 'interrupted_unknown', counts: { changed: 0, skipped: 0, failed: 0, unknown: 1 }, journalSaved: false };
describe('file operation state boundary', () => {
  it('retains a known execution fault while reading old reports without a reason', () => {
    const old = { counts: { changed: 0, skipped: 0, failed: 0, unknown: 0 }, evidence: { performSucceeded: false, aborted: false, outcomesComplete: true, cancellationConfirmed: false }, items: [] };
    expect(operationReportSchema.safeParse(old).success).toBe(true);
    expect(operationReportSchema.safeParse({ ...old, failure: { code: 'stale_request' } }).success).toBe(true);
    expect(operationReportSchema.safeParse({ ...old, failure: { code: 'worker_unavailable' }, evidence: { ...old.evidence, aborted: null, outcomesComplete: false } }).success).toBe(true);
    for (const report of [
      { ...old, failure: { code: 'made_up' } },
      { ...old, failure: { code: 'stale_request', path: 'private' } },
      { ...old, failure: { code: 'stale_request' }, counts: { ...old.counts, changed: 1 } },
      { ...old, failure: { code: 'stale_request' }, evidence: { ...old.evidence, performSucceeded: true } },
      { ...old, failure: { code: 'stale_request' }, evidence: { ...old.evidence, cancellationConfirmed: true } },
    ]) expect(operationReportSchema.safeParse(report).success).toBe(false);
  });
  it('retains unknown and cancellation-requested states without success relabeling', () => {
    const state = parseOperationState({ operations: [row, { ...row, id: 'op-2', status: 'cancel_requested' }], closeState: 'blocked', available: false, progress: null });
    expect(state.operations.map(item => operationStatusText[item.status])).toEqual(['결과 확인 필요', '취소 요청됨']);
  });
  it('rejects duplicated identities, unsupported phases and malformed counts', () => {
    for (const state of [
      { operations: [row, row], closeState: 'open', available: true },
      { operations: [row], closeState: 'forced_exit', available: true },
      { operations: [{ ...row, counts: { ...row.counts, changed: -1 } }], closeState: 'open', available: true },
      { operations: Array.from({ length: 257 }, (_, i) => ({ ...row, id: `op-${i}` })), closeState: 'open', available: true },
    ]) expect(() => parseOperationState({ ...state, progress: null })).toThrow();
  });
  it('binds bounded progress estimates to a currently running operation only', () => {
    const state = { operations: [{ ...row, status: 'running' }], closeState: 'open', available: true };
    const progress = { operationId: row.id, total: 10, completed: 10 };
    expect(parseOperationState({ ...state, progress }).operations[0].status).toBe('running');
    for (const bad of [{ ...progress, operationId: 'other' }, { ...progress, completed: 11 }, { ...progress, total: 0 }, { ...progress, total: 4_294_967_296 }]) {
      expect(() => parseOperationState({ ...state, progress: bad })).toThrow();
    }
    expect(() => parseOperationState({ ...state, operations: [row], progress })).toThrow();
  });
});
