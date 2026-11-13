import { expect, test, type Page } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

const language = (page: Page) => page.getByRole('combobox', { name: /언어 \/ Language|Language \/ 언어/ });
async function installEnglish(page: Page) {
  await installProjects(page, false, false, true);
  await page.addInitScript(() => {
    localStorage.setItem('projectpane.language.v1', 'en');
    const api = Reflect.get(window, '__TAURI_INTERNALS__');
    const original = api.invoke;
    let favorites = { schemaVersion: 1, revision: 0, folders: [] as { name: string; path: string }[] };
    const update = { currentVersion: '1.5.1', installed: true, phase: 'idle', version: null as string | null,
      notes: null, error: null as string | null, received: 0, total: null, lastCheck: null, previousInstall: null };
    const updates: string[] = [];
    Object.assign(window, { languageUpdateCalls: updates });
    api.invoke = async (command: string, args?: { request: { expectedRevision: number; folders: { name: string; path: string }[] } }) => {
      if (command === 'list_favorites') return structuredClone(favorites);
      if (command === 'save_favorites') {
        if (Reflect.get(window, 'languageFavoriteFailure')) throw { code: 'io_error' };
        if (!args || args.request.expectedRevision !== favorites.revision) throw { code: 'revision_conflict' };
        favorites = { ...favorites, revision: favorites.revision + 1, folders: args.request.folders };
        return structuredClone(favorites);
      }
      if (['update_status', 'check_update', 'download_update', 'install_update', 'open_update_page'].includes(command)) {
        updates.push(command);
        if (command === 'check_update') Object.assign(update, { phase: 'available', version: '1.5.2' });
        if (command === 'download_update') Object.assign(update, { phase: 'error', error: 'download_or_signature' });
        return structuredClone(update);
      }
      return original(command, args);
    };
  });
  await page.goto('/');
  await expect(page.getByRole('tablist', { name: 'Projects', exact: true }).getByRole('tab', { name: 'Alpha' })).toBeEnabled();
}
async function projectAction(page: Page, name: string) {
  await page.locator('.project-manager > summary').click();
  await page.getByRole('button', { name, exact: true }).click();
}
async function externalLanguage(page: Page, value: 'ko' | 'en') {
  await page.evaluate(value => {
    localStorage.setItem('projectpane.language.v1', value);
    window.dispatchEvent(new StorageEvent('storage', { key: 'projectpane.language.v1', newValue: value }));
  }, value);
}

test('English project creation and editing preserve Unicode input across language switches', async ({ page }) => {
  await installEnglish(page);
  await projectAction(page, 'New project');
  await page.getByRole('textbox', { name: 'Project name', exact: true }).fill('설계 프로젝트');
  await page.getByRole('textbox', { name: 'Root path', exact: true }).fill('D:\\한글 작업');
  await language(page).selectOption('ko');
  await expect(page.getByRole('textbox', { name: '프로젝트 이름', exact: true })).toHaveValue('설계 프로젝트');
  await expect(page.getByRole('textbox', { name: '루트 경로', exact: true })).toHaveValue('D:\\한글 작업');
  await language(page).selectOption('en');
  await page.getByRole('button', { name: 'Create', exact: true }).click();
  await expect(page.getByRole('tab', { name: '설계 프로젝트', exact: true })).toBeEnabled();
  await projectAction(page, 'Edit project');
  await expect(page.getByRole('textbox', { name: 'Root path', exact: true })).toHaveValue('D:\\한글 작업');
  await page.getByRole('textbox', { name: 'Project name', exact: true }).fill('설계 수정본');
  await page.getByRole('textbox', { name: 'Root path', exact: true }).fill('D:\\변경 폴더');
  await page.getByRole('button', { name: 'Apply', exact: true }).click();
  await expect(page.getByRole('tab', { name: '설계 수정본', exact: true })).toBeEnabled();
  const calls = await page.evaluate(() => Reflect.get(window, 'p3Calls'));
  expect(calls.find((call: { command: string }) => call.command === 'create_project').args.request).toMatchObject({ name: '설계 프로젝트', rootPath: 'D:\\한글 작업' });
  expect(calls.find((call: { command: string }) => call.command === 'edit_project').args.request).toMatchObject({ name: '설계 수정본', rootPath: 'D:\\변경 폴더' });
});

