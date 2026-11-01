import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

const list = (page: Page) => page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
const calls = (page: Page, command: string) => page.evaluate(command => (Reflect.get(window, 'p4Calls') as { command: string; args: unknown }[]).filter(call => call.command === command), command);
test.beforeEach(async ({ page }) => {
  await installOperations(page, 3); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await list(page).getByRole('option').first().click();
});

test('reveal and Windows copy keep a validated selection; paste validates and submits directly', async ({ page }) => {
  await list(page).getByRole('option').first().click({ button: 'right' });
  await page.getByRole('menuitem', { name: '탐색기에서 선택 항목 보기', exact: true }).click();
  await expect.poll(async () => calls(page, 'export_to_explorer')).toHaveLength(1);
  await expect(page.locator('.transfer-status')).toContainText('탐색기에 선택 항목 표시를 요청했습니다');
  await list(page).press('Control+c');
  await expect.poll(async () => calls(page, 'export_to_explorer')).toHaveLength(2);
  await expect(page.locator('.transfer-status')).toContainText('Windows에 복사했습니다');
  const exports = await calls(page, 'export_to_explorer');
  expect(exports.map(call => (call.args as { action: string }).action)).toEqual(['reveal', 'clipboard']);
  await list(page).press('Control+v');
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect.poll(async () => calls(page, 'execute_operation')).toHaveLength(1);
  await expect(dialog).not.toBeVisible();
  expect(await calls(page, 'prepare_transfer')).toHaveLength(1);
});

test('native drop routes physical coordinates only to a ready list and submits one validated copy', async ({ page }) => {
  await page.evaluate(() => {
    const emit = Reflect.get(window, 'p3Emit');
    emit('tauri://drag-drop', { paths: ['C:\\External\\한글.txt'], position: { x: 1, y: 1 } });
  });
  expect(await calls(page, 'import_from_explorer')).toHaveLength(0);
  await list(page).evaluate(element => {
    const rect = element.getBoundingClientRect();
    Reflect.get(window, 'p3Emit')('tauri://drag-drop', { paths: ['C:\\External\\한글.txt'], position: { x: (rect.x + 30) * devicePixelRatio, y: (rect.y + 30) * devicePixelRatio } });
  });
  await expect.poll(async () => calls(page, 'execute_operation')).toHaveLength(1);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await calls(page, 'import_from_explorer')).toEqual([{ command: 'import_from_explorer', args: { paths: ['C:\\External\\한글.txt'] } }]);
  expect(await calls(page, 'execute_operation')).toHaveLength(1);
});

test('unsupported clipboard and IME input cannot submit files', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4RejectExternal', true));
  await list(page).press('Control+v');
  await expect(page.getByRole('alert')).toContainText('외부 잘라내기는 지원하지 않으므로 복사를 사용하세요');
  await expect(page.getByRole('dialog')).toHaveCount(0);
  await list(page).dispatchEvent('keydown', { key: 'v', ctrlKey: true, shiftKey: true, isComposing: true, keyCode: 229 });
  expect(await calls(page, 'import_from_explorer')).toHaveLength(1);
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('WSL drop event preserves exact paths and ignores malformed drops without clipboard fallback', async ({ page }) => {
  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await list(page).evaluate(element => {
    const rect = element.getBoundingClientRect();
    const position = { x: (rect.x + 30) * devicePixelRatio, y: (rect.y + 30) * devicePixelRatio };
    const emit = Reflect.get(window, 'p3Emit');
    for (const payload of [null, {}, { position }, { position, paths: [] }, { position, paths: [null] }]) emit('tauri://drag-drop', payload);
  });
  expect(await calls(page, 'import_from_explorer')).toHaveLength(0);
  const paths = [String.raw`\\?\UNC\wsl.localhost\Ubuntu\home\example-user\한글 space.txt`, String.raw`\\wsl$\Ubuntu\home\example-user\literal%20.txt`];
  await list(page).evaluate((element, paths) => {
    const rect = element.getBoundingClientRect();
    const position = { x: (rect.x + 30) * devicePixelRatio, y: (rect.y + 30) * devicePixelRatio };
    Reflect.get(window, 'p3Emit')('tauri://drag-drop', { paths, position });
  }, paths);
  await expect.poll(async () => calls(page, 'execute_operation')).toHaveLength(1);
  expect(await calls(page, 'import_from_explorer')).toEqual([{ command: 'import_from_explorer', args: { paths: [String.raw`\\wsl.localhost\Ubuntu\home\example-user\한글 space.txt`, paths[1]] } }]);
  expect(await calls(page, 'prepare_transfer')).toHaveLength(1);
  expect(errors).toEqual([]);
  await expect(page.getByRole('dialog')).toHaveCount(0);
});

test('leaving the app starts a Shell copy drag; Escape cancels an internal gesture', async ({ page }) => {
  const source = list(page).getByRole('option').first();
  await source.dispatchEvent('pointerdown', { button: 0, buttons: 1, pointerId: 9, clientX: 40, clientY: 300 });
  await page.locator('body').dispatchEvent('pointermove', { buttons: 1, pointerId: 9, clientX: 60, clientY: 300 });
  await source.dispatchEvent('pointerout', { buttons: 1, pointerId: 9, relatedTarget: null });
  await expect.poll(async () => calls(page, 'export_to_explorer')).toHaveLength(1);
  expect((await calls(page, 'export_to_explorer'))[0].args).toMatchObject({ action: 'drag' });
  await source.dispatchEvent('pointerdown', { button: 0, buttons: 1, pointerId: 10, clientX: 40, clientY: 300 });
  await page.keyboard.press('Escape');
  await source.dispatchEvent('pointerout', { buttons: 1, pointerId: 10, relatedTarget: null });
  expect(await calls(page, 'export_to_explorer')).toHaveLength(1);
});
