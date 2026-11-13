import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test.beforeEach(async ({ page }) => { await installOperations(page, 3); await page.goto('/'); });

test('all four columns resize, align with rows and persist without changing selection', async ({ page }) => {
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await expect(list.getByRole('option').first()).toBeVisible();
  const selected = await list.getByRole('option', { selected: true }).count();
  for (const label of ['이름', '유형', '크기', '수정한 날짜']) {
    const handle = pane.getByRole('separator', { name: `${label} 열 너비`, exact: true });
    await handle.scrollIntoViewIfNeeded();
    const box = await handle.boundingBox(); if (!box) throw new Error('resize handle required');
    const before = await handle.locator('..').evaluate(el => el.getBoundingClientRect().width);
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2); await page.mouse.down();
    await page.mouse.move(box.x + box.width / 2 + 50, box.y + box.height / 2, { steps: 5 }); await page.mouse.up();
    await expect(handle).toHaveAttribute('aria-valuenow', String(Math.round(before + 50)));
  }
  const saved = await page.evaluate(() => localStorage.getItem('projectpane.columns.v1.0'));
  expect(saved).not.toBeNull();
  expect(await list.getByRole('option', { selected: true }).count()).toBe(selected);
  const aligned = await pane.evaluate(el => {
    const headings = [...el.querySelectorAll('.column-label')];
    const cells = [...el.querySelector('.file-row')!.children];
    return headings.every((heading, i) => Math.abs(heading.getBoundingClientRect().x - cells[i].getBoundingClientRect().x) <= 1);
  });
  expect(aligned).toBe(true);
  await page.reload();
  await expect(pane.getByRole('separator', { name: '유형 열 너비', exact: true })).toHaveAttribute('aria-valuenow', '130');
  expect(await page.evaluate(() => localStorage.getItem('projectpane.columns.v1.0'))).toBe(saved);
});

test('keyboard bounds, reset, Escape and pointer cancellation retain usable columns', async ({ page }) => {
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const handle = pane.getByRole('separator', { name: '유형 열 너비', exact: true });
  await handle.focus(); await handle.press('ArrowRight'); await expect(handle).toHaveAttribute('aria-valuenow', '90');
  await handle.press('Shift+ArrowLeft'); await expect(handle).toHaveAttribute('aria-valuenow', '89');
  await handle.press('Home'); await expect(handle).toHaveAttribute('aria-valuenow', '56');
  await handle.dblclick(); await expect(handle).toHaveAttribute('aria-valuenow', '80');
  for (const cancel of ['Escape', 'pointercancel']) {
    const box = await handle.boundingBox(); if (!box) throw new Error('handle required');
    await page.mouse.move(box.x + 5, box.y + 5); await page.mouse.down(); await page.mouse.move(box.x + 60, box.y + 5);
    if (cancel === 'Escape') await page.keyboard.press('Escape'); else await handle.dispatchEvent('pointercancel', { pointerId: 1 });
    await page.mouse.up(); await expect(handle).toHaveAttribute('aria-valuenow', '80');
  }
  await expect(pane.getByRole('listbox').getByRole('option').first()).toBeVisible();
});
