import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test('held refresh preserves rows, selection and geometry while stale actions are disabled', async ({ page }) => {
  await installOperations(page, 3); await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await expect(list.getByRole('option')).toHaveCount(3);
  await list.getByRole('option').nth(1).click();
  // Wait for this mock fixture's asynchronous watch-unavailable notice before
  // measuring refresh geometry; the notice itself intentionally changes layout.
  await expect(page.getByText('자동 갱신을 사용할 수 없습니다.', { exact: false })).toBeVisible();
  const before = await list.boundingBox();
  await page.evaluate(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__');
    const previous = bridge.invoke;
    bridge.invoke = async (command: string, args: unknown) => {
      if (command === 'start_listing') await new Promise(resolve => Reflect.set(window, 'releaseRefresh', resolve));
      return previous(command, args);
    };
  });
  await pane.getByRole('button', { name: '새로 고침', exact: true }).click();
  await expect(list).toHaveAttribute('aria-busy', 'true');
  await expect(list.getByRole('option')).toHaveCount(3);
  await expect(list.getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true');
  expect(await list.boundingBox()).toEqual(before);
  await expect(pane.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeDisabled();
  await expect(pane.getByRole('searchbox', { name: '이름 필터', exact: false })).toBeDisabled();
  await list.getByRole('option').nth(2).click();
  await expect(list.getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'true');
  await page.evaluate(() => Reflect.get(window, 'releaseRefresh')());
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await expect(pane.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled();
});

test('End cannot scroll retained rows away from selection while refresh is pending', async ({ page }) => {
  await installOperations(page, 600); await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await expect(pane.locator('.file-row').first()).toContainText('한글.txt');
  await pane.locator('.file-row').first().click();
  await page.evaluate(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__');
    const previous = bridge.invoke;
    bridge.invoke = async (command: string, args: unknown) => {
      if (command === 'start_listing') await new Promise(resolve => Reflect.set(window, 'releaseRefresh', resolve));
      return previous(command, args);
    };
    window.addEventListener('keydown', event => { if (event.key === 'End') Reflect.set(window, 'endPrevented', event.defaultPrevented); });
  });
  await pane.getByRole('button', { name: '새로 고침', exact: true }).click();
  await expect(list).toHaveAttribute('aria-busy', 'true');
  await list.press('End');
  expect(await page.evaluate(() => Reflect.get(window, 'endPrevented'))).toBe(true);
  expect(await list.evaluate(element => element.scrollTop)).toBe(0);
  await expect(pane.locator('.file-row[data-focused=true]')).toContainText('한글.txt');
  await page.evaluate(() => Reflect.get(window, 'releaseRefresh')());
  await expect(list).toHaveAttribute('aria-busy', 'false');
  await list.press('End');
  await expect(pane.locator('.file-row[data-focused=true]')).toContainText('599');
});
