import { expandOperations } from './compact-ui-fixture';
import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test('native item failure explains the observed locked source and retains the Windows code', async ({ page }, info) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(() => {
    const counts = { changed: 0, skipped: 0, failed: 1, unknown: 0 };
    Reflect.get(window, 'p4Operations').set('locked-1', { id: 'locked-1', projectId: null, status: 'failed', counts, journalSaved: true });
    Reflect.get(window, 'p4Reports').set('locked-1', { counts, items: [{ source: 'C:\\fixture\\잠금 원본.txt', destination: null, status: 'failed', hresult: -2144927705 }], evidence: { performSucceeded: false, aborted: false, outcomesComplete: true, cancellationConfirmed: false } });
  });
  const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
  const result = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  for (let visit = 0; visit < 2; visit++) {
    await tray.getByRole('button', { name: '결과 보기', exact: true }).click();
    await expect(result).toContainText('원본 파일을 다른 앱이 사용 중입니다');
    await expect(result).toContainText('0x80270027');
    if (visit === 0) await result.screenshot({ path: info.outputPath('native-locked-source-reason.png') });
    await result.getByRole('button', { name: '닫기', exact: true }).click();
  }
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === 'execute_operation').length)).toBe(0);
});

for (const code of ['stale_request', 'worker_unavailable'] as const) {
  test(`retained ${code} explains its cause without replay or certainty about unknown results`, async ({ page }, info) => {
    await installOperations(page); await page.goto('/');
    await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
    await page.evaluate(code => {
      const lost = code === 'worker_unavailable';
      const counts = { changed: 0, skipped: 0, failed: 0, unknown: 0 };
      Reflect.get(window, 'p4Operations').set('failure-1', { id: 'failure-1', projectId: null, status: lost ? 'interrupted_unknown' : 'failed', counts, journalSaved: true });
      Reflect.get(window, 'p4Reports').set('failure-1', { failure: { code }, counts, items: [], evidence: { performSucceeded: false, aborted: lost ? null : false, outcomesComplete: !lost, cancellationConfirmed: false } });
    }, code);
    const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
    const result = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
    for (let visit = 0; visit < 2; visit++) {
      await tray.getByRole('button', { name: '결과 보기', exact: true }).click();
      await expect(result).toContainText(code === 'stale_request' ? '선택한 항목이 바뀌었습니다' : '실행 상태를 확인할 수 없습니다');
      if (code === 'worker_unavailable') await expect(result.getByRole('alert')).toContainText('자동으로 재실행하지 않습니다');
      else await expect(result.getByRole('alert')).toHaveCount(0);
      if (visit === 0) await result.screenshot({ path: info.outputPath('operation-failure-reason.png') });
      await result.getByRole('button', { name: '닫기', exact: true }).click();
    }
    expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === 'execute_operation').length)).toBe(0);
  });
}
