import { expect, test, type Page } from '@playwright/test';
import type { Entry, Identity } from '../../src/shared/contracts';
import type { ListingView } from '../../src/shared/listing-view';

type Call = { command: string; entryId?: string; offset?: number | null; view?: ListingView };
declare global {
  interface Window {
    p2Calls: Call[];
    p2RemoveFocused: () => void;
    p2Shrink: (count: number) => void;
    p2ObserveOldRows: () => void;
    p2OldRows: string[];
    p2HoldPage64: () => void;
    p2ReleasePage64: () => void;
    p2ScrollEvents: number;
  }
}

async function installListing(page: Page) {
  await page.addInitScript(() => {
    const entries: Entry[] = Array.from({ length: 192 }, (_, index) => ({
      id: `item-${index}`, name: `${String(index).padStart(3, '0')}-${index % 2 ? 'other' : 'keep'}.txt`,
      kind: 'file', modifiedAtMs: index * 1000, sizeBytes: 192 - index,
    }));
    let visible = entries.slice();
    let revision = 0;
    let removed = false;
    let heldPage: Promise<void> | undefined;
    let releasePage: (() => void) | undefined;
    window.p2HoldPage64 = () => { heldPage = new Promise<void>(resolve => { releasePage = resolve; }); };
    window.p2ReleasePage64 = () => { releasePage?.(); };
    window.p2ScrollEvents = 0;
    window.p2Calls = [];
    window.p2OldRows = [];
    window.p2RemoveFocused = () => { removed = true; };
    window.p2Shrink = count => { entries.splice(count); visible = entries.slice(); };
    window.p2ObserveOldRows = () => {
      const list = document.querySelector('[aria-label="패널 1 파일 목록"]');
      if (!list) throw new Error('Missing list');
      new MutationObserver(() => {
        for (const row of list.querySelectorAll('[data-entry-id]')) {
          const index = Number(row.getAttribute('aria-posinset')) - 1;
          if (row.getAttribute('data-entry-id') !== visible[index]?.id) window.p2OldRows.push(row.textContent ?? '');
        }
      }).observe(list, { subtree: true, childList: true, attributes: true, characterData: true });
    };
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: {
      identity: Identity; offset?: number | null; entryId?: string; view?: ListingView;
    }) => {
      if (command === 'startup_config') return { folders: ['/fixture'], error: null, warnings: [] };
      window.p2Calls.push({ command, entryId: args.entryId, offset: args.offset, view: args.view });
      if (command === 'cancel_listing') return;
      if (command === 'locate_entry') return { identity: args.identity, revision,
        index: visible.some(entry => entry.id === args.entryId) ? visible.findIndex(entry => entry.id === args.entryId) : null };
      if (command === 'activate_entry') return { identity: args.identity, kind: 'file', path: null };
      if (command === 'set_listing_view') {
        const view = args.view;
        if (!view) throw new Error('Missing view');
        revision++;
        visible = entries.filter(entry => entry.name.includes(view.filter) && !(removed && entry.id === 'item-4'));
        visible.sort((a, b) => {
          const comparison = view.sort === 'size' ? (a.sizeBytes ?? 0) - (b.sizeBytes ?? 0)
            : view.sort === 'modified' ? (a.modifiedAtMs ?? 0) - (b.modifiedAtMs ?? 0) : a.name.localeCompare(b.name);
          return view.direction === 'asc' ? comparison : -comparison;
        });
      }
      const offset = command === 'read_listing' ? args.offset ?? 0 : 0;
      if (command === 'read_listing' && offset === 64 && heldPage) {
        const pending = heldPage;
        heldPage = undefined;
        await pending;
      }
      return { identity: args.identity, batch: { revision, offset, entries: visible.slice(offset, offset + 64),
        total: visible.length, complete: true, capped: false, snapshotBytes: 32768, capReason: null } };
    } } });
  });
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 192개');
  await expect(pane.locator('[data-entry-id="item-0"]')).toHaveAttribute('aria-selected', 'true');
  return pane;
}

test('refresh clamps a removed scroll anchor to the remaining viewport without returning to the top', async ({ page }) => {
  const pane = await installListing(page);
  const list = pane.getByRole('listbox');
  await list.evaluate(element => { element.scrollTop = 130 * 26; });
  await expect(pane.locator('[data-entry-id="item-130"]')).toBeVisible();
  await page.evaluate(() => window.p2Shrink(80));
  await pane.getByRole('button', { name: '새로 고침', exact: true }).click();
  await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 80개');
  await expect.poll(() => list.evaluate(element => element.scrollTop === element.scrollHeight - element.clientHeight && element.scrollTop > 0)).toBe(true);
  await expect(pane.locator('[data-entry-id="item-79"]')).toBeVisible();
  await expect(list.getByText('불러오는 중', { exact: true })).toHaveCount(0);
});

