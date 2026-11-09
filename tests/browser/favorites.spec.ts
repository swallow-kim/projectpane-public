import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function install(page: Page) {
  await installOperations(page);
  await page.addInitScript(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__');
    const previous = bridge.invoke;
    bridge.invoke = async (command: string, args?: { request: { expectedRevision: number; folders: { name: string; path: string }[] } }) => {
      if (!['list_favorites', 'save_favorites'].includes(command)) return previous(command, args);
      const current = JSON.parse(sessionStorage.getItem('favorite-test') ?? '{"schemaVersion":1,"revision":0,"folders":[]}');
      if (command === 'list_favorites') return current;
      if (Reflect.get(window, 'favoriteHold')) await new Promise<void>(resolve => Reflect.set(window, 'favoriteRelease', resolve));
      if (Reflect.get(window, 'favoriteFail')) throw { code: 'io_error' };
      if (!args || current.revision !== args.request.expectedRevision) throw { code: 'revision_conflict' };
      const next = { ...current, revision: current.revision + 1, folders: args.request.folders };
      sessionStorage.setItem('favorite-test', JSON.stringify(next)); return next;
    };
  });
  await page.goto('/');
  await expect(page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first()).toBeEnabled();
}
const manager = (page: Page) => page.getByRole('dialog', { name: '폴더 즐겨찾기', exact: true });

test('current folder toggles globally, persists reload, and duplicate edit is rejected', async ({ page }) => {
  await install(page);
  await page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first().click();
  await expect(page.getByRole('button', { name: '현재 폴더 즐겨찾기 해제' }).first()).toBeEnabled();
  await page.reload();
  await expect(page.getByRole('button', { name: '현재 폴더 즐겨찾기 해제' }).first()).toBeEnabled();
  await page.getByRole('tab', { name: 'Beta', exact: true }).click();
  await page.getByRole('button', { name: '즐겨찾기 관리', exact: true }).click();
  await expect(manager(page)).toContainText('C:\\Temporary');
  await manager(page).getByRole('button', { name: '즐겨찾기 추가' }).click();
  await manager(page).getByLabel('즐겨찾기 이름').fill('중복');
  await manager(page).getByLabel('폴더 경로', { exact: true }).fill('c:/Temporary/./');
  await manager(page).getByRole('button', { name: '저장', exact: true }).click();
  await expect(manager(page).getByRole('alert')).toContainText('이미 등록');
});

test('unavailable favorites survive navigation errors and can be edited or removed', async ({ page }) => {
  await install(page);
  await page.getByRole('button', { name: '즐겨찾기 관리', exact: true }).click();
  await manager(page).getByRole('button', { name: '즐겨찾기 추가' }).click();
  await manager(page).getByLabel('즐겨찾기 이름').fill('오프라인');
  await manager(page).getByLabel('폴더 경로', { exact: true }).fill('C:\\Temporary\\missing');
  await manager(page).getByRole('button', { name: '저장', exact: true }).click();
  await manager(page).getByRole('button', { name: '닫기', exact: true }).click();
  await page.getByRole('button', { name: '폴더 즐겨찾기 열기' }).first().click();
  await manager(page).getByRole('button', { name: '새 탭', exact: true }).click();
  await expect(page.locator('[data-pane="0"] .listing-feedback')).toContainText('찾을 수');
  await page.getByRole('button', { name: '폴더 즐겨찾기 열기' }).first().click();
  await expect(manager(page)).toContainText('C:\\Temporary\\missing');
  await manager(page).getByRole('button', { name: '수정', exact: true }).click();
  await manager(page).getByLabel('폴더 경로', { exact: true }).fill('C:\\Temporary');
  await manager(page).getByRole('button', { name: '저장', exact: true }).click();
  await manager(page).getByRole('button', { name: '열기', exact: true }).click();
  await expect(page.getByRole('textbox', { name: '패널 1 주소', exact: true })).toHaveValue('C:\\Temporary');
  await page.getByRole('button', { name: '폴더 즐겨찾기 열기' }).first().click();
  await manager(page).getByRole('button', { name: '해제', exact: true }).click();
  await expect(manager(page)).toContainText('등록된 즐겨찾기가 없습니다');
});

test('destination favorite only fills the path and preserves prepare/confirm boundaries', async ({ page }) => {
  await install(page);
  await page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first().click();
  await page.locator('[data-pane="0"] .file-row').first().click();
  await page.getByRole('button', { name: '패널 1 파일 이동', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await dialog.getByLabel('즐겨찾는 대상 폴더').selectOption('C:\\Temporary');
  await expect(dialog.getByRole('textbox')).toHaveValue('C:\\Temporary');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => ['prepare_operation', 'execute_operation'].includes(call.command)).length)).toBe(0);
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '실행', exact: true })).toBeVisible();
  expect(await page.evaluate(() => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === 'execute_operation').length)).toBe(0);
});

test('failed favorite save preserves old list and blocks close until saved state is reloaded', async ({ page }) => {
  await install(page);
  await page.evaluate(() => Reflect.set(window, 'favoriteFail', true));
  await page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first().click();
  await expect(page.locator('.app-status')).toContainText('즐겨찾기 저장을 확인하지 못했습니다');
  await page.evaluate(() => Reflect.get(window, 'p3Emit')('projectpane-close-requested', { attempt: 1, timeoutMs: 2500 }));
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').at(-1)?.args.saved)).toBe(false);
  await page.getByRole('button', { name: '즐겨찾기 다시 불러오기', exact: true }).click();
  await expect(page.locator('.app-status')).not.toContainText('즐겨찾기 저장을 확인하지 못했습니다');
  await expect(page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first()).toBeEnabled();
});

test('copy favorite selects a destination without submitting until the single Copy action', async ({ page }) => {
  await install(page);
  await page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first().click();
  await page.locator('[data-pane="0"] .file-row').first().click();
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await dialog.getByLabel('즐겨찾는 대상 폴더').selectOption('C:\\Temporary');
  await expect(dialog.getByRole('textbox')).toHaveValue('C:\\Temporary');
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(0);
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(1);
});

test('pending favorite save is awaited before the normal close acknowledgement', async ({ page }) => {
  await install(page);
  await page.evaluate(() => Reflect.set(window, 'favoriteHold', true));
  const register = page.getByRole('button', { name: '현재 폴더 즐겨찾기 등록' }).first();
  await register.click();
  await expect(register).toBeDisabled();
  await page.evaluate(() => Reflect.get(window, 'p3Emit')('projectpane-close-requested', { attempt: 1, timeoutMs: 2500 }));
  expect(await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').length)).toBe(0);
  await page.evaluate(() => Reflect.get(window, 'favoriteRelease')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').at(-1)?.args.saved)).toBe(true);
  await expect(page.getByRole('button', { name: '현재 폴더 즐겨찾기 해제' }).first()).toBeEnabled();
});
