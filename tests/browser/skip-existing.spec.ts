import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function openCopy(page: Page, count = 1) {
  await installOperations(page, count); await page.goto('/');
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.getByRole('option').first().click();
  await expect(page.locator('.selection-status').first()).toHaveText('1개 선택');
  if (count > 2) await list.press('Shift+End');
  else if (count > 1) await list.getByRole('option').nth(count - 1).click({ modifiers: ['Shift'] });
  await expect(page.locator('.selection-status').first()).toHaveText(`${count}개 선택`);
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true }).fill('D:\\대상');
  return dialog;
}
const policy = '같은 이름의 기존 파일 모두 건너뛰기';

for (const width of [375, 768, 1280]) test(`explicit skip uses one submission and keeps the existing files at ${width}px`, async ({ page }, info) => {
  const dialog = await openCopy(page);
  await page.setViewportSize({ width, height: 900 });
  const skip = dialog.getByRole('checkbox', { name: policy, exact: true });
  const named = dialog.getByRole('checkbox', { name: '다른 이름으로 복사 (파일 1개)', exact: true });
  await expect(skip).not.toBeChecked(); await named.check(); await skip.check(); await expect(named).not.toBeChecked();
  await named.check(); await expect(skip).not.toBeChecked(); await skip.check();
  await dialog.screenshot({ path: info.outputPath(`skip-input-${width}.png`) });
  const bounds = await dialog.boundingBox(); expect(bounds).not.toBeNull();
  if (bounds) { expect(bounds.x).toBeGreaterThanOrEqual(0); expect(bounds.x + bounds.width).toBeLessThanOrEqual(width); }
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(0);
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(1);
  const call = await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'begin_copy_job'));
  expect(call).toHaveLength(1); expect(call[0].args.request.conflictPolicy).toBe('skip');
});

for (const count of [1, 2]) test(`skip policy and result retain existing-file evidence for ${count} source files`, async ({ page }, info) => {
  const dialog = await openCopy(page, count);
  await page.evaluate(() => Reflect.set(window, 'p4SkipCount', 1));
  await dialog.getByRole('checkbox', { name: policy, exact: true }).check();
  await dialog.getByRole('button', { name: '복사', exact: true }).click();

  await expect(dialog).not.toBeVisible();
  await page.evaluate(count => {
    const jobs = Reflect.get(window, 'p4CopyJobs');
    for (const job of jobs.values()) { job.status = 'succeeded'; job.counts = { changed: count - 1, skipped: 1, failed: 0, unknown: 0 }; job.rootsCompleted = count; job.outcomeCount = count; }
    Reflect.set(window, 'p4CopyOutcomes', Array.from({ length: count }, (_, i) => ({ source: `C:\\Temporary\\source-${i}.txt`, destination: `D:\\target\\source-${i}.txt`, status: i === count - 1 ? 'skipped' : 'changed', hresult: 0 })));
  }, count);
  await page.getByRole('region', { name: '복사 작업', exact: true }).getByRole('button', { name: /^복사 작업/ }).click();
  const result = page.getByRole('dialog', { name: '복사 진행과 결과', exact: true });
  await expect(result).toContainText(`완료 ${count - 1} · 건너뜀 1 · 실패 0 · 확인 필요 0`);
  await result.getByRole('button', { name: '복사 결과 보기', exact: true }).click();
  const results = result.getByRole('region', { name: '복사 결과 페이지', exact: true });
  await expect(results.getByRole('listitem')).toHaveCount(count);
  await expect(results).toContainText(`source-${count - 1}.txt`);
  const requests = await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'begin_copy_job'));
  expect(requests).toHaveLength(1); expect(requests[0].args.request.conflictPolicy).toBe('skip');
  await result.screenshot({ path: info.outputPath(`skip-result-${count}.png`) });
});

test('cancelled pending skip preview is discarded and cannot execute later', async ({ page }) => {
  const dialog = await openCopy(page);
  await page.evaluate(() => Reflect.set(window, 'p4HoldCopyCapture', true));
  await dialog.getByRole('checkbox', { name: policy, exact: true }).check();
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).toContainText('대상 확인 중'); await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4CopyCaptures').size)).toBe(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(0);
});

test('all 17 sources reach one explicit skip-policy job without a redundant preview page', async ({ page }) => {
  const dialog = await openCopy(page, 17);
  await page.evaluate(() => Reflect.set(window, 'p4SkipCount', 1));
  await dialog.getByRole('checkbox', { name: policy, exact: true }).check();
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const packets = await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'append_copy_capture'));
  expect(packets.flatMap((item: any) => item.args.request.selection.entryIds)).toHaveLength(17);
  const starts = await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'begin_copy_job'));
  expect(starts).toHaveLength(1); expect(starts[0].args.request.conflictPolicy).toBe('skip');
});
