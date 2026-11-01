import { expect, type Page } from '@playwright/test';
export async function expandOperations(page: Page) {
  const toggle = page.getByRole('button', { name: /파일 작업 \d+개/ });
  await expect(toggle).toBeVisible();
  if (await toggle.getAttribute('aria-expanded') === 'false') await toggle.click();
  await expect(toggle).toHaveAttribute('aria-expanded', 'true');
}
