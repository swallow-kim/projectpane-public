import type { Page } from '@playwright/test';
import { installProjects } from './p3-project-fixture';
import type { OperationPreview, OperationReport, OperationSnapshot, TransferPreview } from '../../src/shared/operations';

export async function installOperations(page: Page, entryCount = 1) {
  await installProjects(page, true, false, true, entryCount);
  await page.addInitScript(() => {
    type Action = { kind: OperationPreview['kind'] | 'archive'; extract?: boolean; parent?: string; name?: string; destination?: string; selection?: { entryIds: string[] } };
    type Request = { id?: string; projectId: string | null; action?: Action; token?: string; destination?: string; mode?: TransferPreview['mode']; selection?: { entryIds: string[] }; copyName?: string; skipExisting?: boolean };
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__') as { invoke: (command: string, args?: unknown) => Promise<unknown> };
    const previous = bridge.invoke;
    const calls: { command: string; args: unknown }[] = [];
    const prepared = new Map<string, OperationPreview>();
    const operations = new Map<string, OperationSnapshot>();
    const reports = new Map<string, OperationReport>();
    let next = 0;
    let transfer: TransferPreview | null = null;
    let cutPreview: string | null = null;
    let release: (() => void) | undefined;
    const counts = () => ({ changed: 0, skipped: 0, failed: 0, unknown: 0 });
    Object.assign(window, { p4Calls: calls, p4Operations: operations, p4Reports: reports, p4Prepared: prepared,
      p4Release: () => release?.(),
      p4Finish: () => { for (const item of operations.values()) {
        item.status = 'succeeded'; item.counts.changed = 1;
        const plan = prepared.get(item.id);
        reports.set(item.id, { counts: item.counts, evidence: { performSucceeded: true, aborted: false, outcomesComplete: true, cancellationConfirmed: false }, items: [{ source: plan?.sources[0] ?? 'C:\\Temporary', destination: plan?.destination ?? null, status: 'changed', hresult: 0 }] });
      } },
    });
    bridge.invoke = async (command, args) => {
      if (!['list_archive_tools', 'launch_archive_tool', 'get_operation_state', 'prepare_operation', 'capture_transfer', 'prepare_transfer', 'execute_operation', 'discard_operation', 'cancel_operation', 'get_operation_report', 'forget_operation', 'export_to_explorer', 'import_from_explorer', 'clipboard_sequence'].includes(command)) return previous(command, args);
      calls.push({ command, args: structuredClone(args) });
      const request = (args as { request?: Request } | undefined)?.request;
      if (command === 'list_archive_tools') return [{ id: 'seven_zip', label: '7-Zip' }, { id: 'bandizip', label: 'Bandizip' }];
      if (command === 'launch_archive_tool') { if (Reflect.get(window, 'p4ArchiveLaunchFails')) throw { code: 'not_found' }; return true; }
      if (command === 'clipboard_sequence') return Reflect.get(window, 'p4ClipboardSequence') ?? 1;
      if (command === 'export_to_explorer') {
        if (Reflect.get(window, 'p4ExportError')) throw { code: Reflect.get(window, 'p4ExportError') };
        return Reflect.get(window, 'p4ExportAccepted') ?? true;
      }
      if (command === 'import_from_explorer') {
        if (Reflect.get(window, 'p4RejectExternal')) throw { code: 'invalid_request' };
        const paths = (args as { paths: string[] | null }).paths;
        transfer = { token: `external-${++next}`, mode: 'copy', count: paths?.length ?? 1 };
        return transfer;
      }
      if (command === 'get_operation_state') return { operations: [...operations.values()], closeState: 'open', available: true, progress: Reflect.get(window, 'p4Progress') ?? null };
      if (command === 'capture_transfer') {
        if (!request?.mode || !request.selection) throw new Error('Expected capture selection');
        if (cutPreview) prepared.delete(cutPreview);
        cutPreview = null;
        transfer = { token: `transfer-${++next}`, mode: request.mode, count: request.selection.entryIds.length };
        if (Reflect.get(window, 'p4HoldCapture')) await new Promise<void>(resolve => { release = resolve; });
        return transfer;
      }
      if (command === 'prepare_transfer') {
        if (!transfer || transfer.token !== request?.token || Reflect.get(window, 'p4StaleTransfer')) throw { code: 'stale_request' };
        if (cutPreview) throw { code: 'busy' };
        if (!request.copyName && !request.skipExisting && Reflect.get(window, 'p4TransferError')) throw { code: Reflect.get(window, 'p4TransferError') };
        const preview: OperationPreview = { id: `operation-${++next}`, projectId: request.projectId, kind: request.copyName ? 'copy_as' : request.skipExisting ? 'copy_skip_existing' : transfer.mode,
          sources: Array.from({ length: transfer.count }, (_, i) => `C:\\Temporary\\captured-${i}.txt`), destination: request.copyName ? `${request.destination}\\${request.copyName}` : request.destination ?? null };
        if (request.skipExisting) preview.skippedSources = preview.sources;
        prepared.set(preview.id, preview);
        if (transfer.mode === 'move') cutPreview = preview.id;
        if (Reflect.get(window, 'p4HoldPrepare')) await new Promise<void>(resolve => { release = resolve; });
        return preview;
      }
      if (command === 'prepare_operation') {
        if (!request?.action) throw new Error('Expected prepare action');
        const action = request.action;
        if (Reflect.get(window, 'p4PrepareError')) throw { code: Reflect.get(window, 'p4PrepareError') };
        if (action.name === 'collision') throw { code: 'busy' };
        const id = `operation-${++next}`;
        const preview: OperationPreview = { id, projectId: request.projectId, kind: action.kind === 'archive' ? (action.extract ? 'extract' : 'compress') : action.kind,
          sources: action.kind === 'new_folder' ? [] : (action.kind === 'archive' || action.kind === 'copy_skip_existing' || action.kind === 'move') ? (action.selection?.entryIds ?? []).map((_, index) => `C:\\Temporary\\선택-${index}.txt`) : ['C:\\Temporary\\한글.txt'],
          destination: action.kind === 'new_folder' ? `${action.parent}\\${action.name}` : action.kind === 'rename' ? `C:\\Temporary\\${action.name}`
            : action.kind === 'copy_as' || action.kind === 'archive' ? `${action.destination}\\${Reflect.get(window, 'p4WrongCopyName') ? 'unexpected.txt' : action.name}` : action.destination ?? null };
        if (action.kind === 'copy_skip_existing') preview.skippedSources = preview.sources.slice(preview.sources.length - (Reflect.get(window, 'p4SkipCount') ?? preview.sources.length));
        prepared.set(id, preview);
        if (Reflect.get(window, 'p4HoldPrepare')) await new Promise<void>(resolve => { release = resolve; });
        return preview;
      }
      if (!request?.id) throw new Error('Expected operation ID');
      if (command === 'forget_operation') {
        const item = operations.get(request.id);
        if (item && (!item.journalSaved || !['succeeded', 'partially_succeeded', 'cancelled', 'failed'].includes(item.status))) throw { code: 'busy' };
        if (Reflect.get(window, 'p4FailForget')) throw { code: 'io_error' };
        if (Reflect.get(window, 'p4HoldForget')) await new Promise<void>(resolve => { release = resolve; });
        operations.delete(request.id); reports.delete(request.id); prepared.delete(request.id);
        return request.id;
      }
      if (command === 'discard_operation') {
        if (Reflect.get(window, 'p4FailDiscard')) throw { code: 'io_error' };
        if (Reflect.get(window, 'p4HoldDiscard')) await new Promise<void>(resolve => { release = resolve; });
        if (!operations.has(request.id)) prepared.delete(request.id);
        if (cutPreview === request.id) cutPreview = null;
        return;
      }
      if (command === 'get_operation_report') return reports.get(request.id) ?? null;
      if (command === 'execute_operation') {
        if (!prepared.has(request.id)) throw { code: 'stale_request' };
        if (!operations.has(request.id)) operations.set(request.id, { id: request.id, projectId: null, status: 'queued', counts: counts(), journalSaved: true });
        if (cutPreview === request.id) { transfer = null; cutPreview = null; }
        if (Reflect.get(window, 'p4LoseReply')) { Reflect.set(window, 'p4LoseReply', false); throw { code: 'io_error' }; }
        return operations.get(request.id);
      }
      const item = operations.get(request.id);
      if (!item) throw { code: 'stale_request' };
      item.status = 'cancel_requested';
      return item;
    };
  });
}
