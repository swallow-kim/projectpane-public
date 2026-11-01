import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { prepareFileAction } from './operation-client';
import { operationSelectionSchema, type FileActionDraft } from '../../shared/operations';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const call = vi.mocked(invoke);
const draft: FileActionDraft = { kind: 'copy', root: 'C:\\source', selection: operationSelectionSchema.parse({ identity: { pane: 0, tabId: 'tab-1', requestId: 'request-1', generation: 1 }, entryIds: ['entry-1'] }) };
const preview = { id: 'skip-1', projectId: null, kind: 'copy_skip_existing', sources: ['C:\\source\\keep.txt'], destination: 'D:\\target', skippedSources: ['C:\\source\\keep.txt'] };
beforeEach(() => call.mockReset());

it('prepares the explicit policy using opaque selection without executing', async () => {
  call.mockResolvedValue(preview);
  await expect(prepareFileAction(draft, 'D:\\target', null, undefined, true)).resolves.toEqual(preview);
  expect(call).toHaveBeenCalledExactlyOnceWith('prepare_operation', { request: { projectId: null, action: { kind: 'copy_skip_existing', selection: draft.selection, destination: 'D:\\target' } } });
});
it('rejects named-copy, cut, non-copy and missing selection combinations before IPC', async () => {
  for (const invalid of [{ ...draft, kind: 'move' as const }, { ...draft, selection: null }, { ...draft, transfer: { token: 'transfer-1', mode: 'move' as const, count: 1 } }]) {
    await expect(prepareFileAction(invalid, 'D:\\target', null, undefined, true)).rejects.toMatchObject({ code: 'invalid_request' });
  }
  await expect(prepareFileAction(draft, 'D:\\target', null, 'named.txt', true)).rejects.toMatchObject({ code: 'invalid_request' });
  expect(call).not.toHaveBeenCalled();
});
it('supports explicit skip-existing for a captured copy without changing its source token', async () => {
  call.mockResolvedValue(preview);
  await prepareFileAction({ ...draft, selection: null, transfer: { token: 'transfer-1', mode: 'copy', count: 1 } }, 'D:\\target', null, undefined, true);
  expect(call).toHaveBeenCalledExactlyOnceWith('prepare_transfer', { request: { projectId: null, token: 'transfer-1', destination: 'D:\\target', skipExisting: true } });
});
it('discards a native preview that silently changes the chosen copy policy', async () => {
  call.mockResolvedValueOnce({ ...preview, kind: 'copy', skippedSources: undefined }).mockResolvedValueOnce(undefined);
  await expect(prepareFileAction(draft, 'D:\\target', null, undefined, true)).rejects.toMatchObject({ code: 'protocol_error' });
  expect(call).toHaveBeenLastCalledWith('discard_operation', { request: { id: 'skip-1' } });
});
