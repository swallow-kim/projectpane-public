import { expect, test, type Page } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

async function updates(page: Page, installed = true, ephemeral = false, failed = false, slow = false) {
  await installProjects(page, ephemeral);
  await page.addInitScript(({ installed, failed, slow }) => {
    const api = Reflect.get(window, '__TAURI_INTERNALS__');
    const original = api.invoke;
    const calls: string[] = [];
    const state = { currentVersion: '1.3.0', installed, phase: 'idle', version: null as string | null,
      notes: null as string | null, error: null as string | null, received: 0, total: null, lastCheck: null, previousInstall: null };
    Object.assign(window, { updateCalls: calls, updateState: state });
    let finishDownload: (() => void) | undefined;
    api.invoke = async (command: string, args: Record<string, unknown>) => {
      if (!['update_status', 'check_update', 'download_update', 'cancel_update_download', 'install_update', 'open_update_page'].includes(command)) return original(command, args);
      calls.push(command);
      if (command === 'check_update') Object.assign(state, { phase: 'available', version: '1.3.1', notes: '<script>synthetic-not-executed</script> 새 버전' });
      if (command === 'download_update') {
        if (slow) {
          state.phase = 'downloading';
          await new Promise<void>(resolve => { finishDownload = resolve; });
        } else Object.assign(state, { phase: failed ? 'error' : 'ready', error: failed ? 'download_or_signature' : null });
      }
      if (command === 'cancel_update_download') { state.phase = 'available'; finishDownload?.(); }
      if (command === 'install_update') state.phase = 'preparing';
      return structuredClone(state);
    };
  }, { installed, failed, slow });
  await page.goto('/');
  await page.getByRole('button', { name: '업데이트', exact: true }).click();
  await expect(page.getByRole('dialog', { name: 'ProjectPane 업데이트' })).toBeVisible();
}
test('manual check, separate download/install choices, escaped notes and no unsolicited install', async ({ page }) => {
  await updates(page);
  const dialog = page.getByRole('dialog', { name: 'ProjectPane 업데이트' });
  await dialog.getByRole('button', { name: '업데이트 확인', exact: true }).click();
  await expect(dialog).toContainText('<script>synthetic-not-executed</script>');
  expect(await page.evaluate(() => Reflect.get(window, 'updateCalls').includes('install_update'))).toBe(false);
  await dialog.getByRole('button', { name: '다운로드', exact: true }).click();
  await expect(dialog).toContainText('서명 확인 완료');
  expect(await page.evaluate(() => Reflect.get(window, 'updateCalls').includes('install_update'))).toBe(false);
  await dialog.getByRole('button', { name: '설치 후 다시 시작' }).click();
  await expect(dialog).toContainText('설정 저장을 확인');
  await expect(dialog.getByRole('button', { name: '닫기', exact: true })).toBeDisabled();
});
test('portable never offers an installer action', async ({ page }) => {
  await updates(page, false);
  const dialog = page.getByRole('dialog', { name: 'ProjectPane 업데이트' });
  await dialog.getByRole('button', { name: '업데이트 확인', exact: true }).click();
  await expect(dialog).toContainText('앱 내 설치를 지원하지 않습니다');
  await expect(dialog.getByRole('button', { name: '다운로드', exact: true })).toHaveCount(0);
  await expect(dialog.getByRole('button', { name: '설치 후 다시 시작' })).toHaveCount(0);
  await page.keyboard.press('Escape');
  await expect(dialog).not.toBeVisible();
  await expect(page.getByRole('button', { name: '새 업데이트', exact: true })).toBeFocused();
});
test('failed signature keeps installation unavailable', async ({ page }) => {
  await updates(page, true, false, true);
  const dialog = page.getByRole('dialog', { name: 'ProjectPane 업데이트' });
  await dialog.getByRole('button', { name: '업데이트 확인', exact: true }).click();
  await dialog.getByRole('button', { name: '다운로드', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('설치를 시작하지 않았습니다');
  await expect(dialog.getByRole('button', { name: '설치 후 다시 시작' })).toHaveCount(0);
});
test('an unsaved temporary workspace must be saved before restart', async ({ page }) => {
  await updates(page, true, true);
  const dialog = page.getByRole('dialog', { name: 'ProjectPane 업데이트' });
  await dialog.getByRole('button', { name: '업데이트 확인', exact: true }).click();
  await dialog.getByRole('button', { name: '다운로드', exact: true }).click();
  await expect(dialog.getByRole('button', { name: '설치 후 다시 시작' })).toBeDisabled();
  await expect(dialog).toContainText('프로젝트로 저장한 뒤 설치');
});

test('cancelling a pending download returns to available without invoking install', async ({ page }) => {
  await updates(page, true, false, false, true);
  const dialog = page.getByRole('dialog', { name: 'ProjectPane 업데이트' });
  await dialog.getByRole('button', { name: '업데이트 확인', exact: true }).click();
  await dialog.getByRole('button', { name: '다운로드', exact: true }).click();
  await dialog.getByRole('button', { name: '다운로드 취소' }).click();
  await expect(dialog.getByRole('button', { name: '다운로드', exact: true })).toBeEnabled();
  await expect(dialog.getByRole('button', { name: '설치 후 다시 시작' })).toHaveCount(0);
  expect(await page.evaluate(() => Reflect.get(window, 'updateCalls').includes('install_update'))).toBe(false);
});
