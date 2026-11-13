import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';
import { installRecycle } from './p4-recycle-fixture';

const list = (page: Page) => page.locator('[data-pane="0"] [role="listbox"]');
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === command), command);
async function english(page: Page) {
  await page.addInitScript(() => localStorage.setItem('projectpane.language.v1', 'en'));
  await page.goto('/');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
}
async function externalLanguage(page: Page, language: 'ko' | 'en') {
  await page.evaluate(language => {
    const key = 'projectpane.language.v1';
    const oldValue = localStorage.getItem(key);
    localStorage.setItem(key, language);
    window.dispatchEvent(new StorageEvent('storage', { key, oldValue, newValue: language, storageArea: localStorage }));
  }, language);
  await expect(page.locator('html')).toHaveAttribute('lang', language);
}

test('English large-cut fallback explains preserved originals and copies only after confirmation', async ({ page }, info) => {
  await installOperations(page, 1025); await english(page);
  await list(page).getByRole('option').first().click();
  await list(page).press('Control+a');
  await expect(page.locator('[data-pane="0"] .selection-status')).toContainText('1,025');
  await list(page).press('Control+x');
  const dialog = page.getByRole('dialog', { name: 'Copy files', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('The originals will remain');
  await expect(dialog.getByRole('alert')).not.toContainText(/[가-힣]/);
  expect(await calls(page, 'capture_transfer')).toHaveLength(0);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
  await dialog.getByRole('textbox', { name: 'Destination folder absolute path', exact: true }).fill('C:\\한글 대상');
  await externalLanguage(page, 'ko');
  const korean = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(korean.getByRole('alert')).toContainText('원본은 그대로 남습니다');
  await expect(korean.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true })).toHaveValue('C:\\한글 대상');
  await externalLanguage(page, 'en');
  await dialog.screenshot({ path: info.outputPath('english-copy-fallback.png') });
  await dialog.getByRole('button', { name: 'Copy', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const submitted = await calls(page, 'begin_copy_job');
  expect(submitted).toHaveLength(1);
  expect(submitted[0].args.request.destination).toBe('C:\\한글 대상');
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await expect(page.getByRole('region', { name: 'Copy jobs', exact: true })).toBeVisible();
});

test('English folder recycle shows safety guidance and cancellation never deletes', async ({ page }, info) => {
  await installRecycle(page, { kind: 'folder' }); await english(page);
  await list(page).getByRole('option').first().click();
  await page.getByRole('button', { name: 'Pane 1 Recycle Bin', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: 'Recycle Bin', exact: true });
  await expect(dialog).toContainText('including folder contents');
  await expect(dialog).toContainText('Nothing is deleted if the Recycle Bin is unavailable');
  await expect(dialog).toContainText('C:\\Native\\entry-0-검증.txt');
  await expect(dialog.getByRole('button', { name: 'Send to Recycle Bin', exact: true })).toBeEnabled();
  await dialog.screenshot({ path: info.outputPath('english-folder-recycle.png') });
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect.poll(async () => (await calls(page, 'discard_operation')).length).toBe(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('an existing dialog error retranslates without retrying or changing the typed name', async ({ page }) => {
  await installOperations(page); await english(page);
  await page.getByRole('button', { name: 'Pane 1 New folder', exact: true }).click();
  let dialog = page.getByRole('dialog', { name: 'New folder', exact: true });
  await dialog.getByRole('textbox', { name: 'New name', exact: true }).fill('collision');
  await dialog.getByRole('button', { name: 'Review destination', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('The destination name already exists');
  expect(await calls(page, 'prepare_operation')).toHaveLength(1);
  await externalLanguage(page, 'ko');
  dialog = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('대상 이름이 이미 있거나');
  await expect(dialog.getByRole('textbox', { name: '새 이름', exact: true })).toHaveValue('collision');
  await externalLanguage(page, 'en');
  dialog = page.getByRole('dialog', { name: 'New folder', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('The destination name already exists');
  expect(await calls(page, 'prepare_operation')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
  await expect(dialog).not.toBeVisible();
});

test('external cut failure remains understandable when the language changes', async ({ page }) => {
  await installOperations(page); await english(page);
  await list(page).getByRole('option').first().click();
  await page.evaluate(() => Reflect.set(window, 'p4RejectExternal', true));
  await list(page).press('Control+v');
  const message = page.getByRole('alert').filter({ hasText: 'External cut is not supported' });
  await expect(message).toBeVisible();
  await externalLanguage(page, 'ko');
  await expect(page.getByRole('alert').filter({ hasText: '외부 잘라내기는 지원하지 않으므로' })).toBeVisible();
  await externalLanguage(page, 'en');
  await expect(message).toBeVisible();
  expect(await calls(page, 'capture_copy_clipboard')).toHaveLength(1);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
});

test('English operation results preserve source names and diagnostic codes across language changes', async ({ page }) => {
  await installOperations(page); await english(page);
  await page.evaluate(() => {
    const counts = { changed: 0, skipped: 0, failed: 1, unknown: 0 };
    Reflect.get(window, 'p4Operations').set('locked-1', { id: 'locked-1', projectId: null, status: 'failed', counts, journalSaved: true });
    Reflect.get(window, 'p4Reports').set('locked-1', { counts, items: [{ source: 'C:\\fixture\\잠금 원본.txt', destination: null, status: 'failed', hresult: -2144927705 }], evidence: { performSucceeded: false, aborted: false, outcomesComplete: true, cancellationConfirmed: false } });
  });
  const tray = page.getByRole('region', { name: 'File operation log', exact: true });
  await expect(tray).toBeVisible();
  await tray.getByRole('button', { name: /^File operations/ }).click();
  await tray.getByRole('button', { name: 'View results', exact: true }).click();
  let dialog = page.getByRole('dialog', { name: 'File operation results', exact: true });
  await expect(dialog).toContainText('Another app is using the source file');
  await expect(dialog).toContainText('Cancel is not Undo');
  await expect(dialog).toContainText('C:\\fixture\\잠금 원본.txt');
  await expect(dialog).toContainText('0x80270027');
  await externalLanguage(page, 'ko');
  dialog = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  await expect(dialog).toContainText('원본 파일을 다른 앱이 사용 중입니다');
  await expect(dialog).toContainText('0x80270027');
  await externalLanguage(page, 'en');
  dialog = page.getByRole('dialog', { name: 'File operation results', exact: true });
  await expect(dialog).toContainText('Another app is using the source file');
  await expect(dialog).toContainText('C:\\fixture\\잠금 원본.txt');
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
  await expect(dialog).not.toBeVisible();
});
