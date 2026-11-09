import { expect, test, type Page } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

const list = (page: Page, pane = 1) => page.getByRole('listbox', { name: `패널 ${pane} 파일 목록`, exact: true });
const filter = (page: Page, pane = 1) => page.getByRole('searchbox', { name: `패널 ${pane} 이름 필터`, exact: true });
async function fixture(page: Page) {
  await installProjects(page, false, false, true, 140);
  await page.addInitScript(() => {
    const api = Reflect.get(window, '__TAURI_INTERNALS__'), invoke = api.invoke;
    const entries = Array.from({ length: 140 }, (_, i) => ({ id: `entry-${i}`, name: i === 0 ? 'Alpha.txt' : i === 1 ? 'alpine.txt' : i === 2 ? 'Beta.txt' : i === 100 ? 'Bravo.txt' : i === 130 ? '한글.txt' : `item-${i}.txt`, kind: 'file', sizeBytes: 12, modifiedAtMs: 0 }));
    const views = new Map<string, { filter: string; revision: number }>();
    const calls: { pane: number; filter: string }[] = [];
    Object.assign(window, { filterCalls: calls, filterWaiting: false, filterRelease: () => {} });
    api.invoke = async (command: string, args: Record<string, any>) => {
      if (command === 'load_project') {
        const result = await invoke(command, args);
        result.workspace = structuredClone(result.workspace);
        result.workspace.panes[0].tabs[0].path = { kind: 'relative', value: '' };
        return result;
      }
      if (['start_listing', 'read_listing', 'set_listing_view'].includes(command)) {
        const key = JSON.stringify(args.identity);
        const previous = views.get(key) ?? { filter: '', revision: 0 };
        const view = command === 'read_listing' ? previous : { filter: args.view.filter, revision: previous.revision + (command === 'set_listing_view' ? 1 : 0) };
        views.set(key, view);
        const rows = entries.filter(e => e.name.toLowerCase().includes(view.filter.toLowerCase()));
        const result = { identity: args.identity, batch: { offset: args.offset ?? 0, entries: rows.slice(args.offset ?? 0, (args.offset ?? 0) + 64), total: rows.length, complete: true, capped: false, snapshotBytes: 128, capReason: null, revision: view.revision } };
        if (command === 'set_listing_view') {
          calls.push({ pane: args.identity.pane, filter: view.filter });
          if (Reflect.get(window, 'filterHold')) {
            Reflect.set(window, 'filterWaiting', true);
            return new Promise(resolve => { Reflect.set(window, 'filterRelease', () => { Reflect.set(window, 'filterWaiting', false); resolve(result); }); });
          }
        }
        return result;
      }
      return invoke(command, args);
    };
  });
  const errors: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-selected', 'true');
  return errors;
}

test('first character enters the filter exactly once and subsequent typing edits it normally', async ({ page }) => {
  const errors = await fixture(page);
  await list(page).focus();
  await page.keyboard.type('Bravo');
  await expect(filter(page)).toBeFocused();
  await expect(filter(page)).toHaveValue('Bravo');
  await expect(list(page).getByRole('option')).toHaveCount(1);
  await expect(list(page).getByRole('option')).toContainText('Bravo.txt');
  await expect(list(page).locator('[aria-selected="true"]')).toHaveCount(0);
  await page.keyboard.press('Backspace'); await page.keyboard.type('o.txt');
  await expect(filter(page)).toHaveValue('Bravo.txt');
  expect(errors).toEqual([]);
});

test('typing from a filtered or empty list replaces the query; Escape clears and returns focus', async ({ page }) => {
  await fixture(page);
  await filter(page).fill('Alpha');
  await expect(list(page).getByRole('option')).toHaveCount(1);
  await list(page).press('B');
  await expect(filter(page)).toHaveValue('B');
  await expect(list(page).getByRole('option')).toHaveCount(2);
  await filter(page).fill('no-such-name');
  await expect(list(page).getByRole('option')).toHaveCount(0);
  await list(page).press('a');
  await expect(filter(page)).toHaveValue('a');
  await expect(list(page).getByRole('option')).toHaveCount(4);
  await filter(page).press('Escape');
  await expect(filter(page)).toHaveValue(''); await expect(list(page)).toBeFocused();
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-setsize', '140');
  await filter(page).focus(); await page.keyboard.press('Escape');
  await expect(list(page)).toBeFocused();
});

