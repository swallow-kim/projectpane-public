import { expect, test } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

test('address input and copied relative paths consistently use the project root', async ({ page }) => {
  await installProjects(page, false, false, true, 4);
  await page.addInitScript(() => {
    const api = Reflect.get(window, '__TAURI_INTERNALS__');
    const invoke = api.invoke;
    Reflect.set(window, 'relativeExports', []);
    api.invoke = async (command: string, args: unknown) => {
      if (command === 'export_to_explorer') { Reflect.get(window, 'relativeExports').push(args); return true; }
      return invoke(command, args);
    };
  });
  await page.goto('/');
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.fill('C:\\Alpha\\nested'); await address.press('Enter');
  await expect(address).toHaveValue('C:\\Alpha\\nested');
  await address.fill('src'); await address.press('Enter');
  await expect(address).toHaveValue('C:\\Alpha\\src');
  const row = page.locator('[data-pane="0"] .file-row').first();
  await expect(row).toBeVisible();
  await row.click({ button: 'right' });
  await page.getByRole('menuitem', { name: '상대경로 복사 (프로젝트 루트 기준)', exact: true }).click();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'relativeExports'))).toEqual([
    expect.objectContaining({ action: 'relative_path', basePath: 'C:\\Alpha' }),
  ]);
});
