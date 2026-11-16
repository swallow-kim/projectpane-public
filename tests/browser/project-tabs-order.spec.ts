import { expect, test, type Page, type JSHandle } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

const tabs = (page: Page) => page.locator('.project-tabs [role="tab"]');
const tab = (page: Page, id: string) => page.locator(`.project-tabs [data-project-id="${id}"]`);
const order = (page: Page) => tabs(page).evaluateAll(nodes => nodes.map(node => node.getAttribute('data-project-id')));
const calls = (page: Page, command: string) => page.evaluate(command => Reflect.get(window, 'p3Calls').filter((call: { command: string }) => call.command === command), command);
const initial = ['alpha', 'beta', 'project-2', 'project-3'];
async function setup(page: Page, options: { readOnly?: boolean; persist?: boolean; rootKinds?: Record<string, string>; holdRootKinds?: boolean; rootKindError?: boolean } = {}) {
  await installProjects(page, false, options.readOnly ?? false, false, 1, 4);
  await page.addInitScript(options => { Reflect.set(window, 'p3PersistOrder', options.persist ?? false); Reflect.set(window, 'p3RootKinds', options.rootKinds ?? {}); Reflect.set(window, 'p3HoldRootKinds', options.holdRootKinds ?? false); Reflect.set(window, 'p3RootKindError', options.rootKindError ?? false); }, options);
  await page.goto('/');
  await expect(tab(page, 'alpha')).toBeEnabled();
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  await expect.poll(() => order(page)).toEqual(initial);
}
async function startDrag(page: Page, source: string): Promise<JSHandle<DataTransfer>> {
  const data = await page.evaluateHandle(() => new DataTransfer());
  await tab(page, source).dispatchEvent('dragstart', { dataTransfer: data });
  return data;
}
async function drop(page: Page, data: JSHandle<DataTransfer>, target: string, after: boolean) {
  await tab(page, target).scrollIntoViewIfNeeded();
  const bounds = await tab(page, target).boundingBox();
  if (!bounds) throw new Error('Expected visible project tab');
  const point = { clientX: bounds.x + bounds.width * (after ? 0.8 : 0.2), clientY: bounds.y + bounds.height / 2, dataTransfer: data };
  await tab(page, target).dispatchEvent('dragover', point);
  await tab(page, target).dispatchEvent('drop', point);
}
async function drag(page: Page, source: string, target: string, after: boolean) {
  const data = await startDrag(page, source);
  try { await drop(page, data, target, after); }
  finally { await tab(page, source).dispatchEvent('dragend', { dataTransfer: data }); await data.dispose(); }
}
async function editAlphaRoot(page: Page, root: string) {
  await tab(page, 'alpha').click();
  await page.locator('.project-manager summary').click();
  await page.getByRole('button', { name: '프로젝트 편집', exact: true }).click();
  await page.getByRole('textbox', { name: '루트 경로', exact: true }).fill(root);
  await page.getByRole('button', { name: '변경하기', exact: true }).click();
  await expect(tab(page, 'alpha')).toBeEnabled();
  await expect(tab(page, 'alpha')).toHaveAttribute('title', new RegExp(root.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
}

test('root type badges, tooltips and accessible descriptions follow root edits and language', async ({ page }, info) => {
  await setup(page);
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'windows');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('Windows');
  await expect(tab(page, 'alpha')).toHaveAccessibleName('Alpha');
  await expect(tab(page, 'alpha')).toHaveAccessibleDescription(/Windows · C:\\Alpha/);
  await expect(tab(page, 'alpha').locator('svg')).toHaveAttribute('aria-hidden', 'true');
  await editAlphaRoot(page, '\\\\wsl.localhost\\Ubuntu\\home\\user');
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'wsl');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('WSL');
  await expect(tab(page, 'alpha')).toHaveAccessibleDescription(/WSL/);
  await editAlphaRoot(page, '\\\\192.168.0.5\\shared');
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'network');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('네트워크');
  await expect(tab(page, 'alpha')).toHaveAccessibleName('Alpha');
  await page.getByRole('combobox', { name: /언어 \/ Language|Language \/ 언어/ }).selectOption('en');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('Network');
  await expect(tab(page, 'alpha')).toHaveAccessibleDescription(/Network · \\\\192\.168\.0\.5\\shared/);
  await expect(tab(page, 'alpha')).toHaveAttribute('title', 'Alpha\nNetwork · \\\\192.168.0.5\\shared');
  await expect(tab(page, 'alpha')).toHaveAccessibleName('Alpha');
  await page.locator('.project-toolbar').screenshot({ path: info.outputPath('project-root-kind-network-english.png') });
});

