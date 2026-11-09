import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

// Mocked IPC only: no host clipboard is read or written by this suite.
const rows = (page: Page) => page.locator('[data-pane="0"] .file-row');
const exports = (page: Page) => page.evaluate(() => Reflect.get(window, 'p4Calls')
  .filter((call: { command: string }) => call.command === 'export_to_explorer')
  .map((call: { args: { action: string; basePath: string | null; selection: { entryIds: string[] } } }) => call.args));
test.beforeEach(async ({ page }) => {
  await installOperations(page, 10); await page.goto('/');
  // This fixture intentionally has no watch service. Its delayed notice moves
  // the rows and can close an already-open context menu on the resulting scroll.
  await expect(page.getByText('자동 갱신을 사용할 수 없습니다.', { exact: false })).toBeVisible();
  await expect(rows(page).first()).toHaveAttribute('aria-selected', 'true');
});

test('absolute path copy uses the right-clicked item and stays separate from file copy', async ({ page }, info) => {
  await rows(page).nth(2).click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: '절대경로 복사', exact: true })).toBeEnabled();
  await page.screenshot({ path: info.outputPath('path-copy-menu.png') });
  await page.getByRole('menuitem', { name: '절대경로 복사', exact: true }).click();
  await expect(page.locator('.transfer-status')).toContainText('절대경로 1개를 텍스트로 복사했습니다');
  expect(await exports(page)).toEqual([expect.objectContaining({ action: 'absolute_path', basePath: null, selection: expect.objectContaining({ entryIds: ['entry-2'] }) })]);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((c: { command: string }) => ['capture_transfer', 'execute_operation', 'begin_copy_capture', 'begin_copy_job'].includes(c.command)))).toHaveLength(0);
});

test('relative paths keep the selected group and use the current panel folder', async ({ page }) => {
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.fill('\\\\server\\share\\한글 폴더'); await address.press('Enter');
  await expect(page.locator('[data-pane="0"] .file-list')).toHaveAttribute('aria-busy', 'false');
  await rows(page).first().click(); await rows(page).nth(3).click({ modifiers: ['Control'] });
  await expect(page.locator('[data-pane="0"] .selection-status')).toHaveText('2개 선택');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await rows(page).first().click({ button: 'right' });
  const item = page.getByRole('menuitem', { name: '상대경로 복사 (현재 폴더 기준)', exact: true });
  await expect(item).toHaveAttribute('title', /현재 패널 폴더 기준: \\\\server\\share\\한글 폴더/);
  await item.click();
  await expect(page.locator('.transfer-status')).toContainText('상대경로 2개를 텍스트로 복사했습니다');
  expect((await exports(page))[0]).toMatchObject({ action: 'relative_path', basePath: '\\\\server\\share\\한글 폴더', selection: { entryIds: ['entry-0', 'entry-3'] } });
  await expect(page.locator('[data-pane="0"] .selection-status')).toHaveText('2개 선택');
});

for (const failure of ['io_error', 'unconfirmed']) test(`clipboard ${failure} never shows success`, async ({ page }) => {
  await page.evaluate(failure => { if (failure === 'unconfirmed') Reflect.set(window, 'p4ExportAccepted', false); else Reflect.set(window, 'p4ExportError', failure); }, failure);
  await rows(page).first().click({ button: 'right' });
  await page.getByRole('menuitem', { name: '절대경로 복사', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('경로를 클립보드에 복사하지 못했습니다');
  await expect(page.locator('.transfer-status')).not.toContainText('텍스트로 복사했습니다');
});

test('copying path text clears an earlier internal cut and keyboard menus remain usable', async ({ page }) => {
  const list = page.locator('[data-pane="0"] .file-list');
  await list.press('Control+x'); await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  await list.press('Shift+F10');
  await page.getByRole('menuitem', { name: '절대경로 복사', exact: true }).focus(); await page.keyboard.press('Enter');
  await expect(page.locator('.transfer-status')).toContainText('텍스트로 복사했습니다');
  await expect(page.getByRole('button', { name: '앱에 붙여넣기', exact: true }).first()).toBeDisabled();
  await expect(list).toBeFocused();
});

test('background context menu cannot copy paths without a selected target', async ({ page }) => {
  await page.locator('[data-pane="0"] .file-list').dispatchEvent('contextmenu', { clientX: 100, clientY: 220 });
  await expect(page.getByRole('menuitem', { name: '절대경로 복사', exact: true })).toBeDisabled();
  await expect(page.getByRole('menuitem', { name: '상대경로 복사 (현재 폴더 기준)', exact: true })).toBeDisabled();
  expect(await exports(page)).toHaveLength(0);
});
