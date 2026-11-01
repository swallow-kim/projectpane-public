import { expect, test } from '@playwright/test';

test('address shortcuts select the active address and cycle all four panes', async ({ page }) => {
  await page.goto('/');
  const addresses = page.getByRole('textbox');
  await page.waitForFunction(() => performance.getEntriesByName('projectpane-ui-ready').length > 0);
  await expect(addresses).toHaveCount(4);
  await page.keyboard.press('F6');
  await expect(addresses.nth(0)).toBeFocused();
  await page.keyboard.press('Shift+F6');
  await expect(addresses.nth(3)).toBeFocused();
  await page.keyboard.press('F6');
  await expect(addresses.nth(0)).toBeFocused();
  const path = 'C:\\한글 프로젝트';
  await addresses.nth(2).fill(path);
  await page.getByRole('listbox').nth(2).focus();
  await page.keyboard.press('Control+l');
  await expect(addresses.nth(2)).toBeFocused();
  expect(await addresses.nth(2).evaluate(element => element instanceof HTMLInputElement
    ? [element.selectionStart, element.selectionEnd, element.value.length] : [])).toEqual([0, path.length, path.length]);
});

test('composition lifecycle, native composition and keyCode 229 suppress pane shortcuts', async ({ page }) => {
  await page.goto('/');
  const address = page.getByRole('textbox').nth(1);
  await address.focus();
  await address.dispatchEvent('compositionstart');
  await page.keyboard.press('F6');
  await expect(address).toBeFocused();
  await address.dispatchEvent('compositionend');
  await address.dispatchEvent('keydown', { key: 'F6', isComposing: true });
  await expect(address).toBeFocused();
  await address.dispatchEvent('keydown', { key: 'F6', keyCode: 229 });
  await expect(address).toBeFocused();
  await page.keyboard.press('F6');
  await expect(page.getByRole('textbox').nth(2)).toBeFocused();
});

test('Ctrl+L and row navigation respect IME composition guards', async ({ page }) => {
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 2', exact: true });
  const list = pane.getByRole('listbox');
  await list.focus();
  await list.dispatchEvent('keydown', { key: 'l', ctrlKey: true, isComposing: true });
  await expect(list).toBeFocused();
  await list.dispatchEvent('keydown', { key: 'l', ctrlKey: true, keyCode: 229 });
  await expect(list).toBeFocused();
  await list.dispatchEvent('keydown', { key: 'End', isComposing: true });
  await list.dispatchEvent('keydown', { key: 'End', keyCode: 229 });
  await expect(list.locator('[aria-posinset="1"]')).toHaveAttribute('aria-selected', 'true');
  await page.keyboard.press('Control+l');
  await expect(pane.getByRole('textbox')).toBeFocused();
});
