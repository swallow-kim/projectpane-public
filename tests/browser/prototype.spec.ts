import { expect, test } from '@playwright/test';

test('four 100k lists keep DOM bounded across scrolling and keyboard navigation', async ({ page }) => {
  await page.goto('/');
  const lists = page.getByRole('listbox');
  await expect(lists).toHaveCount(4);
  await expect(page.getByRole('listbox').getByRole('option').first()).toBeVisible();
  expect(await page.getByRole('listbox').getByRole('option').count()).toBeLessThan(200);
  for (const list of await lists.all()) {
    await list.focus(); await page.keyboard.press('End');
    await expect(list.getByRole('option', { name: /100000/ })).toBeVisible();
  }
  expect(await page.getByRole('listbox').getByRole('option').count()).toBeLessThan(200);
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-prototype.png', fullPage: true });
});

test('composition Enter stays in the address field and inactive tabs do not claim real data', async ({ page }) => {
  await page.goto('/');
  const input = page.getByRole('textbox', { name: '패널 1 주소' });
  await input.fill('한글 프로젝트');
  await input.dispatchEvent('compositionstart');
  await input.dispatchEvent('keydown', { key: 'Enter', isComposing: true });
  await input.dispatchEvent('compositionend', { data: '트' });
  await expect(input).toHaveValue('한글 프로젝트');
  await page.getByRole('tab', { name: '실제 폴더', exact: true }).first().click();
  await expect(page.getByRole('listbox').first().getByRole('option')).toHaveCount(0);
  await expect(page.getByRole('button', { name: '열기', exact: true }).first()).toBeDisabled();
  await expect(page.locator('footer [role=status]').first()).toContainText('폴더 경로를 입력하세요');
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-idle.png' });
});

test('fixed rows survive resize, empty-tab shrink, and keyboard return to the beginning', async ({ page }) => {
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  const list = pane.getByRole('listbox');
  await list.focus(); await page.keyboard.press('End');
  await page.setViewportSize({ width: 960, height: 600 });
  await page.keyboard.press('End');
  await expect(list.locator('[aria-posinset="100000"]')).toBeVisible();
  await page.keyboard.press('Home');
  await page.keyboard.press('ArrowDown');
  await expect(list.locator('[aria-posinset="2"]')).toHaveAttribute('aria-selected', 'true');
  await pane.getByRole('tab', { name: '실제 폴더', exact: true }).click();
  await expect(list.getByRole('option')).toHaveCount(0);
  await pane.getByRole('tab', { name: '10만 항목', exact: true }).click();
  await expect(list.locator('[aria-posinset="1"]')).toBeVisible();
  expect(await page.getByRole('listbox').getByRole('option').count()).toBeLessThan(200);
  expect(await list.getByRole('option').first().evaluate(element => element.getBoundingClientRect().height)).toBe(26);
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-resize-960.png' });
});

test('Home and scroll in one turn preserve keyboard focus for the next ArrowDown', async ({ page }) => {
  await page.goto('/');
  const list = page.getByRole('region', { name: '패널 1', exact: true }).getByRole('listbox');
  await list.focus();
  await page.keyboard.press('End');
  await page.setViewportSize({ width: 960, height: 600 });
  await page.keyboard.press('End');
  await expect(list.locator('[aria-posinset="100000"]')).toHaveAttribute('aria-selected', 'true');
  await list.evaluate(element => {
    element.dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true, cancelable: true }));
    element.dispatchEvent(new Event('scroll'));
  });
  await page.keyboard.press('ArrowDown');
  await expect(list.locator('[aria-posinset="2"]')).toHaveAttribute('aria-selected', 'true');
  await expect(list.locator('[aria-posinset="2"]')).toHaveAttribute('data-focused', 'true');
  await expect(list).toHaveAttribute('aria-activedescendant', 'row-0-1');
});

test('mocked IPC streams data, fetches offscreen windows and stops on tab deactivation', async ({ page }) => {
  const runtimeErrors: string[] = [];
  page.on('pageerror', error => runtimeErrors.push(error.message));
  page.on('console', message => {
    if (message.type() === 'error') runtimeErrors.push(message.text());
  });
  // UI adapter test only: does not substitute for a native Tauri run.
  await page.addInitScript(() => {
    const runtime = Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: {}, starts: 0, cancels: 0 });
    runtime.isTauri = true; runtime.starts = 0; runtime.cancels = 0;
    let total = 0;
    runtime.__TAURI_INTERNALS__ = { invoke: async (command: string, args: { identity: unknown; offset: number | null }) => {
      if (command === 'startup_config') return { folders: [], error: null, warnings: [] };
      if (command === 'cancel_listing') { runtime.cancels++; return; }
      let offset: number;
      if (command === 'start_listing') { runtime.starts++; total = 256; offset = 0; }
      else if (args.offset === null) { offset = total; total = Math.min(3000, total + 256); }
      else offset = args.offset;
      await new Promise(resolve => setTimeout(resolve, 4));
      const progress = command !== 'start_listing' && args.offset === null;
      return { identity: args.identity, batch: { revision: 0, offset, entries: progress ? [] : Array.from({ length: Math.min(64, total - offset) }, (_, i) => ({ id: `id-${offset + i}`, name: `실제 목록 모의 ${offset + i + 1}`, kind: 'file', modifiedAtMs: null, sizeBytes: null })), total, complete: total === 3000, capped: false, snapshotBytes: 4096, capReason: null } };
    } };
  });
  await page.goto('/');
  expect(await page.evaluate(() => window.starts)).toBe(0);
  await page.getByRole('tab', { name: '실제 폴더', exact: true }).first().click();
  await page.getByRole('textbox', { name: '패널 1 주소' }).fill('/mock-fixture');
  await page.getByRole('button', { name: '열기', exact: true }).first().click();
  await expect(page.locator('footer [role=status]').first()).toContainText('조회 완료 · 3,000개');
  const list = page.getByRole('listbox').first();
  await list.focus(); await page.keyboard.press('End');
  await expect(list.getByRole('option', { name: /모의 3000/ })).toBeVisible();
  await expect(list).not.toContainText('불러오는 중');
  await page.setViewportSize({ width: 1280, height: 1600 });
  await page.keyboard.press('Home');
  for (let index = 0; index < 64; index++) await page.keyboard.press('ArrowDown');
  await expect(list.locator('[aria-posinset="65"]')).toHaveAttribute('aria-selected', 'true');
  await expect(list).not.toContainText('불러오는 중');
  await page.keyboard.press('Home');
  await expect(list.locator('[aria-posinset="1"] .file-name')).toContainText('실제 목록 모의 1');
  await expect(list.locator('[aria-posinset="1"] .kind')).toHaveText('파일');
  await page.getByRole('tab', { name: '10만 항목', exact: true }).first().click();
  expect(await page.evaluate(() => window.cancels)).toBeGreaterThan(0);
  expect(await page.evaluate(() => window.starts)).toBe(1);
  expect(runtimeErrors).toEqual([]);
});
