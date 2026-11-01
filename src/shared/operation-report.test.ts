import { describe, expect, it } from 'vitest';
import { operationReportSchema, type OperationReport } from './operations';

const statuses = ['changed', 'skipped', 'failed', 'unknown'] as const;
type Status = typeof statuses[number];
function reportFor(status: Status): OperationReport {
  return {
    counts: { changed: 0, skipped: 0, failed: 0, unknown: 0, [status]: 1 },
    evidence: { performSucceeded: status === 'changed', aborted: false, outcomesComplete: true, cancellationConfirmed: false },
    items: [{ source: 'C:\\fixture\\source.txt', destination: null, status, hresult: status === 'failed' ? -2147024891 : 0 }],
  };
}

describe('native operation report integrity', () => {
  for (const field of ['source', 'destination'] as const) {
    it.each(['bad\0name', 'a'.repeat(32769), '한'.repeat(10923)])(`rejects native ${field} containing NUL or exceeding 32768 UTF-8 bytes %#`, value => {
      const report = reportFor('changed');
      expect(operationReportSchema.safeParse({ ...report, items: report.items.map(item => ({ ...item, [field]: value })) }).success).toBe(false);
    });
    it.each(['a'.repeat(32768), `${'한'.repeat(10922)}ab`, '😀'.repeat(8192)])(`preserves native ${field} at the exact UTF-8 byte boundary %#`, value => {
      const report = reportFor('changed');
      const expected = { ...report, items: report.items.map(item => ({ ...item, [field]: value })) };
      expect(operationReportSchema.parse(expected)).toEqual(expected);
    });
  }
  it.each(statuses)('accepts exact complete %s evidence', status => {
    expect(operationReportSchema.safeParse(reportFor(status)).success).toBe(true);
  });
  it.each(statuses)('rejects missing or excess complete %s totals', status => {
    const report = reportFor(status);
    for (const count of [0, 2]) expect(operationReportSchema.safeParse({ ...report, counts: { ...report.counts, [status]: count } }).success).toBe(false);
  });
  it.each(statuses)('rejects observed %s items missing from an incomplete total', status => {
    const report = reportFor(status);
    expect(operationReportSchema.safeParse({ ...report, counts: { ...report.counts, [status]: 0 }, evidence: { ...report.evidence, outcomesComplete: false } }).success).toBe(false);
  });
  it('retains an unobserved tail without inventing item rows or success', () => {
    const report = reportFor('changed');
    const incomplete = { ...report, counts: { ...report.counts, unknown: 2 }, evidence: { ...report.evidence, performSucceeded: false, outcomesComplete: false } };
    expect(operationReportSchema.parse(incomplete)).toEqual(incomplete);
    expect(operationReportSchema.safeParse({ ...incomplete, evidence: { ...incomplete.evidence, outcomesComplete: true } }).success).toBe(false);
  });
  it.each(statuses)('preserves the native allowance for aggregate %s counts above incomplete rows', status => {
    const report = reportFor(status);
    expect(operationReportSchema.safeParse({ ...report, counts: { ...report.counts, [status]: 2 }, evidence: { ...report.evidence, outcomesComplete: false } }).success).toBe(true);
  });
  it('rejects equal grand totals assigned to a different status', () => {
    const report = reportFor('changed');
    for (const outcomesComplete of [true, false]) expect(operationReportSchema.safeParse({ ...report, counts: { changed: 0, skipped: 1, failed: 0, unknown: 0 }, evidence: { ...report.evidence, outcomesComplete } }).success).toBe(false);
  });
  it('accepts positive native success without manufacturing a different status', () => {
    const report = reportFor('changed');
    expect(operationReportSchema.safeParse({ ...report, items: report.items.map(item => ({ ...item, hresult: 0x00270008 })) }).success).toBe(true);
  });
  it('preserves zero-item cancellation and unavailable-worker evidence', () => {
    const empty = { counts: { changed: 0, skipped: 0, failed: 0, unknown: 0 }, items: [] };
    expect(operationReportSchema.safeParse({ ...empty, evidence: { performSucceeded: false, aborted: true, outcomesComplete: true, cancellationConfirmed: true } }).success).toBe(true);
    expect(operationReportSchema.safeParse({ ...empty, failure: { code: 'worker_unavailable' }, evidence: { performSucceeded: false, aborted: null, outcomesComplete: false, cancellationConfirmed: false } }).success).toBe(true);
    const unknown = reportFor('unknown');
    expect(operationReportSchema.safeParse({ ...unknown, evidence: { ...unknown.evidence, cancellationConfirmed: true } }).success).toBe(true);
  });
  it.each([['changed', -2147024891], ['failed', 0], ['failed', 1]] as const)('rejects %s with contradictory HRESULT %i', (status, hresult) => {
    const report = reportFor(status);
    expect(operationReportSchema.safeParse({ ...report, items: report.items.map(item => ({ ...item, hresult })) }).success).toBe(false);
  });
  it.each(['skipped', 'unknown'] as const)('retains both HRESULT signs for native %s outcomes', status => {
    const report = reportFor(status);
    for (const hresult of [-2147024891, 0, 1]) expect(operationReportSchema.safeParse({ ...report, items: report.items.map(item => ({ ...item, hresult })) }).success).toBe(true);
  });
});
