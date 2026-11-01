import { expect, test, type Page } from '@playwright/test';

type Scenario = 'empty' | 'error' | 'partial-error' | 'loading' | 'partial-loading' | 'capped' | 'invalid';

async function installScenario(page: Page, scenario: Scenario, folders = ['/fixture']) {
  await page.addInitScript(({ scenario, folders }) => {
    const runtime = Object.assign(window, { isTauri: true, uiCalls: new Array<string>() });
    let starts = 0;
    Object.assign(window, { __TAURI_INTERNALS__: { invoke: async (command: string, args: {
      identity: { pane: number }; path: string; offset: number | null;
    }) => {
      if (command === 'startup_config') return { folders, error: null, warnings: [] };
      runtime.uiCalls.push(`${command}:${args.identity.pane}:${args.path ?? ''}`);
      if (command === 'cancel_listing') return;
      if (command === 'start_listing') starts++;
      if (scenario === 'loading' || (scenario === 'partial-loading' && command === 'read_listing')) return new Promise(() => {});
      if ((scenario === 'error' && starts === 1) || (scenario === 'partial-error' && command === 'read_listing')) throw { code: 'timeout' };
      if (scenario === 'invalid') throw { code: 'invalid_path' };
      const entries = scenario === 'empty' ? [] : [{ id: 'entry-0', name: '보존할 한글.txt', kind: 'file', modifiedAtMs: 0, sizeBytes: 0 }];
      return { identity: args.identity, batch: { revision: 0, offset: 0, entries, total: entries.length,
        complete: scenario !== 'partial-error' && scenario !== 'partial-loading', capped: scenario === 'capped',
        snapshotBytes: 512, capReason: scenario === 'capped' ? 'entry_limit' : null } };
    } } });
  }, { scenario, folders });
  await page.goto('/');
}

test('empty completion has no rows or active descendant', async ({ page }) => {
  const renderLoopErrors: string[] = [];
  page.on('console', message => {
    if (message.type() === 'error' && message.text().includes('Maximum update depth exceeded')) renderLoopErrors.push(message.text());
  });
  await installScenario(page, 'empty');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.locator('footer [role=status]')).toContainText('빈 폴더 · 0개');
  await expect(pane.getByRole('listbox').getByRole('option')).toHaveCount(0);
  await pane.getByRole('listbox').focus();
  await page.keyboard.press('End');
  await expect(pane.getByRole('listbox')).not.toHaveAttribute('aria-activedescendant');
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-empty.png' });
  expect(renderLoopErrors).toEqual([]);
});

test('retryable errors show safe Korean guidance and retry the submitted path', async ({ page }) => {
  await installScenario(page, 'error');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.getByRole('alert')).toContainText('제한 시간');
  await expect(pane).not.toContainText('timeout');
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-error.png' });
  await pane.getByRole('textbox').fill('/edited-but-not-submitted');
  await pane.getByRole('button', { name: '다시 시도' }).click();
  await expect(pane.locator('footer [role=status]')).toContainText('조회 완료 · 1개');
  expect(await page.evaluate(() => window.uiCalls.filter(call => call.startsWith('start_listing'))))
    .toEqual(['start_listing:0:/fixture', 'start_listing:0:/fixture']);
});

test('partial entries stay visible after a progress error', async ({ page }) => {
  await installScenario(page, 'partial-error');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.getByRole('alert')).toBeVisible();
  await expect(pane.getByRole('option', { name: /보존할 한글/ })).toBeVisible();
  await expect(pane.getByRole('listbox')).toHaveAttribute('aria-busy', 'false');
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-partial-error.png' });
});

test('loading feedback keeps partial rows and identifies the busy list', async ({ page }) => {
  await installScenario(page, 'partial-loading');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.locator('footer [role=status]')).toContainText('일부 목록 표시');
  await expect(pane.getByRole('option', { name: /보존할 한글/ })).toBeVisible();
  await expect(pane.getByRole('listbox')).toHaveAttribute('aria-busy', 'true');
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-partial-loading.png' });
});

test('Escape cancels only active loading and remains inert during composition', async ({ page }) => {
  await installScenario(page, 'loading', ['/fixture/0', '/fixture/1']);
  const panes = page.getByRole('region', { name: /^패널 \d$/ });
  const address = panes.nth(1).getByRole('textbox');
  await expect(panes.nth(1).locator('footer [role=status]')).toContainText('읽는 중');
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-loading.png' });
  await address.focus();
  await address.dispatchEvent('compositionstart');
  await page.keyboard.press('Escape');
  await expect(panes.nth(1).getByRole('button', { name: '취소' })).toBeVisible();
  await address.dispatchEvent('compositionend');
  await address.dispatchEvent('keydown', { key: 'Escape', isComposing: true });
  await address.dispatchEvent('keydown', { key: 'Escape', keyCode: 229 });
  expect(await page.evaluate(() => window.uiCalls.filter(call => call.startsWith('cancel_listing')))).toEqual([]);
  await page.keyboard.press('Escape');
  await expect(panes.nth(1).locator('footer [role=status]')).toContainText('폴더 경로를 입력하세요');
  await expect(panes.nth(0).getByRole('button', { name: '취소' })).toBeVisible();
  await page.keyboard.press('Escape');
  expect(await page.evaluate(() => window.uiCalls.filter(call => call.startsWith('cancel_listing')))).toEqual(['cancel_listing:1:']);
});

test('capped results keep rows and explain the limit', async ({ page }) => {
  await installScenario(page, 'capped');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.locator('footer [role=status]')).toContainText('목록 상한 도달');
  await expect(pane.getByRole('listbox').getByRole('option')).toHaveCount(1);
  await page.screenshot({ path: 'artifacts/p2-20261005/browser-tests/ui-capped.png' });
});

test('invalid paths ask for correction without an automatic retry action', async ({ page }) => {
  await installScenario(page, 'invalid');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.getByRole('alert')).toContainText('절대 경로');
  await expect(pane.getByRole('button', { name: '다시 시도' })).toHaveCount(0);
});

test('Escape preserves a completed listing', async ({ page }) => {
  await installScenario(page, 'capped');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.getByRole('listbox').getByRole('option')).toHaveCount(1);
  await pane.getByRole('listbox').focus();
  await page.keyboard.press('Escape');
  await expect(pane.getByRole('listbox').getByRole('option')).toHaveCount(1);
  expect(await page.evaluate(() => window.uiCalls.filter(call => call.startsWith('cancel_listing')))).toEqual([]);
});
