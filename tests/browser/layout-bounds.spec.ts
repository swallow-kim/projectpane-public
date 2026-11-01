import { expect, test } from '@playwright/test';
import { installRaceListing } from './p2-race-fixture';

test('small four-pane layouts retain at least three complete file rows', async ({ page }, info) => {
  await installRaceListing(page, 192);
  for (const viewport of [{ width: 1280, height: 900 }, { width: 960, height: 650 }, { width: 768, height: 900 }]) {
    await page.setViewportSize(viewport);
    const bounds = await page.getByRole('region', { name: '패널 1', exact: true }).evaluate(pane =>
      Object.fromEntries(['.tabs', '.navigation-toolbar', '.view-toolbar', '.file-actions', '.listing-table', '.column-heading', '.file-list', 'footer'].map(selector => {
        const el = pane.querySelector(selector) as HTMLElement | null;
        return [selector, el ? { height: el.clientHeight, width: el.clientWidth, scrollHeight: el.scrollHeight, rect: el.getBoundingClientRect().toJSON() } : null];
      })));
    await info.attach(`bounds-${viewport.width}`, { body: JSON.stringify(bounds, null, 2), contentType: 'application/json' });
    await expect.poll(() => page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).evaluate(el => el.clientHeight)).toBeGreaterThanOrEqual(90);
  }
});
