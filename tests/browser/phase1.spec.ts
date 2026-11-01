import { expect, test, type Page } from '@playwright/test';
import { installRaceListing } from './p2-race-fixture';
import { installProjects } from './p3-project-fixture';

const pane = (page: Page) => page.locator('[data-pane="0"]');
const list = (page: Page) => pane(page).getByRole('listbox');
const status = (page: Page) => pane(page).locator('.selection-status');
async function begin(page: Page, first: number, last: number, modifier?: 'Control' | 'Shift') {
  await expect(page.getByText('자동 갱신을 사용할 수 없습니다.', { exact: false })).toBeVisible();
  const box = await list(page).boundingBox();
  if (!box) throw new Error('missing list');
  const width = await list(page).evaluate(e => e.clientWidth);
  if (modifier) await page.keyboard.down(modifier);
  await page.mouse.move(box.x + width - 5, box.y + first * 26 + 4);
  await page.mouse.down();
  await page.mouse.move(box.x + 50, box.y + last * 26 + 20, { steps: 5 });
}
async function finish(page: Page, modifier?: 'Control' | 'Shift') {
  await page.mouse.up(); if (modifier) await page.keyboard.up(modifier);
  await expect(list(page).locator('.file-marquee')).toHaveCount(0);
}

test('empty gutter marquee replaces, adds and toggles while row clicks and right click retain selection', async ({ page }) => {
  await installRaceListing(page, 512);
  await begin(page, 1, 3); await finish(page);
  await expect(status(page)).toHaveText('3개 선택');
  for (const n of [1, 2, 3]) await expect(list(page).locator(`[data-entry-id="item-${n}"]`)).toHaveAttribute('aria-selected', 'true');
  await begin(page, 3, 4, 'Shift'); await finish(page, 'Shift');
  await expect(status(page)).toHaveText('4개 선택');
  await begin(page, 2, 3, 'Control'); await finish(page, 'Control');
  await expect(status(page)).toHaveText('2개 선택');
  await list(page).locator('[data-entry-id="item-4"]').click({ button: 'right' });
  await expect(status(page)).toHaveText('2개 선택');
  await page.keyboard.press('Escape');
  await list(page).locator('[data-entry-id="item-7"]').click({ modifiers: ['Shift'] });
  await expect(status(page)).toHaveText('5개 선택');
});

test('Escape before threshold, active Escape, blur and view refresh cancel without committing preview', async ({ page }) => {
  await installRaceListing(page, 512);
  const box = (await list(page).boundingBox())!;
  const width = await list(page).evaluate(e => e.clientWidth);
  await page.mouse.move(box.x + width - 5, box.y + 30); await page.mouse.down();
  await page.keyboard.press('Escape'); await page.mouse.up();
  await expect(status(page)).toHaveText('1개 선택');
  await begin(page, 1, 4); await page.keyboard.press('Escape'); await finish(page);
  await expect(status(page)).toHaveText('1개 선택');
  await begin(page, 1, 4); await page.evaluate(() => window.dispatchEvent(new Event('blur'))); await finish(page);
  await expect(status(page)).toHaveText('1개 선택');
  await begin(page, 1, 4); await page.keyboard.press('Control+Shift+p');
  await expect(palette(page)).toBeVisible();
  await expect(list(page).locator('.file-marquee')).toHaveCount(0);
  await finish(page); await page.keyboard.press('Escape');
  await expect(status(page)).toHaveText('1개 선택');
  await begin(page, 1, 4);
  await pane(page).getByRole('searchbox').fill('keep'); await finish(page);
  await expect(status(page)).toHaveText('0개 선택');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
});

test('auto-scroll selects virtual rows beyond mounted DOM and plain empty click clears', async ({ page }, info) => {
  await installRaceListing(page, 512);
  await begin(page, 1, 3);
  const box = (await list(page).boundingBox())!;
  await page.mouse.move(box.x + 80, box.y + box.height + 40);
  await expect.poll(() => list(page).evaluate(e => e.scrollTop)).toBeGreaterThan(1400);
  await finish(page);
  const count = Number((await status(page).innerText()).replace(/\D/g, ''));
  expect(count).toBeGreaterThan(60);
  expect(await list(page).getByRole('option').count()).toBeLessThan(count);
  await expect(list(page).locator('[aria-selected="true"]').first()).toBeVisible();
  await page.screenshot({ path: info.outputPath('virtual-marquee.png') });
  const width = await list(page).evaluate(e => e.clientWidth);
  await page.mouse.click(box.x + width - 5, box.y + 50);
  await expect(status(page)).toHaveText('0개 선택');
});

