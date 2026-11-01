import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test('recycle prepares opaque selection and cancellation never executes', async ({ page }, info) => {
  // Given a selected native regular file.
  await installOperations(page);
  await page.goto('/');
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  // When the explicit recycle action is opened, only a native preview is prepared.
  await page.getByRole('button', { name: '패널 1 휴지통', exact: true }).click({ timeout: 5000 });
  const dialog = page.getByRole('dialog', { name: '휴지통', exact: true });
  await expect(dialog).toContainText('C:\\Temporary\\한글.txt');
  await expect(dialog.getByRole('textbox')).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: '휴지통으로 보내기', exact: true })).toBeEnabled();
  await page.screenshot({ path: info.outputPath('recycle-preview.png') });
  const call = await page.evaluate(() => Reflect.get(window, 'p4Calls').find((item: { command: string }) => item.command === 'prepare_operation'));
  expect(call.args.request.action).toEqual({ kind: 'recycle', selection: { identity: expect.objectContaining({ pane: 0, tabId: expect.any(String), requestId: expect.any(String), generation: expect.any(Number) }), entryIds: ['entry-0'] } });
  // Then cancellation discards the preview and never submits the operation.
  await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'discard_operation').length)).toBe(1);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'execute_operation'))).toEqual([]);
});
