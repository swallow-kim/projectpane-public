import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

// Entire Tauri boundary is synthetic. Never reads or writes the host clipboard.
for (const total of [129, 1025]) test(`native copy stages ${total} selected roots in bounded packets`, async ({ page }) => {
  await installOperations(page, total);
  await page.addInitScript(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__'); const original = bridge.invoke;
    const calls: { command: string; args: any }[] = []; const captures = new Map<string, any>(); let sequence = 0;
    Object.assign(window, { copyCalls: calls });
    bridge.invoke = async (command: string, args: any) => {
      if (command === 'list_copy_jobs') return [];
      if (!['begin_copy_capture', 'append_copy_capture', 'seal_copy_capture', 'discard_copy_capture', 'export_copy_capture'].includes(command)) return original(command, args);
      calls.push({ command, args: structuredClone(args) }); const request = args?.request;
      if (command === 'begin_copy_capture') { const result = { id: `capture-${++sequence}`, count: 0, sealed: false }; captures.set(result.id, result); return { ...result }; }
      if (command === 'append_copy_capture') { const result = captures.get(request.id); result.count += request.selection.entryIds.length; return { ...result }; }
      if (command === 'seal_copy_capture') { const result = captures.get(request.id); result.sealed = true; return { ...result }; }
      if (command === 'discard_copy_capture') { captures.delete(request.id); return; }
      if (command === 'export_copy_capture') return true;
    };
  });
  await page.goto('/');
  const list = page.locator('[data-pane="0"] [role="listbox"]');
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await list.focus(); await list.press('Control+a');
  await expect(page.locator('[data-pane="0"] .selection-status')).toContainText(total.toLocaleString());
  await list.press('Control+c');
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'copyCalls').filter((c: any) => c.command === 'export_copy_capture').length)).toBe(1);
  const chunks = await page.evaluate(() => Reflect.get(window, 'copyCalls').filter((c: any) => c.command === 'append_copy_capture').map((c: any) => c.args.request.selection.entryIds));
  expect(chunks.flat()).toHaveLength(total); expect(chunks.every((chunk: string[]) => chunk.length <= 128)).toBe(true);
  expect(new Set(chunks.flat()).size).toBe(total);
});

test('cancelling clipboard staging never exports or retries the native clipboard', async ({ page }) => {
  await installOperations(page, 129);
  await page.addInitScript(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__'); const previous = bridge.invoke;
    const calls: string[] = []; let release = () => {};
    Object.assign(window, { copyCalls: calls, releaseCopyCapture: () => release() });
    bridge.invoke = async (command: string, args: any) => {
      if (command === 'list_copy_jobs') return [];
      if (!['begin_copy_capture', 'append_copy_capture', 'seal_copy_capture', 'discard_copy_capture', 'export_copy_capture'].includes(command)) return previous(command, args);
      calls.push(command);
      if (command === 'begin_copy_capture') return { id: 'capture', count: 0, sealed: false };
      if (command === 'append_copy_capture') {
        await new Promise<void>(resolve => { release = resolve; });
        return { id: 'capture', count: args.request.selection.entryIds.length, sealed: false };
      }
      if (command === 'export_copy_capture') return true;
    };
  });
  await page.goto('/');
  const list = page.locator('[data-pane="0"] [role="listbox"]');
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await list.focus(); await list.press('Control+a');
  await expect(page.locator('[data-pane="0"] .selection-status')).toContainText('129');
  await list.press('Control+c');
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'copyCalls').includes('append_copy_capture'))).toBe(true);
  await page.getByRole('button', { name: '항목 수집 취소', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'releaseCopyCapture')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'copyCalls').includes('discard_copy_capture'))).toBe(true);
  expect(await page.evaluate(() => Reflect.get(window, 'copyCalls'))).toEqual(['begin_copy_capture', 'append_copy_capture', 'discard_copy_capture']);
  await expect(page.getByRole('button', { name: '항목 수집 취소', exact: true })).toHaveCount(0);
});
