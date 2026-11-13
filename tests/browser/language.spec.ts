import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

const language = (page: import('@playwright/test').Page) => page.getByRole('combobox', { name: /언어 \/ Language|Language \/ 언어/ });
test('language switches immediately, persists after reload and preserves file content', async ({ page }) => {
  await installOperations(page, 3); await page.goto('/');
  await expect(language(page)).toHaveValue('ko');
  const row = page.getByRole('listbox').first().getByRole('option').first();
  await expect(row).toBeVisible();
  const filename = await row.locator('.file-name').textContent();
  await language(page).selectOption('en');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  await expect(page.getByRole('button', { name: 'Quick navigation', exact: true })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Pane 1', exact: true })).toBeVisible();
  await expect(page.locator('.column-label').filter({ hasText: /^Name$/ }).first()).toBeVisible();
  expect(await row.locator('.file-name').textContent()).toBe(filename);
  await page.reload();
  await expect(language(page)).toHaveValue('en');
  await expect(page.getByRole('button', { name: 'Quick navigation', exact: true })).toBeVisible();
  await language(page).selectOption('ko');
  await expect(page.getByRole('button', { name: '빠른 이동', exact: true })).toBeVisible();
  await expect(page.locator('html')).toHaveAttribute('lang', 'ko');
});
test('invalid saved language falls back to Korean', async ({ page }) => {
  await page.addInitScript(() => localStorage.setItem('projectpane.language.v1', 'unsupported'));
  await page.goto('/'); await expect(language(page)).toHaveValue('ko');
});
test('storage failure is visible and does not block switching', async ({ page }) => {
  await page.addInitScript(() => { Storage.prototype.setItem = () => { throw new Error('storage blocked'); }; });
  await page.goto('/'); await language(page).selectOption('en');
  await expect(page.getByRole('alert').filter({ hasText: 'The language preference could not be saved.' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Quick navigation', exact: true })).toBeVisible();
});
