import { expect, test, type Page } from '@playwright/test';
import { installProjects } from './p3-project-fixture';
const projectTabs = (page: Page) => page.getByRole('tablist', { name: '프로젝트', exact: true });
const activeProject = (page: Page) => projectTabs(page).getByRole('tab', { selected: true });
async function projectAction(page: Page, name: string) {
  await page.locator('.project-manager > summary').click();
  await page.getByRole('button', { name, exact: true }).click();
}
const runtimeErrors = new WeakMap<Page, string[]>();
test.beforeEach(async ({ page }) => {
  await page.route('**/favicon.ico', route => route.fulfill({ status: 204 }));
  const errors: string[] = [];
  runtimeErrors.set(page, errors);
  page.on('pageerror', error => errors.push(error.message));
  page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
});
test.afterEach(({ page }) => { expect(runtimeErrors.get(page)).toEqual([]); });

test('initial last session loads before visible listings and only real tabs restore', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'alpha');
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByRole('tab')).toHaveCount(1);
  await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toHaveValue('C:\\Alpha\\missing');
  await expect(page.getByRole('region', { name: /^패널 \d$/ })).toHaveCount(1);
});

test('folder startup is ephemeral and never loads or saves the last project', async ({ page }) => {
  await installProjects(page, true);
  await page.goto('/');
  await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toHaveValue('C:\\Temporary');
  await page.getByRole('button', { name: '1패널', exact: true }).click();
  await expect(page.getByText('임시 작업 공간 · 자동 저장 안 함')).toBeVisible();
  const calls = await page.evaluate(() => Reflect.get(window, 'p3Calls'));
  expect(calls.some((call: { command: string }) => call.command === 'load_project' || call.command === 'save_project_session')).toBe(false);
});

test('newly shown panes render actual rows after restoring a one-pane project', async ({ page }, testInfo) => {
  await installProjects(page, false, false, true);
  await page.goto('/');
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'alpha');
  for (let cycle = 0; cycle < 2; cycle += 1) {
    await page.getByRole('button', { name: '4패널', exact: true }).click();
    for (const pane of [2, 3, 4]) {
      const list = page.getByRole('listbox', { name: `패널 ${pane} 파일 목록`, exact: true });
      await expect(list.getByRole('option').filter({ hasText: '한글.txt' })).toBeVisible();
    }
    await page.screenshot({ path: testInfo.outputPath(`p3-visible-four-${cycle}.png`) });
    await page.getByRole('button', { name: '1패널', exact: true }).click();
  }
});