test('a range request cancelled by a filter never commits stale selection', async ({ page }) => {
  await installRaceListing(page, 512);
  await page.evaluate(() => window.auditHold('0:read_listing:64'));
  await begin(page, 1, 3);
  const box = (await list(page).boundingBox())!;
  await page.mouse.move(box.x + 80, box.y + box.height + 40);
  await expect.poll(() => list(page).evaluate(e => e.scrollTop)).toBeGreaterThan(2000);
  await finish(page);
  await expect.poll(() => page.evaluate(() => window.auditWaiting)).toContain('0:read_listing:64');
  await pane(page).getByRole('searchbox').fill('keep');
  await page.evaluate(() => window.auditRelease('0:read_listing:64'));
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await expect(status(page)).toHaveText('0개 선택');
});

async function paletteFixture(page: Page) {
  await installProjects(page, true, false, true, 120);
  await page.addInitScript(() => {
    const bridge = Reflect.get(window, '__TAURI_INTERNALS__'), invoke = bridge.invoke;
    bridge.invoke = (command: string, args: unknown) => command === 'list_favorites'
      ? Promise.resolve({ schemaVersion: 1, revision: 0, folders: [{ name: '한글 공유', path: '\\\\server\\share' }, { name: '없는 폴더', path: 'C:\\Temporary\\missing' }] }) : invoke(command, args);
  });
  await page.goto('/');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
}
const palette = (page: Page) => page.getByRole('dialog', { name: '빠른 이동과 명령' });
async function search(page: Page, query: string) {
  await page.keyboard.press('Control+Shift+p');
  await expect(palette(page)).toBeVisible();
  await palette(page).getByRole('combobox').fill(query);
}

test('palette searches existing state without IPC, restores focus, and executes keyboard commands', async ({ page }) => {
  await paletteFixture(page);
  const projectTab = page.locator('[data-project-id="alpha"]');
  await projectTab.focus(); await search(page, '명령');
  await page.keyboard.press('Escape'); await expect(projectTab).toBeFocused();
  const address = pane(page).getByRole('textbox', { name: '패널 1 주소' });
  await address.focus();
  const before = await page.evaluate(() => Reflect.get(window, 'p3Calls').length);
  await search(page, '즐겨찾기 한글');
  await expect(palette(page).getByRole('option')).toHaveCount(1);
  expect(await page.evaluate(before => Reflect.get(window, 'p3Calls').slice(before).filter((c: { command: string }) => ['start_listing', 'read_listing', 'list_favorites', 'list_projects'].includes(c.command)), before)).toEqual([]);
  await page.keyboard.press('Escape'); await expect(address).toBeFocused();
  await search(page, '명령 새 탭'); await page.keyboard.press('Enter');
  await expect(pane(page).getByRole('tab')).toHaveCount(3); await expect(address).toBeFocused();
  await search(page, '최근 폴더 Temporary'); await page.keyboard.press('Enter');
  await expect(address).toHaveValue('C:\\Temporary');
  await search(page, '즐겨찾기 한글'); await page.keyboard.press('Enter');
  await expect(address).toHaveValue('\\\\server\\share');
  await search(page, '프로젝트 Beta'); await page.keyboard.press('Enter');
  await expect(page.locator('[data-project-id="beta"]')).toHaveAttribute('aria-selected', 'true');
});

test('palette reveals a hidden tab, reports unavailable folders, and fits a narrow window', async ({ page }, info) => {
  await paletteFixture(page);
  await page.getByRole('button', { name: '1패널', exact: true }).click();
  await search(page, '열린 탭 패널 4');
  await page.keyboard.press('Enter');
  await expect(page.locator('[data-pane="3"]')).toBeVisible();
  await expect(page.getByRole('textbox', { name: '패널 4 주소' })).toBeFocused();
  await search(page, '없는 폴더'); await page.keyboard.press('Enter');
  await expect(page.locator('[data-pane="3"] .listing-feedback')).toContainText('찾을 수');
  await page.setViewportSize({ width: 768, height: 700 });
  await search(page, '한글');
  expect(await palette(page).evaluate(e => e.scrollWidth <= e.clientWidth)).toBe(true);
  await page.screenshot({ path: info.outputPath('palette-narrow.png') });
  await page.keyboard.press('Tab');
  await expect(palette(page).getByRole('button', { name: '닫기', exact: true })).toBeFocused();
});

