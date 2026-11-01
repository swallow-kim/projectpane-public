import { expect, test } from '@playwright/test';
import { installRaceListing } from './p2-race-fixture';

test('rapid Ctrl clicks accumulate cached selection before React commits', async ({ page }) => {
  await installRaceListing(page, 512);
  const pane = page.locator('[data-pane="0"]');
  await expect(pane.locator('[data-entry-id="item-0"]')).toHaveAttribute('aria-selected', 'true');
  await pane.evaluate(element => {
    for (const index of [1, 2, 3]) element.querySelector(`[data-entry-id="item-${index}"]`)!
      .dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, ctrlKey: true }));
  });
  await expect(pane.locator('.selection-status')).toHaveText('4개 선택');
  for (const index of [0, 1, 2, 3]) await expect(pane.locator(`[data-entry-id="item-${index}"]`)).toHaveAttribute('aria-selected', 'true');
});

test('keyboard movement accumulates before React commits cached selection', async ({ page }) => {
  await installRaceListing(page, 512);
  const list = page.getByRole('region', { name: '패널 1', exact: true }).getByRole('listbox');
  await expect(list.locator('[data-entry-id="item-0"]')).toHaveAttribute('aria-selected', 'true');
  await list.focus();
  await list.evaluate(element => {
    for (let index = 0; index < 12; index++) element.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
  });
  await expect(list.locator('[data-entry-id="item-12"]')).toHaveAttribute('aria-selected', 'true');
});

test('delayed page reads preserve the final keyboard target without flooding IPC', async ({ page }, testInfo) => {
  await installRaceListing(page, 512);
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await expect(list.locator('[data-entry-id="item-0"]')).toHaveAttribute('aria-selected', 'true');
  await page.evaluate(() => window.auditHold('0:read_listing:64'));
  await list.focus();
  await list.evaluate(element => {
    for (let index = 0; index < 160; index++) element.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown', bubbles: true, cancelable: true }));
  });
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:read_listing:64');
  await page.evaluate(() => window.auditRelease('0:read_listing:64'));
  await expect(list.locator('[data-entry-id="item-160"]')).toHaveAttribute('aria-selected', 'true');
  await expect(pane.locator('.selection-feedback')).toHaveCount(0);
  expect(await page.evaluate(() => window.auditCalls.filter(call => call.pane === 0 && call.command === 'read_listing').length)).toBeLessThan(10);
  await page.screenshot({ path: testInfo.outputPath('keyboard-final-target.png'), fullPage: true });
});

test('filter invalidates an in-flight keyboard selection even when that item survives the filter', async ({ page }) => {
  await installRaceListing(page, 512);
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await expect(list.locator('[data-entry-id="item-0"]')).toHaveAttribute('aria-selected', 'true');
  await page.evaluate(() => { window.auditHold('0:read_listing:448'); window.auditHold('0:set_listing_view:0'); });
  await list.focus();
  await page.keyboard.press('End');
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:read_listing:448');
  await page.keyboard.press('ArrowUp');
  await pane.getByRole('searchbox').fill('keep');
  await expect(pane.locator('.selection-status')).toHaveText('0개 선택');
  await expect(list).toHaveAttribute('aria-busy', 'true');
  await page.evaluate(() => window.auditRelease('0:read_listing:448'));
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:set_listing_view:0');
  await expect(pane.locator('.selection-status')).toHaveText('0개 선택');
  await page.evaluate(() => window.auditRelease('0:set_listing_view:0'));
  await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 256개');
  await expect(pane.locator('.selection-status')).toHaveText('0개 선택');
  await expect(pane.locator('.selection-feedback')).toHaveCount(0);
});
