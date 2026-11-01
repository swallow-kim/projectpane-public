import { expect, test } from '@playwright/test';
import { installProjects } from './p3-project-fixture';
import { installOperations } from './p4-operation-fixture';
import { expandOperations } from './compact-ui-fixture';

for (const width of [375, 768, 1280]) {
  test(`many long project tabs keep the workspace compact at ${width}px`, async ({ page }, info) => {
    await page.setViewportSize({ width, height: 900 });
    await installProjects(page, false, false, true, 40, 30);
    await page.goto('/');
    const tabs = page.getByRole('tablist', { name: '프로젝트', exact: true });
    await expect(tabs.getByRole('tab')).toHaveCount(30);
    await expect(tabs.getByRole('tab', { selected: true })).toHaveAttribute('data-project-id', 'alpha');
    const bounds = await page.evaluate(() => ({
      workspaceTop: document.querySelector('.explorer-workspace')!.getBoundingClientRect().top,
      width: document.documentElement.clientWidth, scrollWidth: document.documentElement.scrollWidth,
    }));
    expect(bounds.workspaceTop).toBeLessThan(130);
    expect(bounds.scrollWidth).toBeLessThanOrEqual(bounds.width);
    await expect(page.getByText('Windows 10 개발판', { exact: true })).toHaveCount(0);
    await expect(page.getByText('로컬 폴더 탐색', { exact: true })).toHaveCount(0);
    await expect(page.getByText(/4패널 × 100,000개 항목/)).toHaveCount(0);
    await expect(page.locator('.app-help > div')).not.toBeVisible();
    await tabs.getByRole('tab', { selected: true }).focus();
    await page.keyboard.press('End');
    await expect(tabs.getByRole('tab').last()).toBeFocused();
    await expect(tabs.getByRole('tab', { selected: true })).toHaveAttribute('data-project-id', 'alpha');
    await page.keyboard.press('Enter');
    await expect(tabs.getByRole('tab', { selected: true })).toHaveAttribute('data-project-id', 'project-29');
    await expect(tabs.getByRole('tab').last()).toBeInViewport();
    await page.keyboard.press('Home');
    await page.keyboard.press('ArrowRight');
    await expect(tabs.getByRole('tab', { name: 'Beta', exact: true })).toBeFocused();
    await page.keyboard.press('Space');
    await expect(tabs.getByRole('tab', { selected: true })).toHaveAttribute('data-project-id', 'beta');
    await expect(page.getByRole('button', { name: '2패널', exact: true })).toHaveAttribute('aria-pressed', 'true');
    await page.screenshot({ path: info.outputPath(`compact-${width}.png`) });
    await info.attach('geometry', { body: JSON.stringify(bounds), contentType: 'application/json' });
    await page.locator('.app-help > summary').focus();
    await page.keyboard.press('Enter');
    await expect(page.locator('.app-help > div')).toBeVisible();
    await expect(page.locator('.app-help')).toContainText('Windows 복사/붙여넣기');
  });
}

test('operation history opens in a dialog without taking file-list layout space', async ({ page }, info) => {
  await installOperations(page); await page.goto('/');
  await expect(page.getByRole('button', { name: '패널 1 새 폴더', exact: true })).toBeEnabled();
  await page.evaluate(() => {
    for (let i = 0; i < 16; i++) Reflect.get(window, 'p4Operations').set(`op-${i}`, {
      id: `op-${i}`, projectId: null, status: 'succeeded', journalSaved: true,
      counts: { changed: 1, skipped: 0, failed: 0, unknown: 0 },
    });
  });
  const toggle = page.locator('.operation-tray__toggle');
  await expect(toggle).toHaveAttribute('aria-expanded', 'false');
  for (const height of [900, 650]) {
    await page.setViewportSize({ width: 960, height });
    const collapsed = await page.locator('.operation-tray').boundingBox();
    expect(collapsed!.y + collapsed!.height).toBeLessThanOrEqual(height);
    expect(collapsed!.y).toBeLessThan(50);
    const before = await page.locator('.explorer-workspace').boundingBox();
    await expandOperations(page);
    const expanded = await page.locator('.operation-tray').boundingBox();
    expect(expanded!.height).toBeLessThan(40);
    const history = page.getByRole('dialog', { name: '파일 작업 상태와 결과', exact: true });
    await expect(history).toBeVisible();
    expect(await page.locator('.explorer-workspace').boundingBox()).toEqual(before);
    await expect(page.getByRole('listbox', { name: '패널 1 파일 목록', exact: true })).toBeInViewport();
    await page.getByRole('dialog', { name: '파일 작업 상태와 결과', exact: true }).getByRole('button', { name: '닫기', exact: true }).click();
  }
  await page.screenshot({ path: info.outputPath('history-dialog.png') });
});
