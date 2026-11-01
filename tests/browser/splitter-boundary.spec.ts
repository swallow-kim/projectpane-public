import { expect, test } from '@playwright/test';

for (const width of [800, 1200]) for (const layout of [2, 4]) {
  test(`joined ${layout} panels at ${width}px keep neutral resize boundaries`, async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await page.goto('/');
    await page.getByRole('button', { name: `${layout}패널`, exact: true }).click();
    const panes = page.locator('.panes');
    const geometry = await panes.evaluate(element => {
      const rect = (i: number) => element.querySelector(`[data-pane="${i}"]`)!.getBoundingClientRect();
      return { gapX: rect(1).left - rect(0).right, gapY: rect(2).top - rect(0).bottom };
    });
    expect(Math.abs(geometry.gapX)).toBeLessThanOrEqual(1);
    if (layout === 4) expect(Math.abs(geometry.gapY)).toBeLessThanOrEqual(1);
    const columns = page.getByRole('separator', { name: '좌우 패널 비율' });
    const quiet = () => columns.evaluate(element => ({ background: getComputedStyle(element).backgroundColor,
      after: getComputedStyle(element, '::after').content, outline: getComputedStyle(element).outlineStyle }));
    const initial = await quiet();
    const bounds = (await columns.boundingBox())!;
    expect(bounds.width).toBe(6);
    for (const delta of [-12, 0, 12, 0, -12, 0]) await page.mouse.move(bounds.x + bounds.width / 2 + delta, bounds.y + 30);
    expect(await quiet()).toEqual(initial);
    expect(initial.background).toBe('rgba(0, 0, 0, 0)');
    expect(initial.after).toBe('none');
    await expect(columns).toHaveCSS('cursor', 'col-resize');
    await columns.focus();
    await page.keyboard.press('ArrowRight');
    await expect(columns).toHaveCSS('outline-style', 'solid');
    await page.keyboard.press('End');
    await expect(columns).toHaveAttribute('aria-valuenow', '80');
    await page.keyboard.press('Home');
    await expect(columns).toHaveAttribute('aria-valuenow', '20');
    const grid = (await panes.boundingBox())!;
    const grip = (await columns.boundingBox())!;
    await page.mouse.move(grip.x + 3, grip.y + 40); await page.mouse.down();
    await page.mouse.move(grid.x + grid.width / 2, grip.y + 40); await page.mouse.up();
    await expect(columns).toHaveAttribute('aria-valuenow', '50');
    await page.mouse.move(grid.x + grid.width * .7, grip.y + 40);
    await expect(columns).toHaveAttribute('aria-valuenow', '50');
    if (layout === 4) {
      const rows = page.getByRole('separator', { name: '상하 패널 비율' });
      await expect(rows).toHaveCSS('cursor', 'row-resize');
      expect((await rows.boundingBox())!.height).toBe(6);
      await rows.focus(); await page.keyboard.press('Home'); await page.keyboard.press('Shift+ArrowDown');
      await expect(rows).toHaveAttribute('aria-valuenow', '30');
    }
  });
}

test('resize cancellation and button loss cannot keep changing the ratio', async ({ page }) => {
  await page.goto('/');
  const splitter = page.getByRole('separator', { name: '좌우 패널 비율' });
  await splitter.evaluate(element => element.addEventListener('pointerdown', event => { element.dataset.pointerId = String((event as PointerEvent).pointerId); }));
  const grid = (await page.locator('.panes').boundingBox())!;
  for (const cancel of ['pointercancel', 'buttons', 'Escape']) {
    const rect = (await splitter.boundingBox())!;
    await page.mouse.move(rect.x + 3, rect.y + 40); await page.mouse.down();
    await page.mouse.move(grid.x + grid.width * .6, rect.y + 40);
    const value = await splitter.getAttribute('aria-valuenow');
    const pointerId = Number(await splitter.getAttribute('data-pointer-id'));
    if (cancel === 'Escape') { await splitter.focus(); await page.keyboard.press('Escape'); }
    else await splitter.dispatchEvent(cancel === 'buttons' ? 'pointermove' : cancel,
      { pointerId, buttons: 0, clientX: grid.x + grid.width * .8, clientY: rect.y + 40 });
    await page.mouse.move(grid.x + grid.width * .3, rect.y + 40); await page.mouse.up();
    await expect(splitter).toHaveAttribute('aria-valuenow', value!);
  }
});
