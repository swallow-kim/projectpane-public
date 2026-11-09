import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { faultMessage, nativeFaultSchema, toFaultCode } from '../../shared/fault';
import { operationError } from './operation-client';
import { copyJobSchema, readCopyOutcomes } from './copy-job-client';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('strict copy preservation diagnostics', () => {
  it('distinguishes unsupported destination ADS from unknown provider capabilities', () => {
    const fault = { code: 'destination_streams_unsupported' } as const;
    expect(nativeFaultSchema.parse(fault)).toEqual(fault);
    expect(toFaultCode(fault)).toBe(fault.code);
    expect(operationError(fault)).toBe(faultMessage(fault.code));
    expect(operationError(fault)).toContain('추가 데이터 스트림(ADS)');
    expect(operationError(fault)).toContain('ADS를 지원하는 대상 저장소를 선택하세요');
    expect(operationError(fault)).not.toBe(operationError({ code: 'unsupported_provider' }));
  });

  it('explains that a link may be an ancestor and never suggests automatic traversal', () => {
    const fault = { code: 'unsafe_reparse_point' } as const;
    expect(operationError(fault)).toBe(faultMessage(fault.code));
    expect(operationError(fault)).toContain('선택한 항목이나 상위 경로');
    expect(operationError(fault)).toContain('링크를 따라가지 않습니다');
    expect(operationError(fault)).toContain('실제 원본 경로와 대상 폴더를 직접 선택하세요');
  });

  it.each(['destination_streams_unsupported', 'unsafe_reparse_point'])('retains exact failed paths and partial results for %s', async code => {
    const job = { id: 'job', projectId: null, status: 'failed', counts: { changed: 1, skipped: 0, failed: 1, unknown: 0 }, rootsTotal: 1, rootsCompleted: 0, outcomeCount: 2, journalSaved: true, failure: { code } };
    expect(copyJobSchema.parse(job)).toEqual(job);
    const page = { items: [
      { source: 'C:\\source', destination: '\\\\wsl.localhost\\Ubuntu\\tmp\\copied', status: 'changed', hresult: 0 },
      { source: 'C:\\source\\실패한 항목', destination: '\\\\wsl.localhost\\Ubuntu\\tmp\\copied\\실패한 항목', status: 'failed', hresult: -2147467259 },
    ], nextOffset: 256, done: true };
    vi.mocked(invoke).mockResolvedValueOnce(page);
    await expect(readCopyOutcomes('job', 0)).resolves.toEqual(page);
  });
});
