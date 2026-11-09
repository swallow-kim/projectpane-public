import { expect, test, type Page } from '@playwright/test';
import { installOperations } from './p4-operation-fixture';

const list = (page: Page, pane = 1) => page.getByRole('listbox', { name: `패널 ${pane} 파일 목록`, exact: true });
const calls = (page: Page, command: string) => page.evaluate(command => {
  const log = Reflect.get(window, 'p4Calls') as { command: string; args: unknown }[];
  return log.filter(item => item.command === command);
}, command);

test.beforeEach(async ({ page }) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await list(page).getByRole('option').first().click();
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByRole('button', { name: '복사 담기', exact: true })).toBeEnabled();
});

test('copy remains available after navigation and submits once to the current destination without a dialog', async ({ page }, info) => {
  await list(page).press('Control+Shift+c');
  await expect(page.locator('.transfer-status')).toContainText('복사 1개 담음');
  const address = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  await address.fill('C:\\Destination'); await address.press('Enter');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await list(page).press('Control+Shift+v');
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect.poll(async () => calls(page, 'begin_copy_job')).toHaveLength(1);
  await expect(dialog).not.toBeVisible();
  expect((await calls(page, 'begin_copy_job'))[0].args).toMatchObject({ request: { destination: 'C:\\Destination' } });
  await page.screenshot({ path: info.outputPath('internal-copy-preview.png') });
  expect(await calls(page, 'begin_copy_job')).toHaveLength(1);
  expect(await calls(page, 'prepare_operation')).toHaveLength(0);
  await expect(dialog).not.toBeVisible();
  await expect(page.locator('.transfer-status')).toContainText('복사 작업');
  await expect(page.getByRole('button', { name: '앱에 붙여넣기', exact: true }).first()).toBeEnabled();
  await page.reload();
  await expect(page.locator('.transfer-status')).not.toContainText('담음');
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
});

test('cut can discard a preview, then is consumed after one explicit submission', async ({ page }) => {
  await list(page).press('Control+x');
  await expect(page.locator('.transfer-status')).toContainText('잘라내기 1개 담음');
  const secondAddress = page.getByRole('textbox', { name: '패널 2 주소', exact: true });
  await page.getByRole('region', { name: '패널 2', exact: true }).locator('[role="tab"][data-source="real"]').click();
  await secondAddress.fill('C:\\Target'); await secondAddress.press('Enter');
  await expect(list(page, 2)).toHaveAttribute('aria-busy', 'false');
  await list(page, 2).press('Control+v');
  const dialog = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await dialog.getByRole('button', { name: '취소', exact: true }).click();
  await expect.poll(async () => (await calls(page, 'discard_operation')).length).toBe(1);
  await list(page, 2).press('Control+v');
  await dialog.getByRole('button', { name: '대상 확인', exact: true }).click();
  await dialog.getByRole('button', { name: '실행', exact: true }).click();
  await expect(page.locator('.transfer-status')).not.toContainText('담음');
  await list(page, 2).press('Control+v');
  await expect(dialog).not.toBeVisible();
  await expect.poll(async () => calls(page, 'begin_copy_job')).toHaveLength(1);
  await expect(page.getByRole('dialog', { name: '파일 복사', exact: true })).not.toBeVisible();
  expect(await calls(page, 'capture_copy_clipboard')).toHaveLength(1);
});

test('text fields and composition echoes never invoke clipboard file operations', async ({ page }) => {
  const input = page.getByRole('textbox', { name: '패널 1 주소', exact: true });
  for (const key of ['c', 'x', 'v']) await input.dispatchEvent('keydown', { key, code: `Key${key.toUpperCase()}`, ctrlKey: true });
  const files = list(page);
  for (const key of ['c', 'x', 'v']) {
    const code = `Key${key.toUpperCase()}`;
    await files.dispatchEvent('compositionstart');
    await files.dispatchEvent('keydown', { key: 'Process', code, ctrlKey: true, keyCode: 229, isComposing: true });
    await files.dispatchEvent('compositionend');
    await files.dispatchEvent('keydown', { key, code, ctrlKey: true });
    await files.dispatchEvent('keyup', { key, code });
    await files.dispatchEvent('keydown', { key, code, ctrlKey: true, repeat: true });
  }
  expect(await calls(page, 'capture_transfer')).toHaveLength(0);
  expect(await calls(page, 'begin_copy_job')).toHaveLength(0);
  expect(await calls(page, 'export_copy_capture')).toHaveLength(0);
  expect(await calls(page, 'capture_copy_clipboard')).toHaveLength(0);
  await files.press('Control+Shift+c');
  await expect(page.locator('.transfer-status')).toContainText('복사 1개 담음');
});

