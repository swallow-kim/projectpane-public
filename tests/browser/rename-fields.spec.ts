import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function openRename(page: Page, name = '한글.txt', kind: 'file' | 'folder' = 'file') {
  await installOperations(page);
  await page.addInitScript(({ name, kind }) => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__');
    const previous = bridge.invoke;
    bridge.invoke = async (command: string, args: { request?: { action?: { kind?: string } } }) => {
      const value = await previous(command, args);
      if (['start_listing', 'read_listing', 'set_listing_view'].includes(command)) {
        for (const entry of value.batch.entries) if (entry.id === 'entry-0') Object.assign(entry, { name, kind });
      }
      if (command === 'prepare_operation' && args.request?.action?.kind === 'rename') value.sources = [`C:\\Temporary\\${name}`];
      return value;
    };
  }, { name, kind });
  await page.goto('/');
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록' });
  await list.getByRole('option').first().click(); await list.press('F2');
  return page.getByRole('dialog', { name: '이름 변경', exact: true });
}

for (const [original, stem, extension] of [
  ['한글.txt', '한글', 'txt'], ['.env', '.env', ''], ['README', 'README', ''],
  ['archive.tar.gz', 'archive', 'tar.gz'], ['meeting.v2.final.docx', 'meeting.v2.final', 'docx'],
]) test(`rename ${original} preselects only its name and preserves the extension`, async ({ page }) => {
  const dialog = await openRename(page, original);
  const input = dialog.getByRole('textbox', { name: '이름', exact: true });
  await expect(input).toHaveValue(stem); await expect(input).toBeFocused();
  expect(await input.evaluate((node: HTMLInputElement) => [node.selectionStart, node.selectionEnd])).toEqual([0, stem.length]);
  await expect(dialog.getByRole('textbox', { name: '확장자', exact: true })).toHaveValue(extension);
  await input.fill('새 이름');
  const final = extension ? `새 이름.${extension}` : '새 이름';
  await expect(dialog.getByLabel('최종 파일명')).toHaveText(final);
  await input.press('Enter'); await expect(dialog).toHaveCount(0);
  const calls = await page.evaluate(() => Reflect.get(window, 'p4Calls'));
  expect(calls.find((x: { command: string }) => x.command === 'prepare_operation').args.request.action.name).toBe(final);
  expect(calls.filter((x: { command: string }) => x.command === 'execute_operation')).toHaveLength(1);
});

for (const extension of ['csv', '']) test(`extension ${extension || 'removal'} needs explicit confirmation, and Escape cancels safely`, async ({ page }) => {
  const dialog = await openRename(page);
  await dialog.getByRole('textbox', { name: '확장자', exact: true }).fill(extension);
  await dialog.getByRole('button', { name: '확장자 변경 확인' }).click();
  await expect(dialog.getByRole('alert')).toContainText('파일 내용이나 형식은 변환되지 않으며');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((x: { command: string }) => x.command === 'execute_operation'))).toHaveLength(0);
  await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0);
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Calls').filter((x: { command: string }) => x.command === 'discard_operation').length)).toBe(1);
});

test('extension confirmation executes exactly once and retains the final name', async ({ page }) => {
  const dialog = await openRename(page);
  await dialog.getByRole('textbox', { name: '확장자', exact: true }).fill('csv');
  await dialog.getByRole('button', { name: '확장자 변경 확인' }).click();
  await expect(dialog).toContainText('한글.csv');
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((x: { command: string }) => x.command === 'execute_operation'))).toHaveLength(1);
});

test('folder names with dots stay in one input and Tab/Escape do not execute', async ({ page }) => {
  const dialog = await openRename(page, 'folder.with.dots', 'folder');
  await expect(dialog.getByRole('textbox')).toHaveCount(1);
  await expect(dialog.getByRole('textbox', { name: '이름', exact: true })).toHaveValue('folder.with.dots');
  await page.keyboard.press('Tab'); await page.keyboard.press('Escape'); await expect(dialog).toHaveCount(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((x: { command: string }) => x.command === 'prepare_operation'))).toHaveLength(0);
});

test('case-only rename is allowed and Windows forbidden names never reach preparation', async ({ page }) => {
  const dialog = await openRename(page, 'Name.txt');
  const input = dialog.getByRole('textbox', { name: '이름', exact: true });
  for (const invalid of ['CON', 'bad/name', 'bad:stream']) {
    await input.fill(invalid);
    await expect(dialog.getByRole('button', { name: '이름 변경', exact: true })).toBeDisabled();
  }
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((x: { command: string }) => x.command === 'prepare_operation'))).toHaveLength(0);
  await input.fill('NAME'); await input.press('Enter'); await expect(dialog).toHaveCount(0);
});
