import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function preview(page: Page, count = 1) {
  await installOperations(page, count); await page.goto('/');
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.getByRole('option').first().click();
  if (count > 1) await list.press('Shift+End');
  await expect(page.locator('.selection-status').first()).toHaveText(`${count}개 선택`);
  await page.getByRole('button', { name: '패널 1 파일 이동', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true }).fill('D:\\대상');
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '실행', exact: true })).toBeVisible();
  return dialog;
}

test('revise discards move preview before confirming a new destination', async ({ page }, info) => {
  const dialog = await preview(page);
  await dialog.screenshot({ path: info.outputPath('revise-preview-1280.png') });
  await dialog.getByRole('button', { name: '조건 변경', exact: true }).click();
  const target = dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true });
  await expect(target).toBeFocused(); await expect(target).toHaveValue('D:\\대상');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Prepared').size)).toBe(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
  await target.fill('D:\\다른 대상');
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await expect(dialog).toContainText('D:\\다른 대상');
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  const calls = await page.evaluate(() => Reflect.get(window, 'p4Calls') as { command: string; args: { request: { id?: string } } }[]);
  const discard = calls.findIndex(call => call.command === 'discard_operation');
  const prepares = calls.flatMap((call, index) => call.command === 'prepare_operation' ? [index] : []);
  expect(prepares).toHaveLength(2); expect(discard).toBeGreaterThan(prepares[0]!); expect(discard).toBeLessThan(prepares[1]!);
  const executed = calls.find(call => call.command === 'execute_operation');
  expect(executed?.args.request.id).not.toBe(calls[discard]?.args.request.id);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
});

test('failed discard retains preview and retry resets pagination without submitting', async ({ page }, info) => {
  const dialog = await preview(page, 17);
  await page.setViewportSize({ width: 768, height: 900 });
  await dialog.getByRole('button', { name: '다음 원본', exact: true }).click();
  await page.evaluate(() => Reflect.set(window, 'p4FailDiscard', true));
  await dialog.getByRole('button', { name: '조건 변경', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('이전 확인을 취소하지 못했습니다');
  await expect(dialog).toContainText('선택-16.txt');
  await dialog.screenshot({ path: info.outputPath('revise-failure-768.png') });
  await page.evaluate(() => Reflect.set(window, 'p4FailDiscard', false));
  await dialog.getByRole('button', { name: '조건 변경', exact: true }).click();
  await expect(dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true })).toBeFocused();
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await expect(dialog).toContainText('선택-0.txt'); await expect(dialog).not.toContainText('선택-16.txt');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});

test('pending discard prevents execution and cannot reopen a dismissed dialog', async ({ page }, info) => {
  const dialog = await preview(page);
  await page.setViewportSize({ width: 375, height: 900 });
  await page.evaluate(() => Reflect.set(window, 'p4HoldDiscard', true));
  await dialog.getByRole('button', { name: '조건 변경', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '실행', exact: true })).toBeDisabled();
  await expect(dialog.getByRole('button', { name: '조건 변경', exact: true })).toBeDisabled();
  await dialog.screenshot({ path: info.outputPath('revise-pending-375.png') });
  await page.evaluate(() => Reflect.set(window, 'p4HoldDiscard', false));
  await dialog.press('Escape'); await expect(dialog).not.toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Prepared').size)).toBe(0);
  await expect(dialog).not.toBeVisible();
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});