test('sort revisions retain selected identity and scroll anchor at its new index', async ({ page }) => {
  const pane = await installListing(page);
  // This listing-only IPC fixture intentionally lacks the watch service. Wait
  // for its delayed startup notice before measuring a fixed scroll viewport;
  // otherwise the notice moves the row while Playwright prepares its click.
  await expect(page.locator('.notice').filter({ hasText: '자동 갱신을 사용할 수 없습니다.' })).toBeVisible();
  const list = pane.getByRole('listbox');
  await list.focus();
  await page.keyboard.press('Control+Home');
  await page.evaluate(() => window.p2HoldPage64());
  await list.evaluate(element => {
    element.addEventListener('scroll', () => { window.p2ScrollEvents++; });
    element.scrollTop = 70 * 26 + 7;
  });
  await expect.poll(() => page.evaluate(() => window.p2Calls.filter(call => call.command === 'read_listing').map(call => call.offset))).toContain(64);
  await expect(list.locator('[aria-posinset="71"]')).toContainText('불러오는 중');
  await expect.poll(() => page.evaluate(() => window.p2ScrollEvents)).toBe(1);
  await page.evaluate(() => window.p2ReleasePage64());
  await expect(pane.locator('[data-entry-id="item-70"]')).toBeVisible();
  await pane.locator('[data-entry-id="item-72"]').click();
  await expect(pane.locator('[data-entry-id="item-72"]')).toHaveAttribute('aria-selected', 'true');
  expect(await page.evaluate(() => window.p2ScrollEvents)).toBe(1);
  await expect.poll(() => list.evaluate(element => element.scrollTop)).toBe((70 * 26 + 7));
  await pane.getByRole('button', { name: '정렬 방향' }).click();
  await expect.poll(() => list.evaluate(element => element.scrollTop)).toBe(121 * 26 + 7);
  await expect.poll(() => page.evaluate(() => window.p2Calls.filter(call => call.command === 'locate_entry').map(call => call.entryId)))
    .toContain('item-72');
  await list.focus();
  await page.keyboard.press('Enter');
  await expect.poll(() => page.evaluate(() => window.p2Calls.filter(call => call.command === 'activate_entry').map(call => call.entryId))).toEqual(['item-72']);
  await list.evaluate(element => { element.scrollTop = 119 * 26; });
  await expect(pane.locator('[data-entry-id="item-72"]')).toHaveAttribute('aria-selected', 'true');
});

test('Ctrl toggles and Shift extends a range by entry identity', async ({ page }) => {
  const pane = await installListing(page);
  await pane.locator('[data-entry-id="item-1"]').click();
  await pane.locator('[data-entry-id="item-3"]').click({ modifiers: ['Control'] });
  await expect(pane.locator('[aria-selected=true][data-entry-id]')).toHaveCount(2);
  await pane.locator('[data-entry-id="item-1"]').click({ modifiers: ['Control'] });
  await expect(pane.locator('[data-entry-id="item-1"]')).toHaveAttribute('aria-selected', 'false');
  await pane.locator('[data-entry-id="item-4"]').click({ modifiers: ['Shift'] });
  await expect(pane.locator('[aria-selected=true][data-entry-id]')).toHaveCount(4);
  for (const index of [1, 2, 3, 4]) await expect(pane.locator(`[data-entry-id="item-${index}"]`)).toHaveAttribute('aria-selected', 'true');
});

test('Ctrl+A selects the entire filtered view across pages and excludes hidden entries', async ({ page }) => {
  const pane = await installListing(page);
  await pane.getByRole('listbox').focus();
  await page.keyboard.press('End');
  await expect(pane.locator('[data-entry-id="item-191"]')).toHaveAttribute('aria-selected', 'true');
  await pane.getByRole('searchbox').fill('keep');
  await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 96개');
  await expect.poll(() => pane.getByRole('listbox').evaluate(element => element.scrollTop)).toBe(0);
  await expect(pane.locator('.selection-status')).toHaveText('0개 선택');
  await pane.getByRole('listbox').focus();
  await page.keyboard.press('Control+a');
  await expect(pane.locator('.selection-status')).toHaveText('96개 선택');
  await expect.poll(() => page.evaluate(() => window.p2Calls.filter(call => call.command === 'read_listing').map(call => call.offset))).toContain(64);
  await page.keyboard.press('Control+End');
  await expect(pane.locator('[data-entry-id="item-190"]')).toHaveAttribute('aria-selected', 'true');
  await expect(pane.getByRole('listbox')).not.toContainText('other');
  expect(await pane.getByRole('listbox').getByRole('option').count()).toBeLessThan(40);
});

test('cached pages from an old revision never render after a sort changes row order', async ({ page }) => {
  const pane = await installListing(page);
  const list = pane.getByRole('listbox');
  await list.evaluate(element => { element.scrollTop = 70 * 26; });
  await expect(pane.locator('[data-entry-id="item-70"]')).toBeVisible();
  await page.evaluate(() => window.p2ObserveOldRows());
  await pane.getByRole('combobox', { name: '정렬 기준' }).selectOption('size');
  await expect.poll(() => list.evaluate(element => element.scrollTop)).toBe(121 * 26);
  await list.focus();
  await page.keyboard.press('Home');
  await expect(pane.locator('[aria-posinset="1"]')).toHaveAttribute('data-entry-id', 'item-191');
  await page.keyboard.press('End');
  await expect(pane.locator('[aria-posinset="192"]')).toHaveAttribute('data-entry-id', 'item-0');
  expect(await page.evaluate(() => window.p2OldRows)).toEqual([]);
});

test('removed focused identity cannot activate the replacement row at its old index', async ({ page }) => {
  const pane = await installListing(page);
  await pane.locator('[data-entry-id="item-4"]').click();
  await page.evaluate(() => window.p2RemoveFocused());
  await pane.getByRole('combobox', { name: '정렬 기준' }).selectOption('modified');
  await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 191개');
  await expect(pane.locator('[aria-posinset="5"]')).toHaveAttribute('data-entry-id', 'item-5');
  await expect(pane.locator('[data-focused=true]')).toHaveCount(0);
  await pane.getByRole('listbox').focus();
  await page.keyboard.press('Enter');
  expect(await page.evaluate(() => window.p2Calls.filter(call => call.command === 'activate_entry'))).toEqual([]);
  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('Enter');
  await expect.poll(() => page.evaluate(() => window.p2Calls.filter(call => call.command === 'activate_entry').map(call => call.entryId))).toEqual(['item-1']);
});
