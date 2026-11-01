import { expandOperations } from './compact-ui-fixture';
import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test('estimated 100 percent remains running until the actual result and cancellation is only a request', async ({ page }, info) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(() => {
    Reflect.get(window, 'p4Operations').set('running-1', { id: 'running-1', projectId: null, status: 'running', counts: { changed: 0, skipped: 0, failed: 0, unknown: 0 }, journalSaved: true });
    Reflect.set(window, 'p4Progress', { operationId: 'running-1', total: 20, completed: 20 });
  });
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  await expect(tray).toContainText('실행 중');
  await expect(tray).toContainText('추정 100%');
  await expect(tray.getByRole('button', { name: '결과 보기' })).toHaveCount(0);
  await tray.getByRole('button', { name: '취소 요청', exact: true }).click();
  await expect(tray).toContainText('취소 요청됨');
  await expect(tray.getByRole('button', { name: '취소 요청', exact: true })).toBeDisabled();
  await page.screenshot({ path: info.outputPath('operation-progress-cancel.png') });
  await page.evaluate(() => { Reflect.set(window, 'p4Progress', null); Reflect.get(window, 'p4Finish')(); });
  await expect(tray).toContainText('완료');
  await expect(tray.getByRole('progressbar')).toHaveCount(0);
});

test('recovered unknown without completion explains manual verification and never replays', async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(() => Reflect.get(window, 'p4Operations').set('unknown-1', { id: 'unknown-1', projectId: null, status: 'interrupted_unknown', counts: { changed: 0, skipped: 0, failed: 0, unknown: 0 }, journalSaved: false }));
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  await tray.getByRole('button', { name: '결과 보기' }).click();
  await expect(tray.getByRole('alert')).toContainText('원본과 대상을 확인하세요');
  await expect(tray.getByRole('alert')).toContainText('자동으로 다시 실행되지 않습니다');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === 'execute_operation').length)).toBe(0);
});