test('stale captured originals fail nonmodally without submitting', async ({ page }) => {
  await list(page).press('Control+Shift+c');
  await expect(page.locator('.transfer-status')).toContainText('담음');
  await page.evaluate(() => Reflect.set(window, 'p4StaleTransfer', true));
  await list(page).press('Control+Shift+v');
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect(dialog).not.toBeVisible();
  await expect(page.getByRole('alert')).toContainText('선택한 항목이 바뀌었습니다');
  expect(await page.evaluate(() => Reflect.get(window, 'p4CopyJobs').size)).toBe(0);
});

test('internal panel drag copies and an unrelated external payload is ignored', async ({ page }, info) => {
  const source = list(page).getByRole('option').first();
  const destination = list(page, 2);
  const address = page.getByRole('textbox', { name: '패널 2 주소', exact: true });
  await page.getByRole('region', { name: '패널 2', exact: true }).locator('[role="tab"][data-source="real"]').click();
  await address.fill('C:\\DropTarget'); await address.press('Enter');
  await expect(destination).toHaveAttribute('aria-busy', 'false');
  const foreign = await page.evaluateHandle(() => { const data = new DataTransfer(); data.setData('application/x-projectpane-transfer', 'untrusted'); data.setData('text/plain', 'C:\\private'); return data; });
  await destination.dispatchEvent('drop', { dataTransfer: foreign });
  expect(await calls(page, 'begin_copy_capture')).toHaveLength(0);
  await source.dragTo(destination);
  const dialog = page.getByRole('dialog', { name: '파일 복사', exact: true });
  await expect.poll(async () => calls(page, 'begin_copy_job')).toHaveLength(1);
  await expect(dialog).not.toBeVisible();
  expect((await calls(page, 'begin_copy_job'))[0].args).toMatchObject({ request: { destination: 'C:\\DropTarget' } });
  expect(await calls(page, 'begin_copy_capture')).toHaveLength(1);
  await page.screenshot({ path: info.outputPath('internal-panel-drop.png') });
});

test('Shift drag previews a move and foreign file drop elsewhere cannot navigate or submit', async ({ page }) => {
  const pane = page.getByRole('region', { name: '패널 2', exact: true });
  await pane.locator('[role="tab"][data-source="real"]').click();
  const address = page.getByRole('textbox', { name: '패널 2 주소', exact: true });
  await address.fill('C:\\MoveTarget'); await address.press('Enter');
  await expect(page.getByRole('button', { name: '패널 2 새 폴더', exact: true })).toBeEnabled();
  const foreign = await page.evaluateHandle(() => { const data = new DataTransfer(); data.items.add(new File(['owned browser fixture'], 'external.txt')); return data; });
  const before = page.url();
  await page.locator('header').dispatchEvent('drop', { dataTransfer: foreign });
  expect(page.url()).toBe(before);
  expect(await calls(page, 'capture_transfer')).toHaveLength(0);
  const source = await list(page).getByRole('option').first().boundingBox();
  const target = await list(page, 2).boundingBox();
  expect(source).not.toBeNull(); expect(target).not.toBeNull();
  if (!source || !target) throw new Error('Visible drag endpoints required');
  await page.mouse.move(source.x + 30, source.y + source.height / 2);
  await page.mouse.down();
  await page.mouse.move(source.x + 60, source.y + source.height / 2, { steps: 3 });
  await page.keyboard.down('Shift');
  try {
    await page.mouse.move(target.x + 40, target.y + 40, { steps: 6 });
    await page.mouse.move(target.x + 45, target.y + 45);
    await page.mouse.up();
  } finally { await page.keyboard.up('Shift'); await page.mouse.up(); }
  const dialog = page.getByRole('dialog', { name: '파일 이동', exact: true });
  await expect(dialog.getByRole('textbox')).toHaveValue('C:\\MoveTarget');
  await expect(dialog).toContainText('잘라내기');
  expect(await calls(page, 'execute_operation')).toHaveLength(0);
});
