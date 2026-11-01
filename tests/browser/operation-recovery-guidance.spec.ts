import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';
import { expandOperations } from './compact-ui-fixture';

for (const code of [0x80070070, 0x80070005]) test(`partial failure ${code.toString(16)} preserves results and never retries on review`, async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(code => {
    const counts = { changed: 1, failed: 1, skipped: 0, unknown: 0 };
    Reflect.get(window, 'p4Operations').set('recovery-1', { id: 'recovery-1', projectId: null, status: 'partially_succeeded', counts, journalSaved: true });
    Reflect.get(window, 'p4Reports').set('recovery-1', { counts,
      evidence: { performSucceeded: false, outcomesComplete: true, aborted: false, cancellationConfirmed: false },
      items: [
        { source: 'C:\\fixture\\done.txt', destination: 'D:\\target\\done.txt', status: 'changed', hresult: 0 },
        { source: 'C:\\fixture\\failed.txt', destination: null, status: 'failed', hresult: code | 0 },
      ] });
  }, code);
  await expandOperations(page);
  await page.getByRole('button', { name: '결과 보기', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 작업 결과', exact: true });
  await expect(dialog).toContainText('변경 1 · 건너뜀 0 · 실패 1 · 불명 0');
  await expect(dialog).toContainText('취소는 실행 취소(Undo)가 아닙니다');
  await expect(dialog).toContainText('자동 재시도·삭제·원상복구하지 않습니다');
  await expect(dialog).toContainText(code === 0x80070070 ? '여유 공간이 부족' : '접근할 권한이 없습니다');
  await expect(dialog).toContainText(`0x${code.toString(16)}`);
  await dialog.getByRole('button', { name: '닫기', exact: true }).click();
  const commands = await page.evaluate(() => Reflect.get(window, 'p4Calls').map((x: { command: string }) => x.command));
  expect(commands).not.toContain('execute_operation');
  expect(commands).not.toContain('forget_operation');
});
