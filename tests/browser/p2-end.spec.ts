import { expect, test, type Locator } from '@playwright/test';
import { installRaceListing } from './p2-race-fixture';

async function expectLastRowVisible(last: Locator) {
  await expect(last).toHaveAttribute('data-entry-id', 'item-99999');
  await expect(last).toHaveAttribute('aria-setsize', '100000');
  await expect(last).toContainText('99999-other.txt');
  await expect(last).toHaveAttribute('aria-selected', 'true');
  await expect.poll(() => last.evaluate(row => {
    const viewport = row.closest('[role="listbox"]')?.getBoundingClientRect();
    const bounds = row.getBoundingClientRect();
    return row.isConnected && !!viewport && bounds.top >= viewport.top - 1 && bounds.bottom <= viewport.bottom + 1;
  })).toBe(true);
}

test('End at listing completion keeps the last real row visible', async ({ page }, testInfo) => {
  await page.addInitScript(() => {
    const observer = new MutationObserver(() => {
      const pane = document.querySelector('section[aria-label="패널 1"]');
      const list = pane?.querySelector('[role="listbox"]');
      if (!(list instanceof HTMLElement) || !pane?.querySelector('footer [role="status"]')?.textContent?.includes('조회 완료 · 100,000개')) return;
      observer.disconnect();
      const writes: { top: number; stack: string | undefined }[] = [];
      const descriptor = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTop');
      if (!descriptor?.get || !descriptor.set) throw new Error('Missing scrollTop descriptor');
      Object.defineProperty(list, 'scrollTop', {
        get: () => descriptor.get?.call(list),
        set: (top: number) => { writes.push({ top, stack: new Error().stack }); descriptor.set?.call(list, top); },
        configurable: true,
      });
      list.focus();
      const event = new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true });
      list.dispatchEvent(event);
      Object.assign(window, { endEvidence: { connected: list.isConnected, busy: list.getAttribute('aria-busy'),
        count: list.querySelector('[aria-setsize]')?.getAttribute('aria-setsize'), prevented: event.defaultPrevented,
        immediateTop: list.scrollTop, writes } });
    });
    observer.observe(document, { childList: true, subtree: true, characterData: true });
  });
  await installRaceListing(page, 100_000);
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  try {
    const last = pane.locator('[aria-posinset="100000"]');
    await expectLastRowVisible(last);
    for (const viewport of [{ width: 960, height: 650 }, { width: 768, height: 900 }]) {
      await page.setViewportSize(viewport);
      await pane.getByRole('listbox').press('End');
      await expectLastRowVisible(last);
      await testInfo.attach(`bounds-${viewport.width}x${viewport.height}`, {
        body: JSON.stringify(await last.evaluate(row => {
          const list = row.closest('[role="listbox"]');
          return { connected: row.isConnected, id: row.getAttribute('data-entry-id'), count: row.getAttribute('aria-setsize'),
            row: row.getBoundingClientRect().toJSON(), viewport: list?.getBoundingClientRect().toJSON(),
            scrollTop: list?.scrollTop, scrollHeight: list?.scrollHeight, clientHeight: list?.clientHeight };
        }), null, 2), contentType: 'application/json',
      });
      await page.screenshot({ path: testInfo.outputPath(`responsive-${viewport.width}x${viewport.height}.png`) });
    }
  } finally {
    await testInfo.attach('end-evidence', { body: JSON.stringify(await page.evaluate(() => Reflect.get(window, 'endEvidence')), null, 2), contentType: 'application/json' });
    await testInfo.attach('served-scripts', { body: JSON.stringify(await page.evaluate(() => Array.from(document.scripts, script => script.src))), contentType: 'application/json' });
    await page.screenshot({ path: testInfo.outputPath('end.png') });
  }
});

test('End interrupts a pending anchor lookup after refreshing a real list', async ({ page }, testInfo) => {
  await installRaceListing(page, 100_000);
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await pane.locator('[data-entry-id="item-2"]').click();
  await expect(pane.locator('[data-entry-id="item-2"]')).toHaveAttribute('aria-selected', 'true');
  await page.evaluate(() => { window.auditHold('0:start_listing:0'); window.auditHold('0:locate_entry:0'); });
  await pane.getByRole('button', { name: '새로 고침', exact: true }).click();
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:start_listing:0');
  await expect(list).toHaveAttribute('aria-busy', 'true');
  await page.evaluate(() => window.auditRelease('0:start_listing:0'));
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:locate_entry:0');
  await list.press('End');
  const last = pane.locator('[aria-posinset="100000"]');
  expect(await list.evaluate(element => element.scrollTop)).toBeGreaterThan(2_599_000);
  await page.evaluate(() => window.auditRelease('0:locate_entry:0'));
  await expect.poll(() => page.evaluate(() => window.auditCalls.filter(call => call.pane === 0 && call.command === 'locate_entry').length)).toBeGreaterThanOrEqual(2);
  try { await expectLastRowVisible(last); }
  finally {
    await testInfo.attach('pending-anchor-bounds', { body: JSON.stringify(await last.evaluate(row => {
      const list = row.closest('[role="listbox"]')!;
      return { row: row.getBoundingClientRect().toJSON(), viewport: list.getBoundingClientRect().toJSON(),
        top: list.scrollTop, clientHeight: list.clientHeight, scrollHeight: list.scrollHeight,
        feedback: list.closest('section')?.querySelector('.selection-feedback')?.textContent };
    })), contentType: 'application/json' });
  }
});

