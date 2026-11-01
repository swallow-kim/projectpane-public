import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

// Clipboard reads/writes are mocked; only the React/native IPC contract runs.
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === command), command);
const list = (page: Page) => page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
test.beforeEach(async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled();
});

test('repeated paste while validation is pending submits only once without a popup', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4HoldPrepare', true));
  await list(page).press('Control+v');
  await expect.poll(async () => calls(page, 'prepare_transfer')).toHaveLength(1);
  for (let i = 0; i < 3; i++) await list(page).press('Control+v');
  expect(await calls(page, 'import_from_explorer')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(async () => calls(page, 'execute_operation')).toHaveLength(1);
  await expect(page.locator('.transfer-status')).toContainText('복사 요청을 접수했습니다');
});

test('project change discards a late paste validation without copying', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4HoldPrepare', true));
  await list(page).press('Control+v');
  await expect.poll(async () => calls(page, 'prepare_transfer')).toHaveLength(1);
  await page.getByRole('tab', { name: 'Beta', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(async () => calls(page, 'discard_operation')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('lost paste reply exposes the uncertain result and does not retry or discard', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4LoseReply', true));
  await list(page).press('Control+v');
  await expect(page.getByRole('alert')).toContainText('자동으로 다시 실행하지 않습니다');
  expect(await calls(page, 'execute_operation')).toHaveLength(1);
  expect(await calls(page, 'discard_operation')).toHaveLength(0);
  await expect(page.locator('.transfer-status')).not.toContainText('복사 요청을 접수했습니다');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
});

for (const error of ['busy', 'invalid_path', 'access_denied']) test(`copy validation rejects ${error} without dispatch`, async ({ page }) => {
  await page.evaluate(error => Reflect.set(window, 'p4PrepareError', error), error);
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await dialog.getByRole('textbox').fill('D:\\Target');
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(dialog.getByRole('textbox')).toHaveValue('D:\\Target');
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('a lost destination-copy reply prevents a second request and remains in the log', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4LoseReply', true));
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await dialog.getByRole('textbox').fill('D:\\Target');
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('자동으로 다시 실행하지 않습니다');
  await expect(dialog.getByRole('button', { name: '복사', exact: true })).toBeDisabled();
  await dialog.getByRole('button', { name: '닫기', exact: true }).click();
  expect(await calls(page, 'execute_operation')).toHaveLength(1);
  expect(await calls(page, 'discard_operation')).toHaveLength(0);
});
