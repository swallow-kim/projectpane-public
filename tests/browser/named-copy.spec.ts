import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

async function openNamedCopy(page: Page) {
  await installOperations(page); await page.goto('/');
  await page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true }).getByRole('option').first().click();
  await page.getByRole('button', { name: '패널 1 파일 복사', exact: true }).click();
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true }).fill('D:\\대상');
  await dialog.getByRole('checkbox', { name: '다른 이름으로 복사 (파일 1개)', exact: true }).check();
  return dialog;
}
const prepareCount = (page: Page) => page.evaluate(() => Reflect.get(window, 'p4Calls').filter((item: { command: string }) => item.command === 'prepare_operation').length);

for (const width of [375, 768, 1280]) test(`named copy submits the exact target once at ${width}px`, async ({ page }, info) => {
  const dialog = await openNamedCopy(page);
  await page.setViewportSize({ width, height: 900 });
  await expect(dialog.getByRole('button', { name: '복사', exact: true })).toBeDisabled();
  await dialog.getByRole('textbox', { name: '복사할 새 이름', exact: true }).fill('한글 새 이름.txt');
  await dialog.screenshot({ path: info.outputPath(`named-copy-input-${width}.png`) });
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
  const call = await page.evaluate(() => Reflect.get(window, 'p4Calls').find((item: { command: string }) => item.command === 'prepare_operation'));
  expect(call.args.request.action).toMatchObject({ kind: 'copy_as', name: '한글 새 이름.txt', destination: 'D:\\대상', selection: { entryIds: ['entry-0'] } });
  expect(await page.evaluate(() => [...Reflect.get(window, 'p4Prepared').values()][0].destination)).toBe('D:\\대상\\한글 새 이름.txt');
});

test('invalid names and native collisions leave the name editable without submitting', async ({ page }, info) => {
  const dialog = await openNamedCopy(page);
  const name = dialog.getByRole('textbox', { name: '복사할 새 이름', exact: true });
  await name.fill('..\\escape.txt'); await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible(); expect(await prepareCount(page)).toBe(0);
  await name.fill('collision'); await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog.getByRole('alert')).toContainText('이미 있거나');
  await dialog.screenshot({ path: info.outputPath('named-copy-collision.png') });
  await expect(name).toHaveValue('collision');
  await name.fill('available.txt'); await dialog.getByRole('button', { name: '복사', exact: true }).click();

  await expect(dialog).not.toBeVisible();
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(1);
});

test('a mismatched native target is discarded and never executable', async ({ page }) => {
  const dialog = await openNamedCopy(page); await page.evaluate(() => Reflect.set(window, 'p4WrongCopyName', true));
  await dialog.getByRole('textbox', { name: '복사할 새 이름', exact: true }).fill('approved.txt');
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog.getByRole('alert')).toBeVisible();
  await expect(dialog.getByRole('button', { name: '실행', exact: true })).toHaveCount(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Prepared').size)).toBe(0);
});

test('composition Enter in the new name does not prepare and focus changes retain separate values', async ({ page }) => {
  const dialog = await openNamedCopy(page);
  const name = dialog.getByRole('textbox', { name: '복사할 새 이름', exact: true });
  const target = dialog.getByRole('textbox', { name: '대상 폴더 절대 경로', exact: true });
  await name.dispatchEvent('compositionstart'); await name.fill('조합 이름.txt'); await name.press('Enter');
  await name.dispatchEvent('compositionend'); await name.press('Enter'); expect(await prepareCount(page)).toBe(0);
  await target.focus(); await target.press('End'); await target.press('Backspace');
  await expect(target).toHaveValue('D:\\대'); await expect(name).toHaveValue('조합 이름.txt');
  await target.fill('D:\\대상'); await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).not.toBeVisible();
  expect(await page.evaluate(() => [...Reflect.get(window, 'p4Prepared').values()][0].destination)).toBe('D:\\대상\\조합 이름.txt');
});

test('a late named-copy preview is discarded after cancellation', async ({ page }) => {
  const dialog = await openNamedCopy(page); await page.evaluate(() => Reflect.set(window, 'p4HoldPrepare', true));
  await dialog.getByRole('textbox', { name: '복사할 새 이름', exact: true }).fill('late.txt');
  await dialog.getByRole('button', { name: '복사', exact: true }).click();
  await expect(dialog).toContainText('대상 확인 중'); await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await page.evaluate(() => Reflect.get(window, 'p4Release')());
  await expect.poll(() => page.evaluate(() => Reflect.get(window, 'p4Prepared').size)).toBe(0);
  expect(await page.evaluate(() => Reflect.get(window, 'p4Operations').size)).toBe(0);
});
