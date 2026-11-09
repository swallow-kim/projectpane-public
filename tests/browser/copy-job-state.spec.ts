import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function start(page: Page) {
  await installOperations(page); await page.goto('/');
  const list = page.locator('[data-pane="0"] [role="listbox"]');
  await expect(list).toHaveAttribute('aria-busy', 'false'); await list.press('Control+v');
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(1);
  await page.getByRole('region', { name: '복사 작업', exact: true }).getByRole('button', { name: /^복사 작업/ }).click();
  return page.getByRole('dialog', { name: '복사 진행과 결과', exact: true });
}

test('asynchronous partial failure preserves changed results and never auto retries', async ({ page }) => {
  const dialog = await start(page);
  await page.evaluate(() => {
    for (const job of Reflect.get(window, 'p4CopyJobs').values()) {
      Object.assign(job, { status: 'failed', counts: { changed: 1, skipped: 0, failed: 1, unknown: 0 }, outcomeCount: 2, rootsCompleted: 1, failure: { code: 'destination_exists' } });
    }
    Reflect.set(window, 'p4CopyOutcomes', [
      { source: 'C:\\source\\done.txt', destination: 'D:\\target\\done.txt', status: 'changed', hresult: 0 },
      { source: 'C:\\source\\collision.txt', destination: 'D:\\target\\collision.txt', status: 'failed', hresult: -2147024816 },
    ]);
  });
  await expect(dialog).toContainText('완료 1 · 건너뜀 0 · 실패 1 · 확인 필요 0');
  await dialog.getByRole('button', { name: '복사 결과 보기', exact: true }).click();
  await expect(dialog.getByRole('region', { name: '복사 결과 페이지' }).getByRole('listitem')).toHaveCount(2);
  await expect(dialog).toContainText('done.txt'); await expect(dialog).toContainText('collision.txt');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((c: any) => c.command === 'begin_copy_job').length)).toBe(1);
});

test('cancellation is explicit, preserves already copied files, and paginates outcomes', async ({ page }) => {
  const dialog = await start(page);
  await dialog.getByRole('button', { name: '복사 취소', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '복사 취소', exact: true })).toBeDisabled();
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((c: any) => c.command === 'cancel_copy_job').length)).toBe(1);
  await page.evaluate(() => {
    for (const job of Reflect.get(window, 'p4CopyJobs').values()) Object.assign(job, { status: 'cancelled', counts: { changed: 17, skipped: 0, failed: 0, unknown: 0 }, outcomeCount: 17, rootsCompleted: 0 });
    Reflect.set(window, 'p4CopyOutcomes', Array.from({ length: 17 }, (_, i) => ({ source: `C:\\source\\file-${i}.txt`, destination: `D:\\target\\file-${i}.txt`, status: 'changed', hresult: 0 })));
  });
  await expect(dialog).toContainText('완료 17');
  await expect(dialog).toContainText('취소해도 이미 복사한 항목은 유지됩니다');
  await dialog.getByRole('button', { name: '복사 결과 보기', exact: true }).click();
  const result = dialog.getByRole('region', { name: '복사 결과 페이지' });
  await expect(result.getByRole('listitem')).toHaveCount(16);
  await result.getByRole('button', { name: '다음 복사 결과', exact: true }).click();
  await expect(result.getByRole('listitem')).toHaveCount(1); await expect(result).toContainText('file-16.txt');
  await expect(result.getByRole('button', { name: '다음 복사 결과', exact: true })).toBeDisabled();
  const calls = await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((c: any) => c.command === 'read_copy_job_outcomes'));
  expect(calls.map((c: any) => c.args.request.offset)).toEqual([0, 16]);
  expect(calls.every((c: any) => c.args.request.limit === 16)).toBe(true);
});

test('an active empty result page can refresh without discarding the job', async ({ page }) => {
  const dialog = await start(page);
  await page.evaluate(() => Reflect.set(window, 'p4CopyPageActive', true));
  await dialog.getByRole('button', { name: '복사 결과 보기', exact: true }).click();
  const result = dialog.getByRole('region', { name: '복사 결과 페이지' });
  await expect(result.getByRole('listitem')).toHaveCount(0);
  await expect(dialog.getByRole('alert')).toHaveCount(0);
  await page.evaluate(() => {
    Reflect.set(window, 'p4CopyPageActive', false);
    Reflect.set(window, 'p4CopyOutcomes', [{ source: 'C:\\source\\done.txt', destination: 'D:\\target\\done.txt', status: 'changed', hresult: 0 }]);
  });
  await result.getByRole('button', { name: /새로 고침/ }).click();
  await expect(result.getByRole('listitem')).toHaveCount(1);
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(1);
});

test('an incomplete result journal keeps valid rows and warns without replay', async ({ page }) => {
  const dialog = await start(page);
  await page.evaluate(() => {
    Reflect.set(window, 'p4CopyPageFault', 'io_error');
    Reflect.set(window, 'p4CopyOutcomes', [{ source: 'C:\\source\\saved.txt', destination: 'D:\\target\\saved.txt', status: 'changed', hresult: 0 }]);
  });
  await dialog.getByRole('button', { name: '복사 결과 보기', exact: true }).click();
  const result = dialog.getByRole('region', { name: '복사 결과 페이지' });
  await expect(result.getByRole('alert')).toContainText('불완전');
  await expect(result.getByRole('alert')).toContainText('자동으로 다시 복사하지 않습니다');
  await expect(result.getByRole('listitem')).toHaveCount(1); await expect(result).toContainText('saved.txt');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((c: any) => c.command === 'begin_copy_job').length)).toBe(1);
});
