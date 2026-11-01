import { expandOperations } from './compact-ui-fixture';
import { expect, test } from '@playwright/test';
import type { OperationReport, OperationSnapshot } from '../../src/shared/operations';
import { confirmRecycle, installRecycle, openRecycle, recycleCallCount, recycleDialog } from './p4-recycle-fixture';

test('explicit confirmation displays native sources and returned result, then reload never replays', async ({ page }, info) => {
  await installRecycle(page);
  await openRecycle(page);
  await expect(recycleDialog(page)).toContainText('C:\\Native\\entry-0-검증.txt');
  await expect(recycleDialog(page).locator('form, input')).toHaveCount(0);
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  await confirmRecycle(page).click();
  await expect(recycleDialog(page)).not.toBeVisible();
  await page.evaluate(() => {
    Reflect.get(window, 'p4Finish')();
    const reports: Map<string, OperationReport> = Reflect.get(window, 'p4Reports');
    const report = reports.get('operation-1');
    if (!report) throw new Error('Expected completion report');
    report.items[0].destination = 'C:\\$Recycle.Bin\\owned-browser-fixture';
  });
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  await expect(tray).toContainText('완료');
  await tray.getByRole('button', { name: '결과 보기' }).click();
  const report = page.getByRole('dialog', { name: '파일 작업 결과' });
  await expect(report).toContainText('변경 1');
  await expect(report).toContainText('C:\\Native\\entry-0-검증.txt');
  await expect(report).toContainText('C:\\$Recycle.Bin\\owned-browser-fixture');
  await page.screenshot({ path: info.outputPath('recycle-result.png') });
  const journal = await page.evaluate(() => {
    const operations: Map<string, OperationSnapshot> = Reflect.get(window, 'p4Operations');
    const reports: Map<string, OperationReport> = Reflect.get(window, 'p4Reports');
    return { operations: [...operations], reports: [...reports] };
  });
  await page.addInitScript(journal => {
    for (const [id, value] of journal.operations) Reflect.get(window, 'p4Operations').set(id, value);
    for (const [id, value] of journal.reports) Reflect.get(window, 'p4Reports').set(id, value);
  }, journal);
  await page.reload();
  await expandOperations(page);
  await expect(tray).toContainText('완료');
  await tray.getByRole('button', { name: '결과 보기' }).click();
  await expect(report).toContainText('C:\\Native\\entry-0-검증.txt');
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(0);
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  await page.screenshot({ path: info.outputPath('recycle-reloaded-result.png') });
});

test('lost confirmation reply retries only the immutable prepared ID', async ({ page }) => {
  await installRecycle(page);
  await openRecycle(page);
  await page.evaluate(() => Reflect.set(window, 'p4LoseReply', true));
  await confirmRecycle(page).click();
  await expect(recycleDialog(page).getByRole('alert')).toBeVisible();
  await confirmRecycle(page).click();
  await expect(recycleDialog(page)).not.toBeVisible();
  const requests = await page.evaluate(() => window.recycle.calls.filter(call => call.command === 'execute_operation').map(call => call.args));
  expect(requests).toEqual([{ request: { id: 'operation-1' } }, { request: { id: 'operation-1' } }]);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(1);
});

test('full history refuses recycle without automatic cleanup or execution', async ({ page }, info) => {
  await installRecycle(page);
  await page.goto('/');
  await page.evaluate(() => {
    const operations: Map<string, OperationSnapshot> = Reflect.get(window, 'p4Operations');
    for (let i = 0; i < 256; i++) operations.set(`history-${i}`, { id: `history-${i}`, projectId: null, status: 'cancelled', counts: { changed: 0, skipped: 0, failed: 0, unknown: 0 }, journalSaved: true });
    window.recycle.prepareError = 'resource_limit';
  });
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  await expect(tray.getByRole('status')).toContainText('기록 한도에 도달했습니다');
  await tray.getByRole('dialog', { name: '파일 작업 상태와 결과', exact: true }).getByRole('button', { name: '닫기', exact: true }).click();
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  await page.getByRole('button', { name: '패널 1 휴지통', exact: true }).click();
  await expect(recycleDialog(page).getByRole('alert')).toContainText('작업 한도');
  await expect(confirmRecycle(page)).toBeDisabled();
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  expect(await recycleCallCount(page, 'forget_operation')).toBe(0);
  await page.screenshot({ path: info.outputPath('recycle-capacity.png') });
});