test('debounced session is flushed before switching and restored independently of the preset', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  const select = activeProject(page);
  await expect(select).toHaveAttribute('data-project-id', 'alpha');
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await projectTabs(page).getByRole('tab', { name: 'Beta', exact: true }).click();
  await expect(select).toHaveAttribute('data-project-id', 'beta');
  await projectTabs(page).getByRole('tab', { name: 'Alpha', exact: true }).click();
  await expect(select).toHaveAttribute('data-project-id', 'alpha');
  await expect(page.getByRole('button', { name: '2패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
  const commands: string[] = await page.evaluate(() => Reflect.get(window, 'p3Calls').map((call: { command: string }) => call.command));
  expect(commands.indexOf('save_project_session')).toBeLessThan(commands.lastIndexOf('load_project'));
  await page.getByRole('textbox', { name: '패널 1 주소' }).fill('unfinished draft');
  await page.getByRole('searchbox', { name: '패널 1 이름 필터' }).fill('changed-filter');
  await projectAction(page, '프리셋 복원');
  await page.getByRole('button', { name: '복원하기', exact: true }).click();
  await expect(page.getByRole('searchbox', { name: '패널 1 이름 필터' })).toHaveValue('');
  await expect(page.getByRole('button', { name: '1패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toHaveValue('C:\\Alpha\\missing');
});

test('preset save, rename, duplicate and remove-list preserve folders and working state', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  const select = activeProject(page);
  await expect(select).toHaveAttribute('data-project-id', 'alpha');
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await projectAction(page, '현재 상태를 프리셋으로 저장');
  await page.getByRole('button', { name: '프리셋 저장하기', exact: true }).click();
  await expect(select).toBeEnabled();
  await page.getByRole('button', { name: '4패널', exact: true }).click();
  await projectAction(page, '이름 변경');
  await page.getByRole('textbox', { name: '프로젝트 이름', exact: true }).fill('Alpha renamed');
  await page.getByRole('button', { name: '변경하기', exact: true }).click();
  await expect(select).toBeEnabled();
  await expect(page.getByRole('button', { name: '4패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await projectAction(page, '복제');
  await page.getByRole('textbox', { name: '프로젝트 이름', exact: true }).fill('Alpha copy');
  await page.getByRole('button', { name: '복제하기', exact: true }).click();
  await expect(select).toHaveAttribute('data-project-id', 'copy-2');
  await expect(page.getByRole('button', { name: '2패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await projectAction(page, '목록에서 제거');
  await page.getByRole('button', { name: '제거하기', exact: true }).click();
  await expect(activeProject(page)).toHaveCount(0);
  await expect(projectTabs(page).getByRole('tab')).toHaveCount(2);
});

test('creates a project from an ephemeral folder without changing the previously saved project', async ({ page }) => {
  await installProjects(page, true);
  await page.goto('/');
  await projectAction(page, '새 프로젝트');
  await page.getByRole('textbox', { name: '프로젝트 이름', exact: true }).fill('Temporary project');
  await page.getByRole('textbox', { name: '루트 경로', exact: true }).fill('C:\\Temporary');
  await page.getByRole('button', { name: '만들기', exact: true }).click();
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'created-2');
  await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toHaveValue('C:\\Temporary');
  const saved: string[] = await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'save_project_session').map((call: { args: { request: { projectId: string } } }) => call.args.request.projectId));
  expect(saved).not.toContain('alpha');
});

test('close handshake flushes a pending session before acknowledging', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'alpha');
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p3Close')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').find((call: { command: string }) => call.command === 'project_close_ready')?.args)).toEqual({ attempt: 1, saved: true });
  const commands: string[] = await page.evaluate(() => Reflect.get(window, 'p3Calls').map((call: { command: string }) => call.command));
  expect(commands.indexOf('save_project_session')).toBeLessThan(commands.indexOf('project_close_ready'));
});

for (const width of [375, 768, 1280]) {
  test(`missing folder stays visible and layout is captured at ${width}px`, async ({ page }, testInfo) => {
    await installProjects(page);
    await page.setViewportSize({ width, height: 900 });
    await page.goto('/');
    await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toHaveValue('C:\\Alpha\\missing');
    await expect(page.getByRole('alert').filter({ hasText: '폴더를 찾을 수 없습니다' })).toBeVisible();
    await page.screenshot({ path: testInfo.outputPath(`p3-missing-${width}.png`) });
  });
}

test('recovered state blocks autosave and can be saved explicitly as a new project', async ({ page }) => {
  await installProjects(page, false, true);
  await page.goto('/');
  await page.locator('.project-manager > summary').click();
  await expect(page.getByRole('button', { name: '이름 변경', exact: true })).toBeDisabled();
  await page.locator('.project-manager > summary').click();
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await page.getByRole('button', { name: '현재 상태를 새 프로젝트로 저장', exact: true }).click();
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'created-2');
  await expect(page.getByRole('button', { name: '2패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
  const saved: string[] = await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'save_project_session').map((call: { args: { request: { projectId: string } } }) => call.args.request.projectId));
  expect(saved).not.toContain('alpha');
});

test('failed close flush reports unsaved and does not acknowledge success', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'alpha');
  await page.evaluate(() => Reflect.set(window, 'p3FailSave', true));
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p3Close')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').find((call: { command: string }) => call.command === 'project_close_ready')?.args)).toEqual({ attempt: 1, saved: false });
  await expect(page.getByRole('alert').filter({ hasText: '종료 전 저장을 완료하지 못했습니다' })).toBeVisible();
  await page.evaluate(() => Reflect.set(window, 'p3FailSave', false));
  await page.evaluate(() => Reflect.get(window, 'p3Close')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'project_close_ready').at(-1)?.args)).toEqual({ attempt: 2, saved: true });
});

test('a failed session flush prevents switching and retry saves the retained workspace', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  const select = activeProject(page);
  await expect(select).toHaveAttribute('data-project-id', 'alpha');
  await page.evaluate(() => Reflect.set(window, 'p3FailSave', true));
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await projectTabs(page).getByRole('tab', { name: 'Beta', exact: true }).click();
  await expect(select).toHaveAttribute('data-project-id', 'alpha');
  await expect(page.getByRole('button', { name: '2패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
  await page.evaluate(() => Reflect.set(window, 'p3FailSave', false));
  await page.getByRole('region', { name: '프로젝트 관리' }).getByRole('button', { name: '다시 시도', exact: true }).click();
  await expect(page.getByText('마지막 작업 상태 저장됨', { exact: true })).toBeVisible();
});

test('project controls isolate pane shortcuts and IME Enter echoes from project submission', async ({ page }) => {
  await installProjects(page);
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.getByRole('tab')).toHaveCount(1);
  await projectAction(page, '새 프로젝트');
  const name = page.getByRole('textbox', { name: '프로젝트 이름', exact: true });
  const root = page.getByRole('textbox', { name: '루트 경로', exact: true });
  await name.fill('IME project');
  await root.fill('C:\\New');
  const controls = [name, root, activeProject(page), page.getByRole('button', { name: '만들기', exact: true })];
  for (const control of controls) {
    await control.focus();
    for (const key of ['Control+t', 'Control+w', 'Control+l', 'F6']) {
      await control.press(key);
      await expect(control).toBeFocused();
      await expect(pane.getByRole('tab')).toHaveCount(1);
    }
  }
  await name.focus();
  await name.dispatchEvent('compositionstart', { data: '' });
  await name.dispatchEvent('keydown', { key: 'Process', code: 'Enter', keyCode: 229, isComposing: true, bubbles: true, cancelable: true });
  await name.dispatchEvent('compositionend', { data: '한글' });
  await name.press('Enter');
  expect(await page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'create_project').length)).toBe(0);
  await page.getByRole('button', { name: '만들기', exact: true }).click();
  await expect(activeProject(page)).toHaveAttribute('data-project-id', 'created-2');
});
