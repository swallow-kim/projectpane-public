import { expect, test } from '@playwright/test';
import type { Identity } from '../../src/shared/contracts';

// Synthetic composition sequences in Chromium; not native physical IME validation.
declare global {
  interface Window {
    imeEchoCalls: { readonly command: string; readonly path?: string; readonly entryId?: string }[];
  }
}

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    window.imeEchoCalls = [];
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: {
      readonly identity: Identity; readonly path?: string; readonly entryId?: string;
    }) => {
      if (command === 'startup_config') return { folders: ['/fixture'], error: null, warnings: [] };
      window.imeEchoCalls.push({ command, path: args.path, entryId: args.entryId });
      if (command === 'cancel_listing') return;
      if (command === 'activate_entry') return { identity: args.identity, kind: 'folder', path: '/fixture/child' };
      return { identity: args.identity, batch: { revision: 0, offset: 0,
        entries: [{ id: 'child', name: 'child', kind: 'folder', sizeBytes: null, modifiedAtMs: null }],
        total: 1, complete: true, capped: false, snapshotBytes: 128, capReason: null } };
    } } });
  });
  await page.goto('/');
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).locator('footer [role=status]')).toContainText('조회 완료');
});

test('address Enter echo cannot submit until matching keyup', async ({ page }) => {
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const address = pane.getByRole('textbox', { name: '패널 1 주소' });
  await address.fill('/fixture/한글');
  await address.dispatchEvent('compositionstart');
  const imeDefaultAllowed = await address.evaluate(element => element.dispatchEvent(new KeyboardEvent('keydown', {
    key: 'Process', code: 'Enter', keyCode: 229, isComposing: true, bubbles: true, cancelable: true,
  })));
  expect(imeDefaultAllowed).toBe(true);
  await address.dispatchEvent('compositionend');

  await page.keyboard.down('Enter');

  expect(await page.evaluate(() => window.imeEchoCalls.filter(call => call.command === 'start_listing').map(call => call.path))).toEqual(['/fixture']);
  await expect(pane.getByRole('tab', { selected: true })).toHaveText('fixture');
  await page.keyboard.up('Enter');
  await page.keyboard.press('Enter');
  await expect(pane.getByRole('tab', { selected: true })).toHaveText('한글');
});

for (const key of ['Process', 'Enter']) {
  test(`list ${key}/Enter echo cannot activate a directory until matching keyup`, async ({ page }) => {
    const pane = page.getByRole('region', { name: '패널 1', exact: true });
    const list = pane.getByRole('listbox');
    await expect(list.locator('[data-entry-id="child"]')).toHaveAttribute('data-focused', 'true');
    await list.focus();
    await list.dispatchEvent('compositionstart');
    await list.dispatchEvent('keydown', { key, code: 'Enter', keyCode: 229, isComposing: true });
    await list.dispatchEvent('compositionend');

    await page.keyboard.down('Enter');

    expect(await page.evaluate(() => window.imeEchoCalls.filter(call => call.command === 'activate_entry'))).toEqual([]);
    await expect(pane.getByRole('tab', { selected: true })).toHaveText('fixture');
    await page.keyboard.up('Enter');
    await page.keyboard.press('Enter');
    await expect(pane.getByRole('tab', { selected: true })).toHaveText('child');
  });
}

test('F6 echo stays in its pane across focus changes and unrelated keyup', async ({ page }) => {
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  const list = page.getByRole('listbox').first();
  await address.focus();
  await address.dispatchEvent('compositionstart');
  await address.dispatchEvent('keydown', { key: 'Process', code: 'F6', keyCode: 229, isComposing: true });
  await address.dispatchEvent('compositionend');
  await list.focus();
  await list.dispatchEvent('keyup', { key: 'Shift', code: 'ShiftLeft' });

  await page.keyboard.down('F6');

  await expect(list).toBeFocused();
  await page.keyboard.up('F6');
  await page.keyboard.press('F6');
  await expect(page.getByRole('textbox', { name: '패널 2 주소', exact: true })).toBeFocused();
});

test('Ctrl+T echo does not add a tab; released shortcut adds exactly one', async ({ page }) => {
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const address = pane.getByRole('textbox', { name: '패널 1 주소' });
  await address.focus();
  const before = await pane.getByRole('tab').count();
  await address.dispatchEvent('compositionstart');
  await address.dispatchEvent('keydown', { key: 'Process', code: 'KeyT', ctrlKey: true, keyCode: 229 });
  await address.dispatchEvent('compositionend');

  await address.dispatchEvent('keydown', { key: 't', code: 'KeyT', ctrlKey: true });

  await expect(pane.getByRole('tab')).toHaveCount(before);
  await address.dispatchEvent('keyup', { key: 't', code: 'KeyT' });
  await page.keyboard.press('Control+t');
  await expect(pane.getByRole('tab')).toHaveCount(before + 1);
});

test('window blur releases an IME latch even when compositionend and keyup are lost', async ({ page }) => {
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.focus();
  await address.dispatchEvent('compositionstart');
  await address.dispatchEvent('keydown', { key: 'Process', code: 'F6', keyCode: 229 });

  await page.evaluate(() => window.dispatchEvent(new Event('blur')));

  await page.keyboard.press('F6');
  await expect(page.getByRole('textbox', { name: '패널 2 주소', exact: true })).toBeFocused();
});

test('FileList respects an already prevented keydown before selecting or activating', async ({ page }) => {
  const list = page.getByRole('listbox').first();
  await list.focus();

  await list.evaluate(element => {
    for (const key of ['Enter', ' ']) {
      const event = new KeyboardEvent('keydown', { key, code: key === 'Enter' ? 'Enter' : 'Space', ctrlKey: true, bubbles: true, cancelable: true });
      event.preventDefault();
      element.dispatchEvent(event);
    }
  });

  expect(await page.evaluate(() => window.imeEchoCalls.filter(call => call.command === 'activate_entry'))).toEqual([]);
  await expect(list.locator('[data-entry-id="child"]')).toHaveAttribute('aria-selected', 'true');
});
