import { describe, expect, it, vi, afterEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import type { OperationSelection } from '../../shared/operations';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
afterEach(() => vi.resetAllMocks());
import { copyChunks, stageCopy, readCopyOutcomes } from './copy-job-client';
import { nativeDrop } from './native-drop';
describe('bounded large copy packets', () => {
  it.each([129, 1025, 10001])('retains all %i roots without imposing an operation cap', count => {
    const values = Array.from({ length: count }, (_, i) => `entry-${i}`);
    const chunks = copyChunks(values, chunk => ({ paths: chunk }));
    expect(chunks.flat()).toEqual(values);
    expect(chunks.every(chunk => chunk.length <= 128)).toBe(true);
  });
  it('also bounds encoded bytes for long Unicode paths', () => {
    const values = Array.from({ length: 129 }, (_, i) => `\\server\\share\\${'한'.repeat(5000)}-${i}`);
    const chunks = copyChunks(values, chunk => ({ paths: chunk }));
    expect(chunks.flat()).toEqual(values);
    expect(chunks.every(chunk => new TextEncoder().encode(JSON.stringify({ paths: chunk })).length < 60 * 1024)).toBe(true);
  });
  it('accepts native drop lists above 1024 without falling back to the clipboard', () => {
    const paths = Array.from({ length: 1025 }, (_, i) => `C:\\source\\${i}`);
    expect(nativeDrop({ position: { x: 1, y: 2 }, paths }, 1, true)?.paths).toEqual(paths);
  });
});

describe('copy capture lifecycle', () => {
  const selection = (count: number) => ({ identity: { pane: 0, tabId: 'tab', requestId: 'request', generation: 1 }, entryIds: Array.from({ length: count }, (_, i) => `entry-${i}`) }) as OperationSelection;
  it('cancels between bounded appends, discards and never seals or starts', async () => {
    let current = true;
    vi.mocked(invoke).mockImplementation(async (command, args: any) => {
      if (command === 'begin_copy_capture') return { id: 'capture', count: 0, sealed: false };
      if (command === 'append_copy_capture') { current = false; return { id: 'capture', count: args.request.selection.entryIds.length, sealed: false }; }
    });
    await expect(stageCopy(selection(1025), () => current)).rejects.toMatchObject({ code: 'cancelled' });
    expect(vi.mocked(invoke).mock.calls.map(call => call[0])).toEqual(['begin_copy_capture', 'append_copy_capture', 'discard_copy_capture']);
  });
  it('rejects mismatched append acknowledgment and discards staging', async () => {
    vi.mocked(invoke).mockImplementation(async command => {
      if (command === 'begin_copy_capture') return { id: 'capture', count: 0, sealed: false };
      if (command === 'append_copy_capture') return { id: 'other', count: 128, sealed: false };
    });
    await expect(stageCopy(selection(129))).rejects.toMatchObject({ code: 'protocol_error' });
    expect(vi.mocked(invoke).mock.calls.at(-1)).toEqual(['discard_copy_capture', { request: { id: 'capture' } }]);
  });
});

describe('copy outcome paging', () => {
  it('accepts a running empty page at the unchanged cursor', async () => {
    vi.mocked(invoke).mockResolvedValue({ items: [], nextOffset: 0, done: false });
    await expect(readCopyOutcomes('job', 0)).resolves.toEqual({ items: [], nextOffset: 0, done: false });
  });
  it('preserves available outcomes with a terminal corruption warning', async () => {
    const value = { items: [{ source: 'C:\\source', destination: 'D:\\target', status: 'unknown', hresult: -1 }], nextOffset: 100, done: true, failure: { code: 'invalid_state' } };
    vi.mocked(invoke).mockResolvedValue(value);
    await expect(readCopyOutcomes('job', 0)).resolves.toEqual(value);
  });
});