test('End remains visible after a late toolbar resize without pinning a middle scroll', async ({ page }) => {
  await installRaceListing(page, 100_000);
  const pane = page.locator('[data-pane="0"]');
  const list = pane.getByRole('listbox');
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await list.press('End');
  const last = pane.locator('[aria-posinset="100000"]');
  await expectLastRowVisible(last);
  await pane.locator('.pane-tools').evaluate(element => { element.style.paddingBottom = '30px'; });
  await expectLastRowVisible(last);
  await list.evaluate(element => { element.scrollTop = 13000; });
  await expect.poll(() => list.evaluate(element => element.scrollTop)).toBe(13000);
  await pane.locator('.pane-tools').evaluate(element => { element.style.paddingBottom = '45px'; });
  await expect.poll(() => list.evaluate(element => element.scrollTop)).toBe(13000);
});

test('focus revalidation preserves an End selection whose last page is still loading', async ({ page }) => {
  await installRaceListing(page, 100_000);
  const pane = page.locator('[data-pane="0"]');
  const list = pane.getByRole('listbox');
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await pane.locator('[data-entry-id="item-2"]').click();
  await page.evaluate(() => window.auditHold('0:read_listing:99968'));
  await list.press('End');
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:read_listing:99968');
  const starts = () => page.evaluate(() => window.auditCalls.filter(call => call.pane === 0 && call.command === 'start_listing').length);
  const before = await starts();
  await page.evaluate(() => {
    window.dispatchEvent(new FocusEvent('blur'));
    window.dispatchEvent(new FocusEvent('focus'));
  });
  // Cross the existing 200 ms focus debounce while the selection IPC is held.
  await page.waitForTimeout(300);
  expect(await starts()).toBe(before);
  await page.evaluate(() => window.auditRelease('0:read_listing:99968'));
  await expect.poll(starts).toBe(before + 1);
  await expectLastRowVisible(pane.locator('[aria-posinset="100000"]'));
});

test('a queued focus refresh is discarded when its folder changes during selection', async ({ page }) => {
  await installRaceListing(page, 100_000);
  const pane = page.locator('[data-pane="0"]');
  const list = pane.getByRole('listbox');
  await page.evaluate(() => window.auditHold('0:read_listing:99968'));
  await list.press('End');
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:read_listing:99968');
  await page.evaluate(() => {
    window.dispatchEvent(new FocusEvent('blur'));
    window.dispatchEvent(new FocusEvent('focus'));
  });
  await page.waitForTimeout(300);
  const starts = () => page.evaluate(() => window.auditCalls.filter(call => call.pane === 0 && call.command === 'start_listing').length);
  const before = await starts();
  const address = pane.locator('.navigation-toolbar input');
  await address.fill('/fixture/new-location'); await address.press('Enter');
  await page.evaluate(() => window.auditRelease('0:read_listing:99968'));
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await expect.poll(starts).toBe(before + 1);
  await page.waitForTimeout(300);
  expect(await starts()).toBe(before + 1);
  await expect(address).toHaveValue('/fixture/new-location');
  await expect(pane.locator('[aria-posinset="1"]')).toBeVisible();
});

test('cancelling a pending selection releases its deferred focus refresh', async ({ page }) => {
  await installRaceListing(page, 100_000);
  const pane = page.locator('[data-pane="0"]');
  const list = pane.getByRole('listbox');
  await pane.locator('[data-entry-id="item-2"]').click();
  await page.evaluate(() => window.auditHold('0:read_listing:99968'));
  await list.press('End');
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:read_listing:99968');
  await page.evaluate(() => {
    window.dispatchEvent(new FocusEvent('blur'));
    window.dispatchEvent(new FocusEvent('focus'));
  });
  await page.waitForTimeout(300);
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await list.press('Escape');
  await expect(list).toHaveAttribute('aria-busy', 'true');
  await page.evaluate(() => window.auditRelease('0:read_listing:99968'));
  await expect(list).toHaveAttribute('aria-busy', 'false');
  // Cancel keeps the previous selection, but does not undo the already-applied
  // scroll. Reveal the beginning without changing selection before checking it.
  await list.press('Control+Home');
  await expect(pane.locator('[data-entry-id="item-2"]')).toHaveAttribute('aria-selected', 'true');
});
