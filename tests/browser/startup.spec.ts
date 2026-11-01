import { expect, test } from '@playwright/test';

test('four native startup roots initialize independent real panes', async ({ page }) => {
  await page.addInitScript(() => {
    const runtime = Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: {}, startedPaths: new Array<string>() });
    runtime.isTauri = true;
    runtime.startedPaths = [];
    runtime.__TAURI_INTERNALS__ = { invoke: async (command: string, args: { identity: unknown; path: string; offset: number }) => {
      if (command === 'startup_config') return { folders: ['/fixture/0', '/fixture/1', '/fixture/2', '/fixture/3'], error: null, warnings: [] };
      if (command === 'cancel_listing') return;
      if (command === 'start_listing') runtime.startedPaths.push(args.path);
      const offset = command === 'start_listing' ? 0 : args.offset;
      return { identity: args.identity, batch: { revision: 0, offset, entries: Array.from({ length: 64 }, (_, index) => ({ id: `entry-${offset + index}`, name: `${offset + index}-한글.txt`, kind: 'file', modifiedAtMs: 0, sizeBytes: 0 })), total: 2500, complete: true, capped: false, snapshotBytes: 4096, capReason: null } };
    } };
  });
  await page.goto('/');
  for (let index = 0; index < 4; index++) {
    const pane = page.getByRole('region', { name: `패널 ${index + 1}`, exact: true });
    await expect(pane.getByRole('textbox')).toHaveValue(`/fixture/${index}`);
    await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 2,500개');
    await expect(pane.getByRole('tab', { name: String(index), exact: true })).toHaveAttribute('aria-selected', 'true');
  }
  expect(await page.evaluate(() => window.startedPaths)).toEqual(['/fixture/0', '/fixture/1', '/fixture/2', '/fixture/3']);
});

test('invalid startup arguments display a warning with no partial path loading', async ({ page }) => {
  await page.addInitScript(() => {
    const runtime = Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: {} });
    runtime.isTauri = true;
    runtime.__TAURI_INTERNALS__ = { invoke: async (command: string) => {
      if (command !== 'startup_config') throw new Error('Unexpected listing');
      return { folders: [], error: 'invalid_startup_args', warnings: [] };
    } };
  });
  await page.goto('/');
  await expect(page.getByRole('alert')).toContainText('시작 경로를 적용하지 못했습니다');
  for (const input of await page.getByRole('textbox').all()) await expect(input).toHaveValue('');
  for (const status of await page.locator('footer [role=status]').all()) await expect(status).toContainText('합성 데이터');
});

test('startup persistence warnings use readable messages while panes remain usable', async ({ page }) => {
  await page.addInitScript(() => {
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async () => ({
      folders: [], error: null, warnings: ['state_unavailable', 'log_unavailable', 'window_restore_failed'],
    }) } });
  });
  await page.goto('/');
  await expect(page.getByRole('alert')).toHaveCount(3);
  await expect(page.getByRole('alert').nth(0)).toContainText('창 위치를 저장할 수 없습니다');
  await expect(page.getByRole('alert').nth(1)).toContainText('진단 기록을 저장할 수 없습니다');
  await expect(page.getByRole('alert').nth(2)).toContainText('창 위치를 복원하지 못해');
  await expect(page.getByRole('listbox')).toHaveCount(4);
  await page.keyboard.press('Control+l');
  await expect(page.getByRole('textbox').first()).toBeFocused();
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-startup-warnings.png' });
});