test('only the focused panel changes and existing keyboard selection remains available', async ({ page }) => {
  await fixture(page);
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await expect(list(page, 2)).toHaveAttribute('aria-busy', 'false');
  await list(page, 2).press('b');
  await expect(filter(page, 2)).toBeFocused(); await expect(filter(page, 2)).toHaveValue('b');
  await expect(filter(page)).toHaveValue('');
  await filter(page, 2).press('Escape');
  await expect(list(page, 2)).toHaveAttribute('aria-busy', 'false');
  await list(page).press('ArrowDown');
  await expect(list(page).locator('[data-entry-id="entry-1"]')).toHaveAttribute('aria-selected', 'true');
  await list(page).press('Space');
  await expect(list(page)).toBeFocused(); await expect(filter(page)).toHaveValue('');
});

test('IME, modifiers, editable fields and modal input do not redirect to the panel filter', async ({ page }) => {
  const errors = await fixture(page);
  await list(page).focus();
  await list(page).dispatchEvent('compositionstart');
  await list(page).dispatchEvent('keydown', { key: 'b', isComposing: true, keyCode: 229 });
  await list(page).dispatchEvent('compositionend', { data: '한' });
  for (const shortcut of ['Control+b', 'Alt+b', 'Meta+b']) await list(page).press(shortcut);
  await list(page).dispatchEvent('keydown', { key: 'b', repeat: true });
  await expect(filter(page)).toHaveValue(''); await expect(list(page)).toBeFocused();
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.fill('address draft'); await address.press('b');
  await expect(address).toHaveValue('address draftb');
  await page.getByRole('button', { name: '빠른 이동', exact: true }).click();
  await page.getByRole('dialog').getByRole('combobox').fill('Beta');
  await expect(filter(page)).toHaveValue('');
  expect(await page.evaluate(() => Reflect.get(window, 'filterCalls'))).toEqual([]);
  expect(errors).toEqual([]);
});

test('filter composition commits only after composition ends and Escape does not cancel composition', async ({ page }) => {
  await fixture(page);
  await filter(page).focus();
  await filter(page).dispatchEvent('compositionstart');
  await filter(page).fill('한글');
  await filter(page).dispatchEvent('keydown', { key: 'Escape', isComposing: true, keyCode: 229 });
  await expect(filter(page)).toBeFocused(); await expect(filter(page)).toHaveValue('한글');
  expect(await page.evaluate(() => Reflect.get(window, 'filterCalls'))).toEqual([]);
  await filter(page).dispatchEvent('compositionend', { data: '한글' });
  await expect(list(page).getByRole('option')).toHaveCount(1);
  await expect(list(page).getByRole('option')).toContainText('한글.txt');
  await filter(page).press('Escape');
  await expect(filter(page)).toHaveValue(''); await expect(list(page)).toBeFocused();
});

test('rapid typing then Escape cannot let an older pending filter replace the clear request', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => Reflect.set(window, 'filterHold', true));
  await list(page).press('b');
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'filterWaiting'))).toBe(true);
  await page.keyboard.type('ravo');
  await expect(filter(page)).toHaveValue('bravo');
  await page.keyboard.press('Escape');
  await expect(list(page)).toBeFocused(); await expect(filter(page)).toHaveValue('');
  await page.evaluate(() => { Reflect.set(window, 'filterHold', false); Reflect.get(window, 'filterRelease')(); });
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-setsize', '140');
  expect(await page.evaluate(() => Reflect.get(window, 'filterCalls').map((c: { filter: string }) => c.filter))).toEqual(['b', '']);
});

test('rapid filter input is immediate but sends only the latest coalesced query', async ({ page }) => {
  await fixture(page);
  await filter(page).focus();
  await page.keyboard.type('Bravo', { delay: 0 });
  await expect(filter(page)).toHaveValue('Bravo');
  await expect(list(page).getByRole('option')).toHaveCount(1);
  expect(await page.evaluate(() => Reflect.get(window, 'filterCalls').map((call: { filter: string }) => call.filter))).toEqual(['Bravo']);
  await expect(filter(page)).toBeFocused();
});