test('IME Enter and Escape echoes neither execute nor close the palette', async ({ page }) => {
  await paletteFixture(page); await search(page, '명령 새 탭');
  const input = palette(page).getByRole('combobox');
  await input.dispatchEvent('compositionstart');
  for (const key of ['Enter', 'Escape', 'ArrowDown']) {
    expect(await input.evaluate((e, key) => e.dispatchEvent(new KeyboardEvent('keydown', { key, isComposing: true, keyCode: 229, bubbles: true, cancelable: true })), key)).toBe(true);
  }
  await input.dispatchEvent('compositionend');
  for (const key of ['Enter', 'Escape']) await input.dispatchEvent('keydown', { key });
  await expect(palette(page)).toBeVisible(); await expect(pane(page).getByRole('tab')).toHaveCount(2);
  await input.dispatchEvent('keyup', { key: 'Enter' });
  await page.keyboard.press('Enter'); await expect(pane(page).getByRole('tab')).toHaveCount(3);
});

test('reopened unavailable folders keep a repairable address and palette shortcuts respect held IME keys', async ({ page }) => {
  await paletteFixture(page);
  const address = pane(page).getByRole('textbox', { name: '패널 1 주소' });
  await address.fill('C:\\Temporary\\missing'); await address.press('Enter');
  await expect(pane(page).locator('.listing-feedback')).toContainText('찾을 수');
  await page.keyboard.press('Control+w'); await page.keyboard.press('Control+Shift+t');
  await expect(address).toHaveValue('C:\\Temporary\\missing');
  await expect(pane(page).locator('.listing-feedback')).toContainText('찾을 수');
  await address.fill('C:\\Temporary'); await address.press('Enter');
  await expect(list(page)).toHaveAttribute('aria-busy', 'false');
  await address.dispatchEvent('compositionstart');
  await address.dispatchEvent('keydown', { key: 'Process', code: 'KeyP', ctrlKey: true, shiftKey: true, keyCode: 229, isComposing: true });
  await address.dispatchEvent('compositionend');
  await address.dispatchEvent('keydown', { key: 'P', code: 'KeyP', ctrlKey: true, shiftKey: true });
  await expect(palette(page)).not.toBeVisible();
  await address.dispatchEvent('keyup', { key: 'P', code: 'KeyP' });
  await search(page, '명령'); await page.keyboard.press('Escape'); await expect(address).toBeFocused();
});

test('protected tabs cannot close, restored tabs retain view but not selection, and project switch clears history', async ({ page }) => {
  await paletteFixture(page);
  const address = pane(page).getByRole('textbox', { name: '패널 1 주소' });
  await list(page).getByRole('option').nth(1).click();
  await pane(page).getByRole('searchbox').fill('파일');
  await pane(page).getByRole('button', { name: '현재 탭 닫기 보호', exact: true }).click();
  await page.keyboard.press('Control+w'); await expect(pane(page).getByRole('tab')).toHaveCount(2);
  await expect(pane(page).getByRole('button', { name: 'Temporary 탭 닫기', exact: true })).toBeDisabled();
  await pane(page).getByRole('button', { name: '현재 탭 닫기 보호', exact: true }).click();
  await page.keyboard.press('Control+w'); await expect(pane(page).getByRole('tab')).toHaveCount(1);
  await page.keyboard.press('Control+Shift+t'); await expect(pane(page).getByRole('tab')).toHaveCount(2);
  await expect(address).toHaveValue('C:\\Temporary'); await expect(pane(page).getByRole('searchbox')).toHaveValue('파일');
  await expect(list(page).getByRole('option').nth(1)).toHaveAttribute('aria-selected', 'false');
  await expect(status(page)).toHaveText('1개 선택');
  await page.keyboard.press('Control+w');
  await page.getByRole('tab', { name: 'Beta', exact: true }).click();
  await expect(pane(page).getByRole('button', { name: '닫은 탭 복원', exact: true })).toBeDisabled();
});
