import { expect, test, type Locator } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function appearance(pane: Locator, indexes: number[]) {
  const selected = pane.locator('.file-row[aria-selected=true]');
  await expect(selected).toHaveCount(indexes.length);
  expect(await selected.evaluateAll(rows => rows.map(row => [...row.parentElement!.children].indexOf(row)))).toEqual(indexes);
  const values = await selected.evaluateAll(rows => rows.map(row => {
    const css = getComputedStyle(row);
    return { background: css.backgroundColor, shadow: css.boxShadow, color: css.color };
  }));
  expect(new Set(values.map(v => v.background)).size).toBe(1);
  for (const value of values) {
    expect(value.background).toBe('rgb(53, 92, 115)');
    expect(value.shadow).toContain('inset');
    expect(value.color).toBe('rgb(231, 233, 237)');
  }
  await expect(pane.locator('.file-row[aria-selected=false]').first()).not.toHaveCSS('background-color', values[0].background);
}

test('all selected rows stay distinct with Ctrl held released hovered inactive and refreshed', async ({ page }, info) => {
  await installOperations(page, 20); await page.goto('/');
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  const pane = page.locator('[data-pane="0"]'), row = (i: number) => pane.locator('.file-row').nth(i);
  await row(0).click(); await page.keyboard.down('Control'); await row(2).click();
  await appearance(pane, [0, 2]);
  await page.keyboard.up('Control'); await appearance(pane, [0, 2]);
  await row(4).hover(); await appearance(pane, [0, 2]);
  await page.locator('[data-pane="1"] .file-list').focus(); await appearance(pane, [0, 2]);
  await page.screenshot({ path: info.outputPath('inactive-multiple.png') });
  await pane.getByRole('button', { name: '새로 고침', exact: true }).click();
  await expect(pane.getByRole('listbox')).toHaveAttribute('aria-busy', 'false'); await appearance(pane, [0, 2]);
});

test('Shift range highlights rows without browser text selection and separates keyboard focus', async ({ page }, info) => {
  await installOperations(page, 20); await page.goto('/');
  const pane = page.locator('[data-pane="0"]'), row = (i: number) => pane.locator('.file-row').nth(i);
  await row(0).click(); await row(5).click({ modifiers: ['Shift'] });
  await expect(pane.locator('.selection-status')).toHaveText('6개 선택');
  expect(await page.evaluate(() => getSelection()?.toString())).toBe('');
  await appearance(pane, [0, 1, 2, 3, 4, 5]);
  await pane.getByRole('listbox').focus();
  await expect(pane.locator('.file-row[data-focused=true]')).toHaveCSS('outline-style', 'solid');
  await expect(row(0)).toHaveCSS('outline-style', 'none');
  await page.screenshot({ path: info.outputPath('shift-range.png') });
});

test('Escape during a drag preserves multiple selection after button release', async ({ page }) => {
  await installOperations(page, 20); await page.goto('/');
  // Read coordinates only after the fixture's missing-watch notice is laid out.
  await expect(page.getByText('자동 갱신을 사용할 수 없습니다.', { exact: false })).toBeVisible();
  const pane = page.locator('[data-pane="0"]'), row = (i: number) => pane.locator('.file-row').nth(i);
  await row(1).click(); await row(3).click({ modifiers: ['Control'] });
  const bounds = (await row(1).boundingBox())!;
  await page.mouse.move(bounds.x + 60, bounds.y + 12); await page.mouse.down();
  await page.mouse.move(bounds.x + 75, bounds.y + 12);
  await expect(page.locator('body')).toHaveClass(/file-pointer-drag/);
  await appearance(pane, [1, 3]);
  await page.keyboard.press('Escape');
  await expect(page.locator('body')).not.toHaveClass(/file-pointer-drag/);
  await page.mouse.move(bounds.x + 90, bounds.y + 12); await page.mouse.up();
  await appearance(pane, [1, 3]);
  await expect(page.getByRole('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => (Reflect.get(window, 'p4Calls') as { command: string }[])
    .filter(call => ['capture_transfer', 'execute_operation', 'native_export'].includes(call.command)))).toHaveLength(0);
  await row(4).click(); await appearance(pane, [4]);
});
