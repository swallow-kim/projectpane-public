import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

const list = (page: Page, pane = 1) => page.getByRole('listbox', { name: `패널 ${pane} 파일 목록`, exact: true });
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === command), command);
test.beforeEach(async ({ page }) => {
  await installOperations(page, 3); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled();
});

test('F2 and Delete open explicit confirmations; Shift+Delete never submits a deletion', async ({ page }) => {
  await list(page).press('F2');
  const rename = page.getByRole('dialog', { name: '이름 변경', exact: true });
  await expect(rename).toBeVisible();
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
  await rename.getByRole('button', { name: '취소', exact: true }).click();
  await list(page).press('Shift+Delete'); await expect(page.getByRole('dialog')).toHaveCount(0);
  await list(page).press('Delete');
  await expect(page.getByRole('dialog', { name: '휴지통', exact: true })).toBeVisible();
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});

test('properties uses one validated item and Explorer reveal preserves multiple selected items', async ({ page }) => {
  await list(page).press('Alt+Enter');
  await expect.poll(async () => calls(page, 'export_to_explorer')).toHaveLength(1);
  expect((await calls(page, 'export_to_explorer'))[0].args).toMatchObject({ action: 'properties', selection: { entryIds: ['entry-0'] } });
  await expect(page.locator('.transfer-status')).toContainText('Windows 속성 창을 요청했습니다');
  await list(page).getByRole('option').nth(1).click({ modifiers: ['Control'] });
  await expect(list(page).getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true');
  await list(page).getByRole('option').first().click({ button: 'right' });
  await expect(page.getByRole('menuitem', { name: 'Windows 속성', exact: true })).toBeDisabled();
  await page.getByRole('menuitem', { name: '탐색기에서 선택 항목 보기', exact: true }).click();
  await expect.poll(async () => calls(page, 'export_to_explorer')).toHaveLength(2);
  expect((await calls(page, 'export_to_explorer'))[1].args).toMatchObject({ action: 'reveal', selection: { entryIds: ['entry-0', 'entry-1'] } });
});

test('dragging an unselected row selects that source and highlights the copy destination', async ({ page }) => {
  await page.getByRole('region', { name: '패널 2', exact: true }).locator('[role="tab"][data-source="real"]').click();
  const address = page.getByRole('textbox', { name: '패널 2 주소', exact: true });
  await address.fill('C:\\Target'); await address.press('Enter');
  await expect(page.getByRole('button', { name: '패널 2 새 폴더', exact: true })).toBeEnabled();
  const source = list(page).getByRole('option').nth(1);
  await expect(source).toHaveAttribute('aria-selected', 'false');
  const a = await source.boundingBox(), b = await list(page, 2).boundingBox();
  if (!a || !b) throw new Error('visible source and target required');
  await source.dispatchEvent('pointerdown', { button: 0, buttons: 1, pointerId: 31, clientX: a.x + 30, clientY: a.y + 10 });
  await page.locator('body').dispatchEvent('pointermove', { buttons: 1, pointerId: 31, clientX: b.x + 40, clientY: b.y + 40 });
  await expect(source).toHaveAttribute('aria-selected', 'true');
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-selected', 'false');
  await expect(list(page, 2)).toHaveAttribute('data-drop-effect', 'copy');
  await expect(page.locator('.file-drop-hint')).toHaveText('놓으면 여기에 복사합니다');
  await page.locator('body').dispatchEvent('pointerup', { button: 0, buttons: 0, pointerId: 31, clientX: b.x + 40, clientY: b.y + 40 });
  await expect.poll(async () => calls(page, 'begin_copy_job')).toHaveLength(1);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect((await calls(page, 'append_copy_capture'))[0].args.request).toMatchObject({ selection: { entryIds: ['entry-1'] } });
  expect(await calls(page, 'begin_copy_job')).toHaveLength(1);
  await expect(page.locator('.file-drop-hint')).toHaveCount(0);
});

test('native drag hover and leave change no files and cannot target a toolbar', async ({ page }) => {
  await list(page).evaluate(element => {
    const r = element.getBoundingClientRect();
    Reflect.get(window, 'p3Emit')('tauri://drag-enter', { paths: ['C:\\External\\synthetic.txt'], position: { x: (r.x + 30) * devicePixelRatio, y: (r.y + 30) * devicePixelRatio } });
  });
  await expect(list(page)).toHaveAttribute('data-drop-effect', 'copy');
  expect(await calls(page, 'capture_copy_clipboard')).toHaveLength(0);
  await page.evaluate(() => Reflect.get(window, 'p3Emit')('tauri://drag-over', { position: { x: 1, y: 1 } }));
  await expect(page.locator('.file-drop-hint')).toHaveCount(0);
  await list(page).evaluate(element => {
    const r = element.getBoundingClientRect();
    Reflect.get(window, 'p3Emit')('tauri://drag-over', { position: { x: (r.x + 30) * devicePixelRatio, y: (r.y + 30) * devicePixelRatio } });
  });
  await expect(page.locator('.file-drop-hint')).toBeVisible();
  await page.evaluate(() => Reflect.get(window, 'p3Emit')('tauri://drag-leave', {}));
  await expect(page.locator('.file-drop-hint')).toHaveCount(0);
  await expect(list(page)).not.toHaveAttribute('data-drop-effect');
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
});

test('pointer cancellation clears the target and a released pointer cannot start Shell dragging', async ({ page }) => {
  const source = list(page).getByRole('option').first();
  const r = await source.boundingBox(); if (!r) throw new Error('visible source required');
  await source.dispatchEvent('pointerdown', { button: 0, buttons: 1, pointerId: 32, clientX: r.x + 20, clientY: r.y + 5 });
  await source.dispatchEvent('pointermove', { buttons: 1, pointerId: 32, clientX: r.x + 40, clientY: r.y + 5 });
  await expect(page.locator('.file-drop-hint')).toBeVisible();
  await source.dispatchEvent('pointercancel', { pointerId: 33 });
  await expect(page.locator('.file-drop-hint')).toBeVisible();
  await source.dispatchEvent('pointerout', { buttons: 0, pointerId: 32, relatedTarget: null });
  await expect(page.locator('.file-drop-hint')).toHaveCount(0);
  expect(await calls(page, 'export_copy_capture')).toHaveLength(0);
  expect(await calls(page, 'append_copy_capture')).toHaveLength(0);
});

test('Escape clears an internal cut without reading or changing the Windows clipboard', async ({ page }) => {
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  await list(page).press('Escape');
  await expect(page.locator('.transfer-status')).toContainText('앱 보관을 비웠습니다');
  await expect(page.getByRole('button', { name: '앱에 붙여넣기', exact: true }).first()).toBeDisabled();
  expect(await calls(page, 'export_copy_capture')).toHaveLength(0);
  expect(await calls(page, 'capture_copy_clipboard')).toHaveLength(0);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
});
