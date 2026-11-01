import { expect, test } from '@playwright/test';
import { confirmRecycle, installRecycle, openRecycle, recycleButton, recycleCallCount, recycleDialog } from './p4-recycle-fixture';

test('pending preparation cancellation discards the eventual preview', async ({ page }, info) => {
  await installRecycle(page);
  await page.goto('/');
  await page.evaluate(() => { window.recycle.holdPrepare = true; });
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  await recycleButton(page).click();
  await expect(confirmRecycle(page)).toBeDisabled();
  await expect(recycleDialog(page).getByRole('button', { name: '취소', exact: true })).toBeFocused();
  await page.screenshot({ path: info.outputPath('recycle-pending.png') });
  await recycleDialog(page).getByRole('button', { name: '취소', exact: true }).click();
  await page.evaluate(() => window.recycle.release());
  await expect.poll(() => recycleCallCount(page, 'discard_operation')).toBe(1);
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
});

for (const code of ['invalid_request', 'stale_request', 'resource_limit', 'access_denied']) {
  test(`native preparation refusal ${code} cannot be submitted`, async ({ page }, info) => {
    await installRecycle(page);
    await page.goto('/');
    await page.evaluate(code => { window.recycle.prepareError = code; }, code);
    await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
    await recycleButton(page).click();
    await expect(recycleDialog(page).getByRole('alert')).toBeVisible();
    await expect(confirmRecycle(page)).toBeDisabled();
    await page.screenshot({ path: info.outputPath(`recycle-refusal-${code}.png`) });
    await page.keyboard.press('Enter');
    expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  });
}

for (const mismatch of ['kind', 'project', 'destination', 'count'] as const) {
  test(`rejects and discards a native preview with mismatched ${mismatch}`, async ({ page }) => {
    await installRecycle(page);
    await page.goto('/');
    await page.evaluate(mismatch => { window.recycle.mismatch = mismatch; }, mismatch);
    await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
    await recycleButton(page).click();
    await expect(recycleDialog(page).getByRole('alert')).toBeVisible();
    await expect(confirmRecycle(page)).toBeDisabled();
    expect(await recycleCallCount(page, 'discard_operation')).toBe(1);
    expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
  });
}

test('stale source at confirmation shows refusal without recording success', async ({ page }) => {
  await installRecycle(page);
  await openRecycle(page);
  await expect(confirmRecycle(page)).toBeEnabled();
  await page.evaluate(() => { window.recycle.executeError = 'stale_request'; });
  await confirmRecycle(page).click();
  await expect(recycleDialog(page).getByRole('alert')).toContainText('선택한 항목이 바뀌었습니다');
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});

test('holding the opening Enter cannot confirm when the native preview arrives', async ({ page }) => {
  await installRecycle(page);
  await page.goto('/');
  await page.evaluate(() => { window.recycle.holdPrepare = true; });
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  await expect(recycleButton(page)).toBeEnabled();
  await recycleButton(page).focus();
  await expect(recycleButton(page)).toBeFocused();
  await page.keyboard.down('Enter');
  await expect(recycleDialog(page)).toBeVisible();
  await page.evaluate(() => window.recycle.release());
  await expect(confirmRecycle(page)).toBeEnabled();
  await expect(recycleDialog(page).getByRole('button', { name: '취소', exact: true })).toBeFocused();
  await page.keyboard.down('Enter');
  await page.keyboard.up('Enter');
  await expect(recycleDialog(page)).toBeVisible();
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
});

test('composition, 229, repeated keys and Enter on the source list never submit', async ({ page }) => {
  await installRecycle(page);
  await openRecycle(page);
  await expect(confirmRecycle(page)).toBeEnabled();
  const confirm = confirmRecycle(page);
  await confirm.focus();
  await confirm.dispatchEvent('compositionstart');
  await page.keyboard.press('Enter');
  await confirm.dispatchEvent('click');
  await confirm.dispatchEvent('compositionend');
  await page.keyboard.press('Enter');
  for (const props of [{ isComposing: true }, { keyCode: 229 }, { repeat: true }]) {
    expect(await confirm.evaluate((button, props) => button.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true, ...props })), props)).toBe(false);
  }
  await recycleDialog(page).getByRole('definition').click();
  await recycleDialog(page).locator('.operation-paths').focus();
  await page.keyboard.press('Enter');
  await page.keyboard.press('Delete');
  await page.keyboard.press('Shift+Delete');
  expect(await recycleCallCount(page, 'execute_operation')).toBe(0);
});

test('rapid confirmation submits once and blocks cancellation while pending', async ({ page }, info) => {
  await installRecycle(page);
  await openRecycle(page);
  await expect(confirmRecycle(page)).toBeEnabled();
  await page.evaluate(() => { window.recycle.holdExecute = true; });
  await confirmRecycle(page).evaluate(button => {
    if (!(button instanceof HTMLButtonElement)) throw new Error('Expected confirm button');
    button.click(); button.click();
  });
  await expect(confirmRecycle(page)).toBeDisabled();
  await expect(recycleDialog(page).getByRole('button', { name: '취소', exact: true })).toBeDisabled();
  await page.keyboard.press('Escape');
  await expect(recycleDialog(page)).toBeVisible();
  expect(await recycleCallCount(page, 'execute_operation')).toBe(1);
  await page.screenshot({ path: info.outputPath('recycle-submitting.png') });
  await page.evaluate(() => window.recycle.release());
  await expect(recycleDialog(page)).not.toBeVisible();
});
