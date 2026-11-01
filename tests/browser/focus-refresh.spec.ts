import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

const starts = (page: Page) => page.evaluate(() => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === 'start_listing').length);
async function returnToApp(page: Page) {
  await page.evaluate(() => {
    window.dispatchEvent(new FocusEvent('blur'));
    for (let index = 0; index < 20; index++) window.dispatchEvent(new FocusEvent('focus'));
  });
}
test.beforeEach(async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
});

test('a return burst refreshes the visible real pane once and keeps the address draft', async ({ page }) => {
  const before = await starts(page);
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.fill('C:\\입력 중');
  await page.getByRole('button', { name: '1패널', exact: true }).focus();
  await page.evaluate(() => window.dispatchEvent(new FocusEvent('focus')));
  await page.waitForTimeout(300);
  expect(await starts(page)).toBe(before);
  await returnToApp(page);
  await expect.poll(() => starts(page)).toBe(before + 1);
  await page.waitForTimeout(300);
  expect(await starts(page)).toBe(before + 1);
  await expect(address).toHaveValue('C:\\입력 중');
});

test('focus revalidation preserves an open file action snapshot until it closes', async ({ page }) => {
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(dialog).toBeVisible();
  const before = await starts(page);
  await returnToApp(page); await page.waitForTimeout(300);
  expect(await starts(page)).toBe(before);
  await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await expect.poll(() => starts(page)).toBe(before + 1);
});

test('synthetic composition defers revalidation until compositionend', async ({ page }) => {
  const before = await starts(page);
  await returnToApp(page);
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.dispatchEvent('compositionstart');
  await page.waitForTimeout(300); expect(await starts(page)).toBe(before);
  await address.dispatchEvent('compositionend');
  await expect.poll(() => starts(page)).toBe(before + 1);
});

test('clipboard capture keeps its snapshot until the held IPC completes', async ({ page }) => {
  await page.evaluate(() => Reflect.set(window, 'p4HoldCapture', true));
  const before = await starts(page);
  await page.getByRole('group', { name: '패널 1 파일 작업', exact: true }).getByRole('button', { name: '복사 담기', exact: true }).click();
  await expect(page.getByText('파일 요청 처리 중…', { exact: false })).toBeVisible();
  await returnToApp(page); await page.waitForTimeout(300);
  expect(await starts(page)).toBe(before);
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => starts(page)).toBe(before + 1);
});

test('a pending close decision defers new listing requests until continuing', async ({ page }) => {
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('deciding'));
  await expect(page.getByRole('dialog', { name: '파일 작업을 마친 뒤 닫기', exact: true })).toBeVisible();
  const before = await starts(page);
  await returnToApp(page); await page.waitForTimeout(300);
  expect(await starts(page)).toBe(before);
  await page.evaluate(() => Reflect.get(window, 'p3OperationClose')('open'));
  await expect.poll(() => starts(page)).toBe(before + 1);
});

test('one reload releases combined completion and focus invalidations', async ({ page }) => {
  await page.getByRole('button', { name: '패널 1 새 폴더', exact: true }).click();
  const folder = page.getByRole('dialog', { name: '새 폴더', exact: true });
  await folder.getByRole('textbox').fill('queued');
  await folder.getByRole('button', { name: '대상 확인', exact: true }).click();
  await folder.getByRole('button', { name: '실행', exact: true }).click();
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const copy = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(copy).toBeVisible();
  const before = await starts(page);
  await returnToApp(page);
  await page.evaluate(() => Reflect.get(window, 'p4Finish')());
  await expect(page.getByRole('region', { name: '파일 작업 기록' })).toContainText('완료');
  await page.waitForTimeout(300); expect(await starts(page)).toBe(before);
  await copy.getByRole('button', { name: '취소', exact: true }).click();
  await page.waitForTimeout(500); expect(await starts(page)).toBe(before + 1);
});
