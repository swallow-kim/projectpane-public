import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

// Every clipboard command is mocked. These tests never read or change the host clipboard.
const list = (page: Page) => page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
const calls = (page: Page, command: string) => page.evaluate(command => (Reflect.get(window, 'p4Calls') as { command: string; args: unknown }[]).filter(call => call.command === command), command);
test.beforeEach(async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await list(page).getByRole('option').first().click();
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByRole('button', { name: '복사 담기', exact: true })).toBeEnabled();
});

test('ordinary copy and paste use Windows while explicit app storage stays separate', async ({ page }) => {
  await list(page).press('Control+Shift+c');
  await expect(page.locator('.transfer-status')).toContainText('복사 1개 담음');
  await list(page).press('Control+v');
  await expect.poll(async () => calls(page, 'execute_operation')).toHaveLength(1);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await calls(page, 'import_from_explorer')).toHaveLength(1);
  expect(await calls(page, 'prepare_transfer')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(1);
});

test('changed Windows clipboard cancels a pending internal cut before showing a move', async ({ page }) => {
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  await page.evaluate(() => Reflect.set(window, 'p4ClipboardSequence', 2));
  await list(page).press('Control+v');
  await expect(page.getByRole('alert')).toContainText('앱 잘라내기를 취소했습니다');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await calls(page, 'import_from_explorer')).toHaveLength(0);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await list(page).press('Control+v');
  await expect.poll(async () => calls(page, 'execute_operation')).toHaveLength(1);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await calls(page, 'import_from_explorer')).toHaveLength(1);
});

test('a clipboard change during source capture cannot leave a usable cut token', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4HoldCapture', true));
  await list(page).press('Control+x');
  await expect.poll(async () => calls(page, 'capture_transfer')).toHaveLength(1);
  await page.evaluate(() => { Reflect.set(window, 'p4ClipboardSequence', 2); Reflect.get(window, 'p4Release')(); });
  await expect(page.getByRole('alert')).toContainText('앱 잘라내기를 취소했습니다');
  await expect(page.locator('.transfer-status')).not.toContainText('개 담음');
  await expect(page.getByRole('button', { name: '앱에 붙여넣기', exact: true }).first()).toBeDisabled();
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('Windows copy supersedes a local cut and inaccessible change counters fail closed', async ({ page }) => {
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  await list(page).press('Control+c');
  await expect(page.locator('.transfer-status')).toContainText('Windows에 복사했습니다');
  await expect(page.getByRole('button', { name: '앱에 붙여넣기', exact: true }).first()).toBeDisabled();
  expect(await calls(page, 'export_to_explorer')).toHaveLength(1);
  await page.evaluate(() => Reflect.set(window, 'p4ClipboardSequence', 0));
  await list(page).press('Control+x');
  await expect(page.getByRole('alert')).toBeVisible();
  expect(await calls(page, 'capture_transfer')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});
