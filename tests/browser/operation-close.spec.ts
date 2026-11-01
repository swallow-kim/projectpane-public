import { expect, test } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

for (const scenario of [
  { choice: '완료까지 대기', command: 'wait', newer: 'blocked', text: '결과 저장을 확인하지 못했습니다', keepDisabled: false },
  { choice: '취소 요청 후 닫기', command: 'cancel', newer: 'stopping', text: '작업 종료를 기다리고 있습니다', keepDisabled: true },
  { choice: '완료까지 대기', command: 'wait', newer: 'stopped', text: '', keepDisabled: false },
  { choice: '창 유지', command: 'continue', newer: 'deciding', text: '접수된 파일 작업이 남아 있습니다', keepDisabled: false },
  { choice: '완료까지 대기', command: 'wait', newer: 'results_unsaved', text: '실행 중인 파일 작업은 없습니다', keepDisabled: false },
] as const) {
  test(`newer ${scenario.newer} notice survives a delayed ${scenario.command} reply`, async ({ page }) => {
    await installProjects(page, true);
    await page.goto('/');
    await expect(page.getByText('임시 작업 공간 · 자동 저장 안 함')).toBeVisible();
    await page.evaluate(() => { Reflect.get(window, 'p3HoldCloseReply')(); Reflect.get(window, 'p3OperationClose')('deciding'); });
    const dialog = page.getByRole('dialog', { name: '파일 작업을 마친 뒤 닫기' });

    await dialog.getByRole('button', { name: scenario.choice, exact: true }).click();
    await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'operation_close_choice').length)).toBe(1);
    await page.evaluate(newer => Reflect.get(window, 'p3OperationClose')(newer), scenario.newer);
    await page.evaluate(() => Reflect.get(window, 'p3ReleaseCloseReply')());
    await page.evaluate(() => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve()))));

    if (scenario.newer === 'stopped') {
      await expect(dialog).not.toBeVisible();
    } else {
      await expect(dialog).toContainText(scenario.text);
      if (scenario.keepDisabled) await expect(dialog.getByRole('button', { name: '창 유지', exact: true })).toBeDisabled();
      else await expect(dialog.getByRole('button', { name: '창 유지', exact: true })).toBeEnabled();
      if (scenario.newer === 'blocked' || scenario.newer === 'stopping') await expect(dialog.getByRole('button')).toHaveCount(1);
    }
    expect(await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').length)).toBe(0);
  });
}

test('close choices preserve the window until actual completion and retain both event listeners', async ({ page }, info) => {
  await installProjects(page, true);
  await page.goto('/');
  await expect(page.getByText('임시 작업 공간 · 자동 저장 안 함')).toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('deciding'));
  const dialog = page.getByRole('dialog', { name: '파일 작업을 마친 뒤 닫기' });
  await expect(dialog).toBeVisible();
  await expect(dialog.getByRole('button', { name: '창 유지', exact: true })).toBeFocused();
  await page.screenshot({ path: info.outputPath('operation-close-decision.png') });
  await dialog.getByRole('button', { name: '취소 요청 후 닫기' }).click();
  await expect(dialog).toContainText('작업 종료를 기다리고 있습니다');
  expect(await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').length)).toBe(0);
  await dialog.getByRole('button', { name: '창 유지', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('deciding'));
  await dialog.getByRole('button', { name: '완료까지 대기' }).click();
  await expect(dialog).toBeVisible();
  await page.evaluate(() => { Reflect.get(window, 'p3OperationClose')('stopped'); Reflect.get(window, 'p3Close')(); });
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').length)).toBe(1);
  await expect(dialog).not.toBeVisible();
});

test('unsaved results require explicit exit acknowledgement after native shutdown', async ({ page }) => {
  await installProjects(page, true);
  await page.goto('/');
  await expect(page.getByText('임시 작업 공간 · 자동 저장 안 함')).toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('results_unsaved'));
  const dialog = page.getByRole('dialog', { name: '파일 작업을 마친 뒤 닫기' });
  await expect(dialog).toContainText('결과 일부를 저장하지 못했습니다');
  await expect(dialog).toContainText('남은 작업은 자동으로 실행하지 않습니다');
  await expect(dialog.getByRole('button', { name: '창 유지', exact: true })).toBeFocused();
  await dialog.getByRole('button', { name: '창 유지', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('results_unsaved'));
  await dialog.getByRole('button', { name: '기록 미저장 상태로 닫기', exact: true }).click();
  await expect(dialog).toContainText('작업 종료를 기다리고 있습니다');
  await expect(dialog.getByRole('button', { name: '창 유지', exact: true })).toBeDisabled();
  const choices: string[] = await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'operation_close_choice').map((call: { args: { choice: string } }) => call.args.choice));
  expect(choices).toEqual(['continue', 'exit_without_results']);
  expect(await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').length)).toBe(0);
});

test('unknown native termination stays explicit and keeping the window never claims cancellation', async ({ page }) => {
  await installProjects(page, true);
  await page.goto('/');
  await expect(page.getByText('임시 작업 공간 · 자동 저장 안 함')).toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('blocked'));
  const dialog = page.getByRole('dialog');
  await expect(dialog).toContainText('파일 작업 종료 또는 결과 저장을 확인하지 못했습니다');
  await expect(dialog.getByRole('button')).toHaveCount(1);
  await dialog.getByRole('button', { name: '창 유지', exact: true }).press('Escape');
  await expect(dialog).not.toBeVisible();
  const commands: string[] = await page.evaluate(() => Reflect.get(window, 'p3Calls').map((call: { command: string }) => call.command));
  expect(commands).not.toContain('project_close_ready');
  expect(commands).not.toContain('execute_operation');
});
