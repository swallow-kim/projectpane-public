import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';
const list = (page: Page) => page.locator('[data-pane="0"] [role="listbox"]');
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((c: { command: string }) => c.command === command), command);
async function setup(page: Page, total = 1) {
  await installOperations(page, total);
  await page.addInitScript(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__'); const previous = bridge.invoke;
    bridge.invoke = async (command: string, args: any) => {
      if (command === Reflect.get(window, 'cutFailureCommand') && Reflect.get(window, 'cutFailureCode')) throw { code: Reflect.get(window, 'cutFailureCode') };
      return previous(command, args);
    };
  });
  await page.goto('/');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await list(page).getByRole('option').first().click();
}

test('large cut opens an explicit copy dialog, never moves, and can cancel without copying', async ({ page }) => {
  await setup(page, 1025);
  await list(page).press('Control+a');
  await expect(page.locator('[data-pane="0"] .selection-status')).toContainText('1,025');
  await expect(page.locator('[data-pane="0"]').getByRole('button', { name: '잘라내기', exact: true })).toBeEnabled();
  await list(page).press('Control+x');
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('원본은 그대로 남습니다');
  expect(await calls(page, 'capture_transfer')).toHaveLength(0);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
  await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
});

test('unsupported cut capture switches to copy with reason and chosen destination', async ({ page }) => {
  await setup(page);
  await page.evaluate(() => { Reflect.set(window, 'cutFailureCommand', 'capture_transfer'); Reflect.set(window, 'cutFailureCode', 'unsupported_provider'); });
  await list(page).press('Control+x');
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('복사로 전환');
  await dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true }).fill('C:\\Target');
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(1);
});

test('move destination fallback survives source navigation and only copies after explicit confirmation', async ({ page }) => {
  await setup(page);
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.fill('C:\\Target'); await address.press('Enter');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await page.evaluate(() => { Reflect.set(window, 'cutFailureCommand', 'prepare_transfer'); Reflect.set(window, 'cutFailureCode', 'invalid_request'); });
  await list(page).press('Control+v');
  await page.getByRole('dialog', { name: '파일 이동', exact: true }).getByRole('button', { name: '대상 확인', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(dialog.getByRole('alert')).toContainText('원본은 그대로 남습니다');
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(await calls(page, 'begin_copy_job')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await expect(page.locator('.transfer-status')).not.toContainText('잘라내기 1개 담음');
});

for (const code of ['stale_request', 'io_error', 'protocol_error']) test(`move preparation ${code} never auto-converts to copy`, async ({ page }) => {
  await setup(page);
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  await page.evaluate(code => { Reflect.set(window, 'cutFailureCommand', 'prepare_transfer'); Reflect.set(window, 'cutFailureCode', code); }, code);
  await list(page).press('Control+v');
  const dialog = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
  await expect(page.getByRole('dialog', { name: '파일 복사', exact: true })).not.toBeVisible();
});

test('failed execution cannot become a second copy', async ({ page }) => {
  await setup(page);
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  await list(page).press('Control+v');
  const dialog = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await page.evaluate(() => Reflect.set(window, 'p4LoseReply', true));
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
  expect(await calls(page, 'execute_operation')).toHaveLength(1);
});


test('direct move of more than 1024 selections offers bounded copy rather than staying disabled', async ({ page }) => {
  await setup(page, 1025);
  await list(page).press('Control+a');
  await expect(page.locator('[data-pane="0"] .selection-status')).toContainText('1,025');
  await page.getByRole('button', { name: '패널 1 파일 이동', exact: true }).click();
  const move = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await move.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true }).fill('C:\\Target');
  await move.getByRole('button', { name: '대상 확인', exact: true }).click();
  const copy = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(copy.getByRole('alert')).toContainText('처리 한도');
  await copy.getByRole('button', { name: '복사', exact: true }).click();
  await expect(copy).not.toBeVisible();
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(1);
  const chunks = await calls(page, 'append_copy_capture');
  expect(chunks.reduce((n: number, call: any) => n + call.args.request.selection.entryIds.length, 0)).toBe(1025);
});
