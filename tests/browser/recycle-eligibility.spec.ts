import { expect, test } from '@playwright/test';
import { confirmRecycle, installRecycle, openRecycle, recycleButton, recycleCallCount, recycleDialog } from './p4-recycle-fixture';

test('folder recycle is enabled from toolbar and context menu with cancellable preview', async ({ page }) => {
  await installRecycle(page, { kind: 'folder' });
  await page.goto('/');
  const row = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first();
  await row.click();
  await expect(recycleButton(page)).toBeEnabled();
  await recycleButton(page).click();
  await expect(confirmRecycle(page)).toBeEnabled();
  await recycleDialog(page).getByRole('button', { name: '취소', exact: true }).click();
  await row.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '휴지통으로 이동', exact: false }).click();
  await expect(confirmRecycle(page)).toBeEnabled();
  await recycleDialog(page).getByRole('button', { name: '취소', exact: true }).click();
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(2);
});

for (const kind of ['link', 'unknown'] as const) {
  test(`known selected ${kind} cannot open recycle confirmation`, async ({ page }) => {
    await installRecycle(page, { kind });
    await page.goto('/');
    await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
    await expect(recycleButton(page)).toBeDisabled();
    expect(await recycleCallCount(page, 'prepare_operation')).toBe(0);
  });
}

test('Delete only previews an eligible file; Shift+Delete and empty selection do nothing', async ({ page }) => {
  await installRecycle(page);
  await page.goto('/');
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.getByRole('option').first().click();
  await expect(recycleButton(page)).toBeEnabled();
  await list.press('Shift+Delete');
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(0);
  await list.press('Delete');
  await expect(recycleDialog(page)).toBeVisible();
  await expect(confirmRecycle(page)).toBeEnabled();
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(1);
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  await recycleDialog(page).getByRole('button', { name: '취소', exact: true }).click();
  await list.getByRole('option').first().click({ modifiers: ['Control'] });
  await expect(recycleButton(page)).toBeDisabled();
  await list.press('Delete');
  await expect(recycleDialog(page)).toHaveCount(0);
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(1);
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
});

test('all virtualized selected source paths are inspectable in the native preview', async ({ page }, info) => {
  await installRecycle(page, { count: 80 });
  await page.goto('/');
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.getByRole('option').first().click();
  await list.press('Control+a');
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByText('80개 선택')).toBeVisible();
  await recycleButton(page).click();
  await expect(confirmRecycle(page)).toBeEnabled();
  const paths = recycleDialog(page).locator('.operation-paths dd > div');
  await expect(paths).toHaveCount(80);
  await expect(paths.first()).toHaveText('C:\\Native\\entry-0-검증.txt');
  await paths.last().scrollIntoViewIfNeeded();
  await expect(paths.last()).toHaveText('C:\\Native\\entry-79-검증.txt');
  await page.screenshot({ path: info.outputPath('recycle-all-paths.png') });
  const selection = await page.evaluate(() => window.recycle.calls.find(call => call.command === 'prepare_operation')?.args);
  expect(selection).toEqual({ request: { projectId: null, action: { kind: 'recycle', selection: { identity: expect.any(Object), entryIds: Array.from({ length: 80 }, (_, i) => `entry-${i}`) } } } });
});

test('selection beyond request limit cannot open recycle', async ({ page }) => {
  await installRecycle(page, { count: 1025 });
  await page.goto('/');
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.getByRole('option').first().click();
  await list.press('Control+a');
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByText('1,025개 선택')).toBeVisible();
  await expect(recycleButton(page)).toBeDisabled();
  expect(await recycleCallCount(page, 'prepare_operation')).toBe(0);
});

for (const width of [375, 768, 1280]) {
  test(`recycle confirmation is usable at ${width}px`, async ({ page }, info) => {
    await installRecycle(page);
    await openRecycle(page);
    await page.setViewportSize({ width, height: 900 });
    await expect(confirmRecycle(page)).toBeEnabled();
    await expect(recycleDialog(page)).toContainText('고정 드라이브의 파일과 폴더');
    const bounds = await recycleDialog(page).boundingBox();
    expect(bounds).not.toBeNull();
    expect(bounds?.x).toBeGreaterThanOrEqual(0);
    expect((bounds?.x ?? 0) + (bounds?.width ?? 0)).toBeLessThanOrEqual(width);
    await page.screenshot({ path: info.outputPath(`recycle-${width}.png`) });
  });
}
