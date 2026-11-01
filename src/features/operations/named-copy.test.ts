import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { prepareFileAction } from './operation-client';
import { operationSelectionSchema, type FileActionDraft } from '../../shared/operations';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);
const draft: FileActionDraft = { kind: 'copy', root: 'C:\\source', selection: operationSelectionSchema.parse({ identity: { pane: 0, tabId: 'tab-1', requestId: 'request-1', generation: 1 }, entryIds: ['entry-1'] }) };
const preview = { id: 'copy-as-1', projectId: null, kind: 'copy_as', sources: ['C:\\source\\original.txt'], destination: 'D:\\target\\한글 새 이름.txt' };
beforeEach(() => invokeMock.mockReset());

describe('explicit single-file copy name boundary', () => {
  it('sends an opaque selection and exact name without executing during preparation', async () => {
    invokeMock.mockResolvedValue(preview);
    const result = await prepareFileAction(draft, 'D:\\target', null, '한글 새 이름.txt');
    expect(result).toEqual(preview);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('prepare_operation', { request: { projectId: null, action: { kind: 'copy_as', selection: draft.selection, destination: 'D:\\target', name: '한글 새 이름.txt' } } });
  });
  it('rejects multiple entries and cut drafts before invoking native operations', async () => {
    const selection = draft.selection;
    if (!selection) throw new Error('fixture selection missing');
    for (const invalid of [{ ...draft, selection: operationSelectionSchema.parse({ ...selection, entryIds: ['one', 'two'] }) }, { ...draft, transfer: { token: 'transfer-1', mode: 'move' as const, count: 1 } }]) {
      await expect(prepareFileAction(invalid, 'D:\\target', null, 'renamed.txt')).rejects.toMatchObject({ code: 'invalid_request' });
    }
    expect(invokeMock).not.toHaveBeenCalled();
  });
  it('uses an immutable copy token for collision resolution without recapturing the clipboard', async () => {
    invokeMock.mockResolvedValue({ ...preview, destination: 'D:\\target\\new.txt' });
    await prepareFileAction({ ...draft, selection: null, transfer: { token: 'transfer-1', mode: 'copy', count: 1 } }, 'D:\\target', null, 'new.txt');
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith('prepare_transfer', { request: { projectId: null, token: 'transfer-1', destination: 'D:\\target', copyName: 'new.txt' } });
  });
  it('discards a preview that changes the approved target name', async () => {
    invokeMock.mockResolvedValueOnce({ ...preview, destination: 'D:\\target\\different.txt' }).mockResolvedValueOnce(undefined);
    await expect(prepareFileAction(draft, 'D:\\target', null, '한글 새 이름.txt')).rejects.toMatchObject({ code: 'protocol_error' });
    expect(invokeMock).toHaveBeenLastCalledWith('discard_operation', { request: { id: preview.id } });
  });
  it('accepts the native canonical parent for a short directory alias', async () => {
    invokeMock.mockResolvedValue({ ...preview, destination: 'D:\\Long Target Folder\\한글 새 이름.txt' });
    await expect(prepareFileAction(draft, 'D:\\LONGTA~1', null, '한글 새 이름.txt')).resolves.toMatchObject({ destination: 'D:\\Long Target Folder\\한글 새 이름.txt' });
  });
  it.each([['Approved.txt', 'approved.txt'], ['K.txt', 'k.txt']])('discards a folded leaf mismatch for %s', async (requested, returned) => {
    invokeMock.mockResolvedValueOnce({ ...preview, destination: `D:\\target\\${returned}` }).mockResolvedValueOnce(undefined);
    await expect(prepareFileAction(draft, 'D:\\target', null, requested)).rejects.toMatchObject({ code: 'protocol_error' });
    expect(invokeMock).toHaveBeenLastCalledWith('discard_operation', { request: { id: preview.id } });
  });
  it.each(['', '..\\escape.txt', '../escape.txt', 'CON .txt', 'name.', 'nul.txt', 'a'.repeat(256)])('rejects invalid leaf %s without invoking native operations', async name => {
    await expect(prepareFileAction(draft, 'D:\\target', null, name)).rejects.toMatchObject({ code: 'invalid_path' });
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
