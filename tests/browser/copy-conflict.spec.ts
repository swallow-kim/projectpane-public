import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

for (const option of ['skip', 'name'] as const) test(`paste collision offers ${option} without overwriting or re-reading clipboard`, async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled();
  await page.evaluate(() => Reflect.set(window, 'p4TransferError', 'destination_exists'));
  await page.getByRole('listbox', { name: '패널 1 파일 목록' }).press('Control+v');
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(dialog).toContainText('덮어쓰기는 지원하지 않습니다');
  await expect(dialog.getByRole('textbox', { name: '대상 폴더 절대 경로' })).toHaveValue('C:\\Temporary');
  if (option === 'skip') await dialog.getByRole('checkbox', { name: '같은 이름의 기존 파일 모두 건너뛰기' }).check();
  else {
    await dialog.getByRole('checkbox', { name: '다른 이름으로 복사 (파일 1개)' }).check();
    await dialog.getByRole('textbox', { name: '복사할 새 이름' }).fill('새 이름.txt');
  }
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).toHaveCount(0);
  const calls = await page.evaluate(() => Reflect.get(window, 'p4Calls'));
  expect(calls.filter((x: { command: string }) => x.command === 'begin_copy_job')).toHaveLength(2);
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(1);
  expect(calls.filter((x: { command: string }) => x.command === 'capture_copy_clipboard')).toHaveLength(1);
  const request = calls.filter((x: { command: string }) => x.command === 'begin_copy_job').at(-1).args.request;
  expect(option === 'skip' ? request.conflictPolicy : request.name).toBe(option === 'skip' ? 'skip' : '새 이름.txt');
});

test('queue busy is not misidentified as a destination collision', async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled();
  await page.evaluate(() => Reflect.set(window, 'p4TransferError', 'busy'));
  await page.getByRole('listbox', { name: '패널 1 파일 목록' }).press('Control+v');
  await expect(page.getByRole('alert')).toBeVisible();
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(0);
});
