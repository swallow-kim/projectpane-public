import { expect, test } from '@playwright/test';

test('hidden panes retain drafts and tabs while F6 uses only visible panes', async ({ page }) => {
  await page.goto('/');
  const fourth = page.getByRole('region', { name: '패널 4', exact: true });
  await fourth.getByRole('textbox', { name: '패널 4 주소' }).fill('보존할 초안');
  await page.getByRole('button', { name: '1패널', exact: true }).click();
  await expect(page.getByRole('region')).toHaveCount(1);
  await page.keyboard.press('F6');
  await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toBeFocused();
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await page.keyboard.press('F6');
  await expect(page.getByRole('textbox', { name: '패널 2 주소' })).toBeFocused();
  await page.keyboard.press('F6');
  await expect(page.getByRole('textbox', { name: '패널 1 주소' })).toBeFocused();
  await page.getByRole('button', { name: '4패널', exact: true }).click();
  await expect(fourth.getByRole('textbox', { name: '패널 4 주소' })).toHaveValue('보존할 초안');
});

test('splitters clamp keyboard and pointer ratios', async ({ page }) => {
  await page.goto('/');
  const splitter = page.getByRole('separator', { name: '좌우 패널 비율' });
  await splitter.focus();
  await page.keyboard.press('End');
  await expect(splitter).toHaveAttribute('aria-valuenow', '80');
  await page.keyboard.press('ArrowRight');
  await expect(splitter).toHaveAttribute('aria-valuenow', '80');
  await page.keyboard.press('Home');
  await expect(splitter).toHaveAttribute('aria-valuenow', '20');
  const bounds = await page.locator('.panes').boundingBox();
  if (!bounds) throw new Error('Missing pane layout');
  const grip = await splitter.boundingBox();
  if (!grip) throw new Error('Missing splitter');
  await page.mouse.move(grip.x + grip.width / 2, grip.y + 10);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width / 2, grip.y + 10);
  await page.mouse.up();
  await expect(splitter).toHaveAttribute('aria-valuenow', '50');

});

test('tabs add close reorder and preserve drafts, with a 32 tab limit', async ({ page }) => {
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const address = pane.getByRole('textbox', { name: '패널 1 주소' });
  await address.fill('첫 번째 초안');
  await page.keyboard.press('Control+t');
  await expect(pane.getByRole('tab')).toHaveCount(3);
  await expect(address).toHaveValue('');
  await address.fill('두 번째 초안');
  await pane.getByRole('button', { name: '탭 왼쪽으로 이동' }).click();
  await expect(pane.getByRole('tab').nth(1)).toHaveAttribute('aria-selected', 'true');
  await pane.getByRole('tab', { name: '10만 항목' }).click();
  await expect(address).toHaveValue('첫 번째 초안');
  await pane.getByRole('tab').nth(1).click();
  await expect(address).toHaveValue('두 번째 초안');
  await page.keyboard.press('Control+w');
  await expect(pane.getByRole('tab')).toHaveCount(2);
  for (let index = 0; index < 30; index++) await page.keyboard.press('Control+t');
  await expect(pane.getByRole('tab')).toHaveCount(32);
  await expect(pane.getByRole('button', { name: '새 탭', exact: true })).toBeDisabled();
  await page.keyboard.press('Control+t');
  await expect(pane.getByRole('tab')).toHaveCount(32);
  for (let index = 0; index < 32; index++) await page.keyboard.press('Control+w');
  await expect(pane.getByRole('tab')).toHaveCount(1);
  await expect(address).toHaveValue('');
});

test('native tab history and view updates do not restart on draft edits or hidden panes', async ({ page }) => {
  const renderLoopErrors: string[] = [];
  page.on('console', message => {
    if (message.type() === 'error' && message.text().includes('Maximum update depth exceeded')) renderLoopErrors.push(message.text());
  });
  await page.addInitScript(() => {
    const calls: string[] = [];
    Object.assign(window, { isTauri: true, p2TabCalls: calls, __TAURI_INTERNALS__: { invoke: async (command: string, args: { identity: { pane: number }; path: string; offset: number | null }) => {
      if (command === 'startup_config') return { folders: ['/fixture/a', '/fixture/b', '/fixture/c', '/fixture/d'], error: null, warnings: [] };
      calls.push(`${command}:${args.identity.pane}:${args.path ?? ''}`);
      if (command === 'cancel_listing') return;
      return { identity: args.identity, batch: { offset: args.offset ?? 0, entries: [], total: 0, complete: true, capped: false, snapshotBytes: 0, capReason: null, revision: command === 'set_listing_view' ? 1 : 0 } };
    } } });
  });
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const address = pane.getByRole('textbox', { name: '패널 1 주소' });
  await expect(pane.locator('footer').getByRole('status')).toContainText('빈 폴더');
  await address.fill('/fixture/a/child');
  await address.press('Enter');
  await expect(pane.getByRole('tab', { selected: true })).toHaveText('child');
  await address.press('Alt+ArrowLeft');
  await expect(address).toHaveValue('/fixture/a');
  await address.press('Alt+ArrowRight');
  await expect(address).toHaveValue('/fixture/a/child');
  await address.press('Alt+ArrowUp');
  await expect(address).toHaveValue('/fixture/a');
  await expect(pane.locator('footer').getByRole('status')).toContainText('빈 폴더');
  const starts = () => page.evaluate(() => {
    const calls = Reflect.get(window, 'p2TabCalls');
    return Array.isArray(calls) ? calls.filter((value: unknown) => typeof value === 'string' && value.startsWith('start_listing')).length : 0;
  });
  const before = await starts();
  await address.fill('/only-draft');
  await pane.getByRole('searchbox').fill('한글');
  await pane.getByRole('combobox').selectOption('size');
  expect(await starts()).toBe(before);
  await page.getByRole('button', { name: '1패널', exact: true }).click();
  await page.getByRole('button', { name: '4패널', exact: true }).click();
  await expect.poll(starts).toBe(before + 3);
  await expect(address).toHaveValue('/only-draft');
  await expect(pane.getByRole('searchbox')).toHaveValue('한글');
  await page.setViewportSize({ width: 768, height: 900 });
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/narrow-tabs.png' });
  expect(renderLoopErrors).toEqual([]);
});
