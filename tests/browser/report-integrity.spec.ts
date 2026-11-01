import { expandOperations } from './compact-ui-fixture';
import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

for (const fault of ['counts', 'hresult', 'source-nul', 'destination-nul', 'source-bytes', 'destination-bytes'] as const) {
  test(`inconsistent ${fault} cannot open a trusted result or retire its record`, async ({ page }, info) => {
    await installOperations(page); await page.goto('/');
    await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
    await page.evaluate(fault => {
      const counts = { changed: 1, skipped: 0, failed: 0, unknown: 0 };
      const operations: Map<string, unknown> = Reflect.get(window, 'p4Operations');
      const reports: Map<string, unknown> = Reflect.get(window, 'p4Reports');
      operations.set('report-1', { id: 'report-1', projectId: null, status: 'succeeded', counts, journalSaved: true });
      reports.set('report-1', { counts: fault === 'counts' ? { ...counts, changed: 2 } : counts,
        evidence: { performSucceeded: true, aborted: false, outcomesComplete: true, cancellationConfirmed: false },
        items: [{ source: fault === 'source-nul' ? 'C:\\fixture\\bad\0name' : fault === 'source-bytes' ? '한'.repeat(10923) : 'C:\\fixture\\source.txt',
          destination: fault === 'destination-nul' ? 'D:\\fixture\\bad\0name' : fault === 'destination-bytes' ? '한'.repeat(10923) : 'D:\\fixture\\copy.txt',
          status: 'changed', hresult: fault === 'hresult' ? -2147024891 : 0 }] });
    }, fault);
    const tray = page.getByRole('region', { name: '파일 작업 기록' });
  await expandOperations(page);
    await tray.getByRole('button', { name: '결과 보기', exact: true }).click();
    await expect(tray.getByRole('alert')).toContainText('결과 기록을 확인할 수 없어 표시하지 않았습니다');
    await expect(tray.getByRole('alert')).toContainText('자동으로 다시 실행되지 않습니다');
    await expect(page.getByRole('dialog', { name: '파일 작업 결과', exact: true })).toHaveCount(0);
    await expect(page.getByRole('button', { name: '기록 정리', exact: true })).toHaveCount(0);
    const commands = await page.evaluate(() => {
      const calls: { command: string }[] = Reflect.get(window, 'p4Calls');
      return calls.map(call => call.command);
    });
    expect(commands).not.toContain('execute_operation'); expect(commands).not.toContain('forget_operation');
    await tray.screenshot({ path: info.outputPath(`report-${fault}-refused.png`) });
  });
}

test('unobserved native tail remains visible and cannot be retired', async ({ page }, info) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(() => {
    const counts = { changed: 1, skipped: 0, failed: 1, unknown: 1 };
    const operations: Map<string, unknown> = Reflect.get(window, 'p4Operations');
    const reports: Map<string, unknown> = Reflect.get(window, 'p4Reports');
    operations.set('partial-1', { id: 'partial-1', projectId: null, status: 'interrupted_unknown', counts, journalSaved: true });
    reports.set('partial-1', { counts, evidence: { performSucceeded: false, aborted: false, outcomesComplete: false, cancellationConfirmed: false }, items: [
      { source: 'C:\\fixture\\copied.txt', destination: 'D:\\fixture\\copied.txt', status: 'changed', hresult: 0 },
      { source: 'C:\\fixture\\locked.txt', destination: null, status: 'failed', hresult: -2144927705 },
    ] });
  });
  await expandOperations(page);
  await page.getByRole('region', { name: '파일 작업 기록' }).getByRole('button', { name: '결과 보기', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  await expect(dialog).toContainText('변경 1 · 건너뜀 0 · 실패 1 · 불명 1');
  await expect(dialog.getByRole('alert')).toContainText('자동으로 재실행하지 않습니다');
  await expect(dialog.getByRole('listitem')).toHaveCount(2);
  await expect(dialog.getByRole('button', { name: '기록 정리', exact: true })).toBeDisabled();
  await dialog.screenshot({ path: info.outputPath('report-unobserved-tail.png') });
});
