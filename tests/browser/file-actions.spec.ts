import { expandOperations } from './compact-ui-fixture';
import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test.beforeEach(async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
});

test('new folder previews exact paths before submit and displays completion results', async ({ page }, info) => {
  await page.getByRole('button', { name: '패널 1 새 폴더', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await dialog.getByRole('textbox', { name: '새 이름', exact: true }).fill('새 한글 폴더');
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await expect(dialog).toContainText('C:\\Temporary\\새 한글 폴더');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
  await page.screenshot({ path: info.outputPath('file-action-preview.png') });
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  await expect(tray).toContainText('대기 중');
  await page.evaluate(() => Reflect.get(window, 'p4Finish')());
  await expect(tray).toContainText('완료');
  await tray.getByRole('button', { name: '결과 보기' }).click();
  const report = page.getByRole('dialog', { name: '파일 작업 결과' });
  await expect(report).toContainText('변경 1');
  await expect(report).toContainText('새 한글 폴더');
});

test('selection is bound to opaque entries and cancelling pending copy preparation never submits', async ({ page }) => {
  const rename = page.getByRole('button', { name: '패널 1 이름 변경', exact: true });
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click({ modifiers: ['Control'] });
  await expect(rename).toBeDisabled();
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  await expect(rename).toBeEnabled();
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true }).fill('D:\\대상');
  await page.evaluate(() => Reflect.set(window, 'p4HoldPrepare', true));
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).toContainText('대상 확인 중');
  const call = await page.evaluate(() => Reflect.get(window, 'p4Calls').find((item: { command: string }) => item.command === 'prepare_operation'));
  expect(call.args.request.action.selection.entryIds).toEqual(['entry-0']);
  expect(call.args.request.action.selection.identity.tabId).toBeTruthy();
  await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'discard_operation').length)).toBe(1);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});

test('composition Enter cannot prepare or execute a file change', async ({ page }) => {
  await page.getByRole('button', { name: '패널 1 새 폴더', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '새 폴더', exact: true });
  const input = dialog.getByRole('textbox');
  await input.dispatchEvent('compositionstart');
  await input.fill('조합 이름');
  await input.press('Enter');
  await input.dispatchEvent('compositionend');
  await input.press('Enter');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'prepare_operation').length)).toBe(0);
  await dialog.getByRole('button', { name: '대상 확인' }).click();
  await expect(dialog).toContainText('C:\\Temporary\\조합 이름');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});

test('dismissed preparation discards its eventual result without executing', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4HoldPrepare', true));
  await page.getByRole('button', { name: '패널 1 새 폴더', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await dialog.getByRole('textbox').fill('abandoned');
  await dialog.getByRole('button', { name: '대상 확인' }).click();
  await expect(dialog).toContainText('대상 확인 중');
  await dialog.getByRole('button', { name: '취소' }).click();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'discard_operation').length)).toBe(1);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});

test('a lost submit response retains the immutable ID for an idempotent retry', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4LoseReply', true));
  await page.getByRole('button', { name: '패널 1 새 폴더', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await dialog.getByRole('textbox').fill('retained');
  await dialog.getByRole('button', { name: '대상 확인' }).click();
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const ids = await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'execute_operation').map((item: { args: { request: { id: string } } }) => item.args.request.id));
  expect(ids).toEqual(['operation-1', 'operation-1']);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
});
