import { expect, type Page } from '@playwright/test';
import type { Entry, Identity } from '../../src/shared/contracts';
import { installOperations } from './p4-operation-fixture';

type RecycleControl = {
  holdPrepare: boolean; holdExecute: boolean; prepareError: string | null; executeError: string | null;
  mismatch: 'kind' | 'project' | 'destination' | 'count' | null;
  calls: { command: string; args: unknown }[];
  release: () => void;
};
declare global { interface Window { recycle: RecycleControl } }

export async function installRecycle(page: Page, options: { readonly count?: number; readonly kind?: Entry['kind'] } = {}) {
  await installOperations(page);
  await page.addInitScript(({ count, kind }) => {
    type Request = { id?: string; projectId?: string | null; action?: { kind: string; selection?: { identity: Identity; entryIds: string[] } } };
    type Args = { request?: Request; identity?: Identity; offset?: number | null };
    const bridge: { invoke: (command: string, args?: Args) => Promise<unknown> } = Reflect.get(window, '__TAURI_INTERNALS__');
    const previous = bridge.invoke;
    let release: (() => void) | undefined;
    window.recycle = { holdPrepare: false, holdExecute: false, prepareError: null, executeError: null, mismatch: null, calls: [], release: () => release?.() };
    bridge.invoke = async (command, args) => {
      window.recycle.calls.push({ command, args: structuredClone(args) });
      if (['start_listing', 'read_listing', 'set_listing_view'].includes(command)) {
        const offset = args?.offset ?? 0;
        return { identity: args?.identity, batch: { offset, entries: Array.from({ length: Math.min(64, count - offset) }, (_, i) => ({
          id: `entry-${offset + i}`, name: `파일-${offset + i}.txt`, kind, sizeBytes: 12, modifiedAtMs: 0,
        })), total: count, complete: true, capped: false, snapshotBytes: count * 128, capReason: null, revision: 0 } };
      }
      if (command === 'prepare_operation') {
        if (window.recycle.prepareError) throw { code: window.recycle.prepareError };
        const response = await previous(command, args);
        if (typeof response !== 'object' || !response) throw new Error('Expected native preview');
        if (window.recycle.holdPrepare) await new Promise<void>(resolve => { release = resolve; });
        const sources = args?.request?.action?.selection?.entryIds.map(id => `C:\\Native\\${id}-검증.txt`) ?? [];
        const mismatches = {
          kind: { kind: 'move' }, project: { projectId: 'foreign-project' }, destination: { destination: 'D:\\unsafe' }, count: { sources: [] },
        };
        const preview = { ...response, sources, ...(window.recycle.mismatch ? mismatches[window.recycle.mismatch] : {}) };
        if ('id' in response && typeof response.id === 'string') {
          const prepared: Map<string, unknown> = Reflect.get(window, 'p4Prepared');
          prepared.set(response.id, preview);
        }
        return preview;
      }
      if (command === 'execute_operation') {
        if (window.recycle.executeError) throw { code: window.recycle.executeError };
        const response = await previous(command, args);
        if (window.recycle.holdExecute) await new Promise<void>(resolve => { release = resolve; });
        return response;
      }
      return previous(command, args);
    };
  }, { count: options.count ?? 1, kind: options.kind ?? 'file' });
}

export const recycleButton = (page: Page) => page.getByRole('button', { name: '패널 1 휴지통', exact: true });
export const recycleDialog = (page: Page) => page.getByRole('dialog', { name: '휴지통', exact: true });
export const confirmRecycle = (page: Page) => recycleDialog(page).getByRole('button', { name: '휴지통으로 보내기', exact: true });
export const recycleCallCount = (page: Page, command: string) => page.evaluate(command => window.recycle.calls.filter(call => call.command === command).length, command);
export async function openRecycle(page: Page) {
  await page.goto('/');
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  await recycleButton(page).click();
  await expect(recycleDialog(page)).toBeVisible();
}
