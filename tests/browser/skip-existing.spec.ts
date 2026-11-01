import { expandOperations } from './compact-ui-fixture';
import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';
import type { OperationPreview, OperationReport, OperationSnapshot } from '../../src/shared/operations';

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
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
  const plan = await page.evaluate(() => [...Reflect.get(window, 'p4Prepared').values()][0]);
  expect(plan.kind).toBe('copy_skip_existing');
  expect(plan.skippedSources).toEqual(plan.sources);
});

for (const count of [1, 2]) test(`skip result preserves decision provenance for ${count} source files`, async ({ page }, info) => {
  const dialog = await openCopy(page, count);
  await page.evaluate(() => Reflect.set(window, 'p4SkipCount', 1));
  await dialog.getByRole('checkbox', { name: policy, exact: true }).check();
  await dialog.getByRole('button', { name: '복사', exact: true }).click();

  await expect(dialog).not.toBeVisible();
  await page.evaluate(() => {
    const plans: Map<string, OperationPreview> = Reflect.get(window, 'p4Prepared');
    const operations: Map<string, OperationSnapshot> = Reflect.get(window, 'p4Operations');
    const reports: Map<string, OperationReport> = Reflect.get(window, 'p4Reports');
    for (const [id, snapshot] of operations) {
      const plan = plans.get(id); if (!plan?.skippedSources) throw new Error('missing policy preview');
      const userSkipped = plan.skippedSources.map(source => ({ source, destination: `${plan.destination}\\${source.split('\\').at(-1)}` }));
      const items = plan.sources.filter(source => !plan.skippedSources?.includes(source)).map(source => ({ source, destination: `${plan.destination}\\${source.split('\\').at(-1)}`, status: 'changed' as const, hresult: 0 }));
      const counts = { changed: items.length, skipped: userSkipped.length, failed: 0, unknown: 0 };
      const only = items.length === 0;
      snapshot.status = only ? 'succeeded' : 'partially_succeeded'; snapshot.counts = counts;
      reports.set(id, { counts, items, userSkipped, ...(only ? { userDecisionsOnly: true } : {}), evidence: { performSucceeded: !only, aborted: only ? null : false, outcomesComplete: true, cancellationConfirmed: false } });
    }
  });
  await expandOperations(page);
  await page.getByRole('region', { name: '파일 작업 기록' }).getByRole('button', { name: '결과 보기', exact: true }).click();
  const result = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  await expect(result).toContainText(`변경 ${count - 1} · 건너뜀 1 · 실패 0 · 불명 0`);
  await expect(result).toContainText('건너뜀 · 사용자 선택');
  await expect(result).not.toContainText('Windows 코드');
  await expect(result).not.toContainText('작업을 시작하지 못했습니다');
  if (count === 1) await expect(result).toContainText('실제 복사는 실행하지 않았습니다');
  await expect(result.getByRole('listitem')).toHaveCount(count);
  await result.screenshot({ path: info.outputPath(`skip-result-${count}.png`) });
});

test('cancelled pending skip preview is discarded and cannot execute later', async ({ page }) => {
  const dialog = await openCopy(page);
  await page.evaluate(() => Reflect.set(window, 'p4HoldPrepare', true));
  await dialog.getByRole('checkbox', { name: policy, exact: true }).check();
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).toContainText('대상 확인 중'); await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Prepared').size)).toBe(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});

test('all 17 sources retain skip provenance without a redundant preview page', async ({ page }) => {
  const dialog = await openCopy(page, 17);
  await page.evaluate(() => Reflect.set(window, 'p4SkipCount', 1));
  await dialog.getByRole('checkbox', { name: policy, exact: true }).check();
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  const plan = await page.evaluate(() => [...Reflect.get(window, 'p4Prepared').values()][0]);
  expect(plan.sources).toHaveLength(17);
  expect(plan.skippedSources).toEqual(['C:\\Temporary\\선택-16.txt']);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((x: { command: string }) => x.command === 'execute_operation').length)).toBe(1);
});
