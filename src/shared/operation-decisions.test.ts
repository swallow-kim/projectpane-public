import { describe, expect, it } from 'vitest';
import { operationPreviewSchema, operationReportSchema } from './operations';

const decision = { source: 'C:\\source\\keep.txt', destination: 'D:\\target\\keep.txt' };
const only = { counts: { changed: 0, skipped: 1, failed: 0, unknown: 0 },
  evidence: { performSucceeded: false, aborted: null, outcomesComplete: true, cancellationConfirmed: false },
  items: [], userSkipped: [decision], userDecisionsOnly: true };
const mixed = { counts: { changed: 1, skipped: 1, failed: 0, unknown: 0 },
  evidence: { performSucceeded: true, aborted: false, outcomesComplete: true, cancellationConfirmed: false },
  items: [{ source: 'C:\\source\\copy.txt', destination: 'D:\\target\\copy.txt', status: 'changed', hresult: 0 }], userSkipped: [decision] };

describe('explicit skip decision provenance', () => {
  it('accepts decisions without a fabricated HRESULT and without claiming native execution', () => {
    expect(operationReportSchema.parse(only)).toEqual(only);
    expect(operationReportSchema.parse(mixed)).toEqual(mixed);
  });
  it('retains incomplete native evidence alongside explicit user decisions', () => {
    const value = { ...mixed, counts: { ...mixed.counts, unknown: 1 }, evidence: { ...mixed.evidence, outcomesComplete: false } };
    expect(operationReportSchema.parse(value)).toEqual(value);
  });
  it.each([
    { ...only, userSkipped: [] },
    { ...only, userDecisionsOnly: undefined },
    { ...only, userDecisionsOnly: false },
    { ...only, counts: { ...only.counts, skipped: 2 } },
    { ...only, evidence: { ...only.evidence, performSucceeded: true } },
    { ...only, evidence: { ...only.evidence, aborted: false } },
    { ...only, evidence: { ...only.evidence, outcomesComplete: false } },
    { ...only, evidence: { ...only.evidence, cancellationConfirmed: true } },
    { ...only, failure: { code: 'busy' } },
    { ...mixed, userDecisionsOnly: true },
    { ...mixed, counts: { ...mixed.counts, skipped: 0 } },
    { ...mixed, userSkipped: [decision, decision], counts: { ...mixed.counts, skipped: 2 } },
    { ...mixed, userSkipped: [decision, { source: decision.source.toUpperCase(), destination: 'D:\\other' }], counts: { ...mixed.counts, skipped: 2 } },
    { ...mixed, userSkipped: [decision, { source: 'C:\\other', destination: decision.destination.toUpperCase() }], counts: { ...mixed.counts, skipped: 2 } },
    { ...mixed, userSkipped: [{ source: decision.source, destination: decision.source.toUpperCase() }] },
    { ...mixed, userSkipped: [{ ...decision, source: mixed.items[0]?.source.toUpperCase() }] },
    { ...mixed, userSkipped: [{ ...decision, hresult: 0 }] },
    { ...mixed, userSkipped: [{ ...decision, destination: null }] },
    { ...mixed, userSkipped: [{ ...decision, source: mixed.items[0]?.source }] },
    { ...mixed, userSkipped: [{ ...decision, source: 'C:\\source\\bad\0name' }] },
    { ...mixed, userSkipped: [{ ...decision, destination: 'D:\\target\\bad\0name' }] },
    { ...mixed, userSkipped: [{ ...decision, source: '한'.repeat(10923) }] },
    { ...mixed, userSkipped: [{ ...decision, destination: '한'.repeat(10923) }] },
  ])('rejects inconsistent or ambiguous provenance %#', value => {
    expect(operationReportSchema.safeParse(value).success).toBe(false);
  });
  it('bounds the combined native and decision rows', () => {
    const userSkipped = Array.from({ length: 2048 }, (_, n) => ({ source: `C:\\source\\${n}`, destination: `D:\\target\\${n}` }));
    expect(operationReportSchema.safeParse({ ...mixed, userSkipped, counts: { ...mixed.counts, skipped: 2048 } }).success).toBe(false);
  });
});

describe('skip preview disclosure', () => {
  const preview = { id: 'skip-1', projectId: null, kind: 'copy_skip_existing', sources: [decision.source], destination: 'D:\\target', skippedSources: [decision.source] };
  it('accepts every skipped source as part of the original source list', () => {
    expect(operationPreviewSchema.parse(preview)).toEqual(preview);
    expect(operationPreviewSchema.parse({ ...preview, skippedSources: [] }).skippedSources).toEqual([]);
  });
  it.each([
    { ...preview, skippedSources: undefined },
    { ...preview, skippedSources: ['C:\\other'] },
    { ...preview, skippedSources: [decision.source, decision.source] },
    { ...preview, kind: 'copy' },
    { ...preview, destination: null },
    { ...preview, sources: [] },
  ])('rejects incomplete or misleading disclosure %#', value => {
    expect(operationPreviewSchema.safeParse(value).success).toBe(false);
  });
});
