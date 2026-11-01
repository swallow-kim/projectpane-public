import { expandOperations } from './compact-ui-fixture';
import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function seed(page: Page, count: number, protectedRecords = false) {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(({ count, protectedRecords }) => {
    for (let i = 0; i < count; i++) {
      const id = `history-${i}`;
      const unknown = protectedRecords && i === 0;
      const counts = { changed: 0, skipped: 0, failed: 0, unknown: 0 };
      Reflect.get(window, 'p4Operations').set(id, { id, projectId: null, status: unknown ? 'interrupted_unknown' : 'cancelled', counts, journalSaved: !(protectedRecords && i === 1) });
      Reflect.get(window, 'p4Reports').set(id, { counts, items: [], evidence: { performSucceeded: false, aborted: unknown ? null : true, outcomesComplete: !unknown, cancellationConfirmed: !unknown } });
    }
  }, { count, protectedRecords });
  await expect(page.getByRole('button', { name: new RegExp(`파일 작업 ${count}개`) })).toBeVisible();
}

const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === command).length, command);

test('reviewed metadata removal needs confirmation and clamps the final page', async ({ page }, info) => {
  await seed(page, 9);
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  await tray.getByRole('button', { name: '다음 작업', exact: true }).click();
  await tray.getByRole('button', { name: '결과 보기', exact: true }).click();
  const result = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  await result.getByRole('button', { name: '기록 정리', exact: true }).click();
  await expect(result).toContainText('실제 파일은 그대로 유지됩니다');
  await result.getByRole('button', { name: '정리 취소', exact: true }).click();
  expect(await calls(page, 'forget_operation')).toBe(0);
  await result.getByRole('button', { name: '기록 정리', exact: true }).click();
  const forget = result.getByRole('button', { name: '기록만 지우기', exact: true });
  await forget.focus();
  await forget.dispatchEvent('compositionstart');
  await page.keyboard.press('Enter');
  await forget.dispatchEvent('compositionend');
  await forget.dispatchEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229 });
  expect(await calls(page, 'forget_operation')).toBe(0);
  await result.screenshot({ path: info.outputPath('history-confirm.png') });
  await forget.click();
  await expect(result).not.toBeVisible();
  await expect(tray).toContainText('파일 작업 8개');
  await expect(tray.getByRole('button', { name: '결과 보기', exact: true })).toHaveCount(8);
  expect(await calls(page, 'forget_operation')).toBe(1);
  expect(await calls(page, 'execute_operation')).toBe(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').has('history-0'))).toBe(false);
});

test('unknown and unsaved results cannot be forgotten', async ({ page }) => {
  await seed(page, 2, true);
  await expect(page.locator('.operation-tray__toggle')).toContainText('확인 필요 2');
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  const result = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  for (let i = 0; i < 2; i++) {
    await tray.getByRole('button', { name: '결과 보기', exact: true }).nth(i).click();
    await expect(result.getByRole('button', { name: '기록 정리', exact: true })).toBeDisabled();
    await result.getByRole('button', { name: '닫기', exact: true }).click();
  }
  expect(await calls(page, 'forget_operation')).toBe(0);
});

test('capacity guidance survives failure and a pending removal submits only once', async ({ page }, info) => {
  await seed(page, 256);
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  const history = tray.getByRole('dialog', { name: '파일 작업 상태와 결과', exact: true });
  await expect(history.getByRole('status')).toContainText('기록 한도에 도달했습니다');
  await tray.screenshot({ path: info.outputPath('history-capacity.png') });
  await tray.getByRole('button', { name: '결과 보기', exact: true }).first().click();
  const result = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  await result.getByRole('button', { name: '기록 정리', exact: true }).click();
  await page.evaluate(() => Reflect.set(window, 'p4FailForget', true));
  await result.getByRole('button', { name: '기록만 지우기', exact: true }).click();
  await expect(result.getByRole('alert')).toContainText('기록을 정리하지 못했습니다');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(256);
  await page.evaluate(() => { Reflect.set(window, 'p4FailForget', false); Reflect.set(window, 'p4HoldForget', true); });
  await result.getByRole('button', { name: '기록만 지우기', exact: true }).click();
  await expect(result.getByRole('button', { name: '기록 정리 중…', exact: true })).toBeDisabled();
  await expect(result.getByRole('button', { name: '정리 취소', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(result).toBeVisible();
  expect(await calls(page, 'forget_operation')).toBe(2);
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect(result).not.toBeVisible();
  await expect(tray).toContainText('파일 작업 255개');
  await expect(history.getByRole('status')).toHaveCount(0);
  expect(await calls(page, 'execute_operation')).toBe(0);
});