test('favorite validation and save failures translate live without replacing entered names or paths', async ({ page }) => {
  await installEnglish(page);
  await page.getByRole('button', { name: 'Manage favorites', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('button', { name: 'Add favorite' }).click();
  await dialog.getByLabel('Favorite name', { exact: true }).fill('내 자료');
  await dialog.getByLabel('Folder path', { exact: true }).fill('relative-folder');
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Enter a name and a valid absolute folder path.');
  await externalLanguage(page, 'ko');
  await expect(dialog.getByRole('alert')).toHaveText('이름과 올바른 절대 폴더 경로를 입력하세요.');
  await expect(dialog.getByLabel('즐겨찾기 이름')).toHaveValue('내 자료');
  await expect(dialog.getByLabel('폴더 경로', { exact: true })).toHaveValue('relative-folder');
  await externalLanguage(page, 'en');
  await dialog.getByLabel('Folder path', { exact: true }).fill('D:\\자료');
  await page.evaluate(() => Reflect.set(window, 'languageFavoriteFailure', true));
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog.getByRole('alert').filter({ hasText: 'Could not confirm that favorites were saved.' })).toBeVisible();
  await page.evaluate(() => Reflect.set(window, 'languageFavoriteFailure', false));
  await dialog.getByRole('button', { name: 'Save', exact: true }).click();
  await expect(dialog.locator('.favorite-description')).toHaveText('내 자료D:\\자료');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
});

test('English update errors preserve the download and install safety boundary', async ({ page }) => {
  await installEnglish(page);
  await page.getByRole('button', { name: 'Updates', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: /^ProjectPane (updates|업데이트)$/ });
  await dialog.getByRole('button', { name: 'Check for updates', exact: true }).click();
  await expect(dialog).toContainText('Version 1.5.2 is available.');
  await dialog.getByRole('button', { name: 'Download', exact: true }).click();
  await expect(dialog.getByRole('alert')).toHaveText('Download or signature verification failed. Installation has not started.');
  await expect(dialog.getByRole('button', { name: 'Install and restart' })).toHaveCount(0);
  await externalLanguage(page, 'ko');
  await expect(dialog.getByRole('alert')).toHaveText('다운로드 또는 서명 확인에 실패했습니다. 설치를 시작하지 않았습니다.');
  expect(await page.evaluate(() => Reflect.get(window, 'languageUpdateCalls').includes('install_update'))).toBe(false);
  await externalLanguage(page, 'en');
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
});

test('an open palette updates its command text and search after an external language change', async ({ page }) => {
  await installEnglish(page);
  await page.getByRole('button', { name: 'Quick navigation', exact: true }).click();
  const dialog = page.getByRole('dialog');
  await dialog.getByRole('combobox', { name: 'Search quick navigation' }).fill('reopen closed tab');
  await expect(dialog.getByRole('option')).toHaveCount(1);
  await externalLanguage(page, 'ko');
  await expect(dialog.getByRole('combobox', { name: '빠른 이동 검색' })).toHaveValue('reopen closed tab');
  await dialog.getByRole('combobox', { name: '빠른 이동 검색' }).fill('닫은 탭 복원');
  await expect(dialog.getByRole('option')).toHaveCount(1);
  await externalLanguage(page, 'en');
  await dialog.getByRole('combobox', { name: 'Search quick navigation' }).fill('Alpha');
  await expect(dialog.getByRole('option').filter({ hasText: 'Alpha' }).first()).toBeVisible();
  await dialog.getByRole('button', { name: 'Close', exact: true }).click();
});

test('English header controls remain on screen and do not overlap at 800 by 560', async ({ page }, info) => {
  await page.setViewportSize({ width: 800, height: 560 });
  await installEnglish(page);
  const controls = [
    language(page),
    page.getByRole('button', { name: 'Quick navigation', exact: true }),
    page.getByRole('button', { name: 'Manage favorites', exact: true }),
    page.getByRole('button', { name: 'Updates', exact: true }),
    page.locator('.project-manager > summary'),
  ];
  const boxes = [];
  for (const control of controls) {
    await expect(control).toBeVisible();
    const box = await control.boundingBox();
    expect(box).not.toBeNull();
    expect(box!.x).toBeGreaterThanOrEqual(0);
    expect(box!.x + box!.width).toBeLessThanOrEqual(800);
    expect(box!.y + box!.height).toBeLessThanOrEqual(560);
    boxes.push(box!);
  }
  for (let i = 0; i < boxes.length; i++) for (let j = i + 1; j < boxes.length; j++) {
    const a = boxes[i]!, b = boxes[j]!;
    expect(Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x) > 1 &&
      Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y) > 1).toBe(false);
  }
  await page.screenshot({ path: info.outputPath('english-800x560.png') });
});

