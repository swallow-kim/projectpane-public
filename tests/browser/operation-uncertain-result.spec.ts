import { expandOperations } from './compact-ui-fixture';
import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

for (const scenario of [
  { name: 'conflicting cancellation evidence', status: 'interrupted_unknown', aborted: false, cancellationConfirmed: true, label: '결과 확인 필요' },
  { name: 'unconfirmed native termination', status: 'interrupted_unknown', aborted: null, cancellationConfirmed: false, label: '결과 확인 필요' },
  { name: 'confirmed cancellation after changes', status: 'partially_succeeded', aborted: true, cancellationConfirmed: true, label: '일부 완료' },
] as const) {
  test(`${scenario.name} retains the whole-operation verdict with zero unknown items`, async ({ page }, info) => {
    await installOperations(page); await page.goto('/');
    await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
    await page.evaluate(scenario => {
      const counts = { changed: 1, skipped: 0, failed: 0, unknown: 0 };
      Reflect.get(window, 'p4Operations').set('evidence-1', { id: 'evidence-1', projectId: null, status: scenario.status, counts, journalSaved: true });
      Reflect.get(window, 'p4Reports').set('evidence-1', { counts,
        evidence: { performSucceeded: false, outcomesComplete: true, aborted: scenario.aborted, cancellationConfirmed: scenario.cancellationConfirmed },
        items: [{ source: 'C:\\fixture\\copied.txt', destination: 'D:\\fixture\\copied.txt', status: 'changed', hresult: 0 }] });
    }, scenario);
    await expandOperations(page);
    await page.getByRole('region', { name: '파일 작업 기록' }).getByRole('button', { name: '결과 보기', exact: true }).click();
    const dialog = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
    await expect(dialog).toContainText(`전체 작업: ${scenario.label}`);
    await expect(dialog).toContainText('변경 1 · 건너뜀 0 · 실패 0 · 불명 0');
    if (scenario.status === 'interrupted_unknown') {
      await expect(dialog.getByRole('alert')).toContainText('작업 전체의 완료·취소 여부를 확인하지 못했습니다');
      await expect(dialog.getByRole('alert')).toContainText('자동으로 재실행하지 않습니다');
      await expect(dialog.getByRole('button', { name: '기록 정리', exact: true })).toBeDisabled();
    } else {
      await expect(dialog.getByRole('alert')).toHaveCount(0);
      await expect(dialog.getByRole('button', { name: '기록 정리', exact: true })).toBeEnabled();
    }
    if (scenario.cancellationConfirmed) await expect(dialog).toContainText('이미 변경된 항목은 자동으로 되돌리지 않습니다');
    const commands = await page.evaluate(() => Reflect.get(window, 'p4Calls').map((call: { command: string }) => call.command));
    expect(commands).not.toContain('execute_operation'); expect(commands).not.toContain('forget_operation');
    await dialog.screenshot({ path: info.outputPath(`${scenario.status}-${scenario.aborted}.png`) });
  });
}
