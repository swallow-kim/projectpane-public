import { expect, test } from '@playwright/test';

// Delivered text and ordinary editing only; these are not physical IME tests.
for (const surface of ['projectpane', 'plain-controls'] as const) {
  test(`${surface}: committed text stays in its field after focus and Backspace`, async ({ page }) => {
    if (surface === 'projectpane') await page.goto('/');
    else await page.setContent('<input aria-label="패널 2 주소"><input aria-label="패널 3 주소">');
    const source = page.getByRole('textbox', { name: '패널 2 주소', exact: true });
    const destination = page.getByRole('textbox', { name: '패널 3 주소', exact: true });
    await source.focus();
    await page.keyboard.insertText('한글');
    for (const initial of ['', 'XY']) {
      await destination.fill(initial);
      await destination.click();
      await page.keyboard.press('End');
      await page.keyboard.press('Backspace');
      await expect(destination).toHaveValue(initial === '' ? '' : 'X');
      await expect(source).toHaveValue('한글');
      await expect(destination).toBeFocused();
    }
  });

  test(`${surface}: a separate Unicode input after Backspace edits only the focused field`, async ({ page }) => {
    if (surface === 'projectpane') await page.goto('/');
    else await page.setContent('<input aria-label="패널 2 주소"><input aria-label="패널 3 주소">');
    const source = page.getByRole('textbox', { name: '패널 2 주소', exact: true });
    const destination = page.getByRole('textbox', { name: '패널 3 주소', exact: true });
    await source.focus();
    await page.keyboard.insertText('한글');
    await destination.click();
    await page.keyboard.press('Backspace');
    await expect(destination).toHaveValue('');
    await page.keyboard.insertText('가');
    await expect(destination).toHaveValue('가');
    await expect(source).toHaveValue('한글');
  });
}
