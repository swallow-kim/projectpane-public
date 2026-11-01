import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

for (const width of [1200, 800, 480]) test(`two header rows keep file space and accessible tools at ${width}px`, async ({ page }, info) => {
  await page.setViewportSize({ width, height: 800 });
  await installOperations(page, 600);
  await page.goto('/');
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  const pane = page.locator('[data-pane="0"]');
  await expect(pane.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  const geometry = await pane.evaluate(element => {
    const rect = (selector: string) => element.querySelector(selector)!.getBoundingClientRect();
    return { header: rect('.pane-header').height, tools: rect('.pane-tools').height, listTop: rect('.file-list').y, listHeight: rect('.file-list').height,
      tabY: rect('.tabs').y, navY: rect('.navigation-toolbar').y, filterY: rect('.view-toolbar').y, actionsY: rect('.file-actions').y,
      row: rect('.file-row').height, overflow: document.documentElement.scrollWidth > innerWidth };
  });
  expect(Math.abs(geometry.tabY - geometry.navY)).toBeLessThanOrEqual(2);
  expect(Math.abs(geometry.filterY - geometry.actionsY)).toBeLessThanOrEqual(2);
  expect(geometry.header).toBeLessThanOrEqual(48);
  expect(geometry.tools).toBeLessThanOrEqual(48);
  expect(geometry.listHeight).toBeGreaterThan(490);
  expect(geometry.row).toBe(26);
  expect(geometry.overflow).toBe(false);
  const actions = pane.locator('.file-actions button');
  for (const action of await actions.all()) { await expect(action).toHaveAccessibleName(/.+/); await expect(action).toHaveAttribute('title', /.+/); }
  await pane.getByRole('button', { name: '패널 1 새 폴더', exact: true }).focus();
  await page.keyboard.press('Enter');
  const modal = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await expect(modal).toBeVisible();
  await modal.getByRole('button', { name: '취소', exact: true }).click();
  await pane.locator('.file-list').focus();
  await page.keyboard.press('End');
  await expect(pane.locator('.file-row[data-focused=true]')).toContainText('599');
  await page.screenshot({ path: info.outputPath(`density-${width}.png`) });
});

for (const deviceScaleFactor of [1.5, 2]) test.describe(`scaled rendering ${deviceScaleFactor}`, () => {
  test.use({ deviceScaleFactor });
  test('compact controls keep CSS hit areas and full editable paths', async ({ page }, info) => {
    await page.setViewportSize({ width: 800, height: 600 });
    await installOperations(page, 128);
    await page.goto('/');
    await page.getByRole('button', { name: '2패널', exact: true }).click();
    const pane = page.locator('[data-pane="0"]');
    const address = pane.getByRole('textbox', { name: '패널 1 주소', exact: true });
    await address.fill('C:\\Temporary\\' + '긴 폴더 이름\\'.repeat(10));
    await expect(address).toHaveValue('C:\\Temporary\\' + '긴 폴더 이름\\'.repeat(10));
    await address.press('Control+a');
    expect(await address.evaluate((element: HTMLInputElement) => element.selectionEnd! - element.selectionStart!)).toBeGreaterThan(80);
    const area = await pane.locator('.file-actions button').first().boundingBox();
    expect(area?.width).toBeGreaterThanOrEqual(24); expect(area?.height).toBeGreaterThanOrEqual(24);
    expect(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth)).toBe(false);
    await page.screenshot({ path: info.outputPath(`scale-${deviceScaleFactor}.png`) });
  });
});

test('forced colors preserves selected row and visible keyboard focus', async ({ page }, info) => {
  await page.emulateMedia({ forcedColors: 'active' });
  await installOperations(page, 128);
  await page.goto('/');
  const pane = page.locator('[data-pane="0"]');
  await pane.locator('.file-list').press('Home');
  const selected = pane.locator('.file-row[data-focused=true]');
  await expect(selected).toHaveAttribute('aria-selected', 'true');
  const style = await selected.evaluate(element => ({ color: getComputedStyle(element).color, bg: getComputedStyle(element).backgroundColor, outline: getComputedStyle(element).outlineStyle }));
  expect(style.color).not.toBe(style.bg); expect(style.outline).not.toBe('none');
  await selected.hover();
  expect(await selected.evaluate(element => ({ color: getComputedStyle(element).color, bg: getComputedStyle(element).backgroundColor, outline: getComputedStyle(element).outlineStyle }))).toEqual(style);
  await page.screenshot({ path: info.outputPath('forced-colors.png') });
});
