import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

const list = (page: Page) => page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === command), command);

async function setup(page: Page, result: 'accepted' | 'cancelled' | 'failed' = 'accepted') {
  await installOperations(page, 3);
  await page.addInitScript(result => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__');
    const previous = bridge.invoke;
    bridge.invoke = async (command: string, args: { action?: string }) => {
      const response = await previous(command, args);
      if (command === 'export_to_explorer' && args.action === 'context_menu') {
        if (result === 'failed') throw { code: 'io_error' };
        return result === 'accepted';
      }
      return response;
    };
  }, result);
  await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled();
}

test('right click keeps the app menu; Shell integration is an explicit action with preserved multi-selection', async ({ page }) => {
  await setup(page);
  await list(page).getByRole('option').nth(1).click({ modifiers: ['Control'] });
  await list(page).getByRole('option').first().click({ button: 'right' });
  expect(await calls(page, 'export_to_explorer')).toHaveLength(0);
  await expect(page.getByRole('menuitem', { name: '이름 변경', exact: true })).toBeDisabled();
  const shell = page.getByRole('menuitem', { name: 'Windows 메뉴', exact: true });
  await expect(shell).toHaveAttribute('title', /앱 작업 기록과 복구 기능은 적용되지 않습니다/);
  await shell.click();
  await expect(page.locator('.transfer-status')).toContainText('Windows 메뉴의 명령을 전달했습니다');
  expect((await calls(page, 'export_to_explorer'))[0].args).toMatchObject({ action: 'context_menu', selection: { entryIds: ['entry-0', 'entry-1'] } });
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('Shell cancellation does not claim success, clear selection or replay the action', async ({ page }) => {
  await setup(page, 'cancelled');
  await list(page).getByRole('option').first().click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Windows 메뉴', exact: true }).click();
  await expect(page.locator('.transfer-status')).toContainText('Windows 메뉴를 취소했습니다');
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-selected', 'true');
  expect(await calls(page, 'export_to_explorer')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('a Shell error keeps the normal menu and explicit Explorer fallback available without retry', async ({ page }) => {
  await setup(page, 'failed');
  await list(page).getByRole('option').first().click({ button: 'right' });
  await page.getByRole('menuitem', { name: 'Windows 메뉴', exact: true }).click();
  await expect(page.getByRole('alert')).toContainText('명령이 실행됐는지 확인');
  expect(await calls(page, 'export_to_explorer')).toHaveLength(1);
  await list(page).getByRole('option').first().click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: '열기', exact: true })).toBeEnabled();
  await page.getByRole('menuitem', { name: '탐색기에서 선택 항목 보기', exact: true }).click();
  await expect.poll(async () => calls(page, 'export_to_explorer')).toHaveLength(2);
  expect((await calls(page, 'export_to_explorer'))[1].args.action).toBe('reveal');
});
