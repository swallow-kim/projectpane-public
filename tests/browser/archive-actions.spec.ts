import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((c: { command: string }) => c.command === command), command);
async function setup(page: Page, archive?: string) {
  await installOperations(page, 3);
  if (archive) await page.addInitScript(name => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__'); const previous = bridge.invoke;
    bridge.invoke = async (command: string, args: unknown) => { const result = await previous(command, args);
      if (['start_listing', 'read_listing', 'set_listing_view'].includes(command) && result.batch) result.batch.entries[0].name = name;
      return result;
    };
  }, archive);
  await page.goto('/'); await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  return page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
}
test('single and multiple right-click compression preserve selection and submit Windows ZIP once', async ({ page }) => {
  const list = await setup(page); const rows = list.getByRole('option');
  await rows.nth(0).click(); await rows.nth(2).click({ modifiers: ['Control'] }); await rows.nth(0).click({ button: 'right' });
  await expect(list.locator('[aria-selected=true]')).toHaveCount(2);
  await page.getByRole('menuitem', { name: '압축하기…', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '압축하기', exact: true });
  await expect(dialog.getByLabel('압축 앱', { exact: true })).toHaveValue('windows');
  await dialog.getByLabel('ZIP 파일 이름', { exact: true }).fill('한글 공백.zip');
  await dialog.getByRole('button', { name: '압축하기', exact: true }).click();
  await expect.poll(() => calls(page, 'execute_operation')).toHaveLength(1);
  expect((await calls(page, 'prepare_operation'))[0].args.request.action).toMatchObject({ kind: 'archive', name: '한글 공백.zip', extract: false, selection: { entryIds: ['entry-0', 'entry-2'] } });
  await expect(dialog).not.toBeVisible();
  expect(await calls(page, 'launch_archive_tool')).toHaveLength(0);
});
test('ZIP extraction proposes a separate directory and keeps the right-click target', async ({ page }) => {
  const list = await setup(page, '한글 archive.zip');
  await list.getByRole('option').nth(0).click({ button: 'right' });
  await page.getByRole('menuitem', { name: '압축 해제…', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '압축 해제', exact: true });
  await expect(dialog.getByLabel('새 압축 해제 폴더 이름')).toHaveValue('한글 archive (압축 해제)');
  await dialog.getByRole('button', { name: '압축 해제', exact: true }).click();
  await expect.poll(() => calls(page, 'execute_operation')).toHaveLength(1);
  expect((await calls(page, 'prepare_operation'))[0].args.request.action).toMatchObject({ kind: 'archive', extract: true, selection: { entryIds: ['entry-0'] } });
});
test('other formats require a detected app; handoff and failure are not recorded as completed jobs', async ({ page }) => {
  const list = await setup(page, 'sample.7z');
  await list.getByRole('option').nth(0).click({ button: 'right' });
  await page.getByRole('menuitem', { name: '압축 해제…', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '압축 해제', exact: true });
  await expect(dialog.getByRole('button', { name: '압축 해제', exact: true })).toBeDisabled();
  await dialog.getByLabel('압축 앱', { exact: true }).selectOption('seven_zip');
  await page.evaluate(() => Reflect.set(window, 'p4ArchiveLaunchFails', true));
  await dialog.getByRole('button', { name: '압축 해제', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(dialog).not.toContainText('압축 앱으로 전달했습니다');
  await page.evaluate(() => Reflect.set(window, 'p4ArchiveLaunchFails', false));
  await dialog.getByRole('button', { name: '압축 해제', exact: true }).click();
  await expect(dialog).toContainText('압축 앱으로 전달했습니다');
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  expect(await calls(page, 'prepare_operation')).toHaveLength(0);
});
