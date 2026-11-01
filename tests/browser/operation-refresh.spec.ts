import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test('completion refresh waits until the open file action releases its listing snapshot', async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await page.getByRole('button', { name: '패널 1 새 폴더', exact: true }).click();
  const folder = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await folder.getByRole('textbox').fill('queued');
  await folder.getByRole('button', { name: '대상 확인', exact: true }).click();
  await folder.getByRole('button', { name: '실행', exact: true }).click();
  await expect(folder).not.toBeVisible();
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const copy = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(copy.getByRole('textbox')).toBeVisible();
  const listingStarts = () => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'start_listing').length);
  const before = await listingStarts();
  expect(before).toBeGreaterThan(0);
  await page.evaluate(() => Reflect.get(window, 'p4Finish')());
  await expect(page.getByRole('region', { name: '파일 작업 기록' })).toContainText('완료');
  await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));
  expect(await listingStarts()).toBe(before);
  await copy.getByRole('button', { name: '취소', exact: true }).click();
  await expect.poll(listingStarts).toBe(before + 1);
});
