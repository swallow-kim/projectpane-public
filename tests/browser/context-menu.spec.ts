import { expect, test } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

test.beforeEach(async ({ page }) => { await installOperations(page, 3); await page.goto('/'); await expect(page.getByRole('button', { name: '패널 1 파일 복사', exact: true })).toBeEnabled(); });

test('right-click an unselected row selects it before rename', async ({ page }) => {
  const rows = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option');
  await rows.nth(1).click({ button: 'right' });
  await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(rows.first()).toHaveAttribute('aria-selected', 'false');
  await page.getByRole('menuitem', { name: '이름 변경', exact: true }).click();
  await expect(page.getByRole('dialog', { name: '이름 변경', exact: true })).toBeVisible();
});
test('right-click a selected row preserves multi-selection for internal copy', async ({ page }) => {
  const rows = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option');
  await rows.nth(1).click({ modifiers: ['Control'] });
  await rows.first().click({ button: 'right' });
  await expect(rows.first()).toHaveAttribute('aria-selected', 'true');
  await expect(rows.nth(1)).toHaveAttribute('aria-selected', 'true');
  await expect(page.getByRole('menuitem', { name: '이름 변경', exact: true })).toBeDisabled();
  await page.getByRole('menuitem', { name: '복사 담기', exact: true }).click();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Calls').filter((call: { command: string }) => call.command === 'append_copy_capture').at(-1)?.args.request.selection.entryIds)).toEqual(['entry-0', 'entry-1']);
  await expect(page.getByRole('menu')).toHaveCount(0);
});
test('keyboard menu supports navigation, escape and focus restoration', async ({ page }) => {
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.focus(); await page.keyboard.press('Shift+F10');
  await expect(page.getByRole('menu')).toBeVisible();
  await expect(page.getByRole('menuitem', { name: '열기', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: '탐색기에서 선택 항목 보기', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: 'Windows 메뉴', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: '절대경로 복사', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: '상대경로 복사 (현재 폴더 기준)', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: 'Windows에 복사', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: 'Windows에서 붙여넣기', exact: true })).toBeFocused();
  await page.keyboard.press('ArrowDown');
  await expect(page.getByRole('menuitem', { name: '복사 담기', exact: true })).toBeFocused();
  await page.keyboard.press('Escape'); await expect(list).toBeFocused(); await expect(page.getByRole('menu')).toHaveCount(0);
});
test('outside click dismisses menu without stealing the destination focus', async ({ page }) => {
  const list = page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true });
  await list.getByRole('option').first().click({ button: 'right' });
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.click(); await expect(page.getByRole('menu')).toHaveCount(0); await expect(address).toBeFocused();
});