test('drag reorders both directions without switching projects and survives reload', async ({ page }) => {
  await setup(page, { persist: true });
  const loaded = (await calls(page, 'load_project')).length;
  await drag(page, 'alpha', 'project-2', true);
  await expect.poll(() => order(page)).toEqual(['beta', 'project-2', 'alpha', 'project-3']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  expect((await calls(page, 'load_project')).length).toBe(loaded);
  await drag(page, 'project-3', 'beta', false);
  await expect.poll(() => order(page)).toEqual(['project-3', 'beta', 'project-2', 'alpha']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  expect((await calls(page, 'load_project')).length).toBe(loaded);
  expect((await calls(page, 'reorder_projects')).map((call: { args: { request: unknown } }) => call.args.request)).toEqual([
    { expectedOrder: initial, projectIds: ['beta', 'project-2', 'alpha', 'project-3'] },
    { expectedOrder: ['beta', 'project-2', 'alpha', 'project-3'], projectIds: ['project-3', 'beta', 'project-2', 'alpha'] },
  ]);
  await page.reload();
  await expect.poll(() => order(page)).toEqual(['project-3', 'beta', 'project-2', 'alpha']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
});

test('keyboard reorders the focused tab without selecting it and ignores list boundaries', async ({ page }) => {
  await setup(page);
  await tab(page, 'beta').focus();
  await tab(page, 'beta').press('Control+Shift+ArrowRight');
  await expect.poll(() => order(page)).toEqual(['alpha', 'project-2', 'beta', 'project-3']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  await tab(page, 'beta').focus();
  await tab(page, 'beta').press('Control+Shift+ArrowLeft');
  await expect.poll(() => order(page)).toEqual(initial);
  await tab(page, 'alpha').focus();
  await tab(page, 'alpha').press('Control+Shift+ArrowLeft');
  await tab(page, 'project-3').focus();
  await tab(page, 'project-3').press('Control+Shift+ArrowRight');
  expect(await calls(page, 'reorder_projects')).toHaveLength(2);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
});

test('escape and dragend cancel dragging and remove the insertion indicator', async ({ page }) => {
  await setup(page);
  for (const cancel of ['escape', 'dragend']) {
    const data = await startDrag(page, 'alpha');
    const target = tab(page, 'beta');
    const bounds = await target.boundingBox(); if (!bounds) throw new Error('Missing tab');
    await target.dispatchEvent('dragover', { dataTransfer: data, clientX: bounds.x + 1, clientY: bounds.y + 1 });
    await expect(target).toHaveAttribute('data-drop-position', 'before');
    if (cancel === 'escape') await target.dispatchEvent('keydown', { key: 'Escape' });
    else await tab(page, 'alpha').dispatchEvent('dragend', { dataTransfer: data });
    await expect(target).not.toHaveAttribute('data-drop-position');
    await drop(page, data, 'project-3', true);
    await data.dispose();
    await expect.poll(() => order(page)).toEqual(initial);
  }
  expect(await calls(page, 'reorder_projects')).toHaveLength(0);
});

test('external text, files and forged project payloads cannot reorder tabs', async ({ page }) => {
  await setup(page);
  for (const type of ['text', 'file', 'forged']) {
    const data = await page.evaluateHandle(type => {
      const value = new DataTransfer();
      if (type === 'text') value.setData('text/plain', 'alpha');
      if (type === 'file') value.items.add(new File(['fixture'], 'alpha.txt', { type: 'text/plain' }));
      if (type === 'forged') value.setData('application/x-projectpane-project-tab', 'alpha');
      return value;
    }, type);
    await drop(page, data, 'project-3', true);
    await data.dispose();
    await expect.poll(() => order(page)).toEqual(initial);
    await expect(tab(page, 'project-3')).not.toHaveAttribute('data-drop-position');
  }
  expect(await calls(page, 'reorder_projects')).toHaveLength(0);
});

test('same-position drops are no-ops and cannot switch the selected project', async ({ page }) => {
  await setup(page);
  await drag(page, 'beta', 'beta', true);
  await drag(page, 'alpha', 'beta', false);
  await expect.poll(() => order(page)).toEqual(initial);
  expect(await calls(page, 'reorder_projects')).toHaveLength(0);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
});

test('failed order saving keeps the previous order and active project', async ({ page }) => {
  await setup(page);
  await page.evaluate(() => Reflect.set(window, 'p3FailReorder', true));
  await drag(page, 'alpha', 'project-3', true);
  await expect(page.locator('.project-toolbar').getByRole('alert')).toBeVisible();
  await expect.poll(() => order(page)).toEqual(initial);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  expect(await calls(page, 'reorder_projects')).toHaveLength(1);
  await page.evaluate(() => Reflect.set(window, 'p3FailReorder', false));
  await tab(page, 'beta').focus();
  await tab(page, 'beta').press('Control+Shift+ArrowLeft');
  await expect.poll(() => order(page)).toEqual(['beta', 'alpha', 'project-2', 'project-3']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
});

test('read-only workspace disables reorder without disabling project navigation', async ({ page }) => {
  await setup(page, { readOnly: true });
  await expect(tab(page, 'alpha')).toHaveAttribute('draggable', 'false');
  await tab(page, 'beta').focus();
  await tab(page, 'beta').press('Control+Shift+ArrowLeft');
  await expect.poll(() => order(page)).toEqual(initial);
  expect(await calls(page, 'reorder_projects')).toHaveLength(0);
  await tab(page, 'beta').click();
  await expect(tab(page, 'beta')).toHaveAttribute('aria-selected', 'true');
});

test('native mouse drag reorders tabs without an accidental project activation', async ({ page }) => {
  await setup(page);
  const target = tab(page, 'beta');
  const bounds = await target.boundingBox(); if (!bounds) throw new Error('Missing target tab');
  await tab(page, 'alpha').dragTo(target, { targetPosition: { x: bounds.width - 5, y: bounds.height / 2 } });
  await expect.poll(() => order(page)).toEqual(['beta', 'alpha', 'project-2', 'project-3']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  expect(await calls(page, 'reorder_projects')).toHaveLength(1);
});

test('pending order save disables repeat reorder and commits only the confirmed order', async ({ page }) => {
  await setup(page);
  await page.evaluate(() => Reflect.set(window, 'p3HoldReorder', true));
  await drag(page, 'alpha', 'project-3', true);
  await expect(tab(page, 'alpha')).toBeDisabled();
  await expect(tab(page, 'alpha')).toHaveAttribute('draggable', 'false');
  await expect.poll(() => order(page)).toEqual(initial);
  await tab(page, 'beta').dispatchEvent('keydown', { key: 'ArrowLeft', ctrlKey: true, shiftKey: true });
  expect(await calls(page, 'reorder_projects')).toHaveLength(1);
  await page.evaluate(() => { Reflect.set(window, 'p3HoldReorder', false); Reflect.get(window, 'p3ReleaseReorder')(); });
  await expect.poll(() => order(page)).toEqual(['beta', 'project-2', 'project-3', 'alpha']);
  await expect(tab(page, 'alpha')).toBeEnabled();
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  expect(await calls(page, 'reorder_projects')).toHaveLength(1);
});

test('drive-letter roots use native mapped-drive classification rather than assuming local storage', async ({ page }) => {
  await setup(page, { rootKinds: { 'C:\\Alpha': 'network' } });
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'network');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('네트워크');
  await expect(tab(page, 'beta')).toHaveAttribute('data-root-kind', 'windows');
  await expect(tab(page, 'alpha')).toHaveAttribute('title', 'Alpha\n네트워크 · C:\\Alpha');
  expect((await calls(page, 'classify_project_roots')).length).toBeGreaterThan(0);
});

test('failed native classification remains visibly unknown in Korean and English', async ({ page }) => {
  await setup(page, { rootKindError: true });
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'unknown');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('확인 불가');
  await page.getByRole('combobox', { name: /언어 \/ Language|Language \/ 언어/ }).selectOption('en');
  await expect(tab(page, 'alpha').locator('.project-tab-kind')).toHaveText('Unknown');
  await expect(tab(page, 'alpha')).toHaveAccessibleDescription(/Unknown · C:\\Alpha/);
});

test('obsolete classification replies cannot overwrite a newly edited root', async ({ page }) => {
  await setup(page, { rootKinds: { 'C:\\Alpha': 'network' }, holdRootKinds: true });
  await expect.poll(() => page.evaluate(() => typeof Reflect.get(window, 'p3ReleaseRootKinds'))).toBe('function');
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'unknown');
  await editAlphaRoot(page, 'D:\\Fresh');
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'windows');
  await page.evaluate(() => Reflect.get(window, 'p3ReleaseRootKinds')());
  await expect(tab(page, 'alpha')).toHaveAttribute('title', 'Alpha\nWindows · D:\\Fresh');
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'windows');
  await expect(tab(page, 'beta')).toHaveAttribute('data-root-kind', 'windows');
});

test('desktop pointer drag works with mouse movement and leaves the active tab selected', async ({ page }) => {
  await setup(page);
  const source = await tab(page, 'alpha').boundingBox(), target = await tab(page, 'beta').boundingBox();
  if (!source || !target) throw new Error('Expected visible tabs');
  await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
  await page.mouse.down();
  await page.mouse.move(target.x + target.width - 4, target.y + target.height / 2, { steps: 8 });
  await page.mouse.up();
  await expect.poll(() => order(page)).toEqual(['beta', 'alpha', 'project-2', 'project-3']);
  await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  expect(await calls(page, 'reorder_projects')).toHaveLength(1);
});

test('desktop pointer escape, cancellation and outside release cannot reorder or activate tabs', async ({ page }) => {
  await setup(page);
  await expect(tab(page, 'alpha')).toHaveAttribute('data-root-kind', 'windows');
  await expect(tab(page, 'beta')).toHaveAttribute('data-root-kind', 'windows');
  // The fixture's unsupported watcher adds a notice asynchronously above the tabs.
  // Wait for that layout change before capturing pointer coordinates.
  await expect(page.locator('.notice').filter({ hasText: 'F5' })).toBeVisible();
  for (const cancel of ['escape', 'pointercancel', 'outside'] as const) {
    const source = await tab(page, 'beta').boundingBox(), target = await tab(page, 'alpha').boundingBox();
    if (!source || !target) throw new Error('Expected visible tabs');
    await page.mouse.move(source.x + source.width / 2, source.y + source.height / 2);
    await page.mouse.down();
    await page.mouse.move(target.x + 4, target.y + target.height / 2, { steps: 8 });
    await expect(tab(page, 'alpha'), cancel).toHaveAttribute('data-drop-position', 'before');
    if (cancel === 'escape') await page.keyboard.press('Escape');
    else if (cancel === 'pointercancel') await tab(page, 'beta').dispatchEvent('pointercancel');
    else await page.mouse.move(700, 700, { steps: 4 });
    await page.mouse.up();
    await expect(page.locator('[data-drop-position]')).toHaveCount(0);
    await expect.poll(() => order(page)).toEqual(initial);
    await expect(tab(page, 'alpha')).toHaveAttribute('aria-selected', 'true');
  }
  expect(await calls(page, 'reorder_projects')).toHaveLength(0);
  await tab(page, 'beta').click();
  await expect(tab(page, 'beta')).toHaveAttribute('aria-selected', 'true');
});
