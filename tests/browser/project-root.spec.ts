import { expect, test, type Page } from '@playwright/test';
import { installProjects } from './p3-project-fixture';

const list = (page: Page, pane = 1) => page.getByRole('listbox', { name: `패널 ${pane} 파일 목록`, exact: true });
const address = (page: Page, pane = 1) => page.getByRole('textbox', { name: `패널 ${pane} 주소`, exact: true });
async function fixture(page: Page) {
  await installProjects(page, false, false, true, 140);
  await page.addInitScript(() => {
    const api = Reflect.get(window, '__TAURI_INTERNALS__');
    const invoke = api.invoke;
    const names = Array.from({ length: 140 }, (_, i) => i === 0 ? 'Alpha.txt' : i === 1 ? 'alpine.txt' : i === 2 ? 'Beta.txt' : i === 100 ? 'Bravo.txt' : i === 130 ? '한글.txt' : `item-${i}.txt`);
    api.invoke = async (command: string, args: Record<string, any>) => {
      if (command === 'start_listing' && Reflect.get(window, 'missingRoot') && args.path === 'C:\\Alpha') throw { code: 'not_found' };
      const result = await invoke(command, args);
      if (command === 'load_project' && !Reflect.get(window, 'p3Sessions').has(args.request.projectId)) {
        result.workspace = structuredClone(result.workspace);
        result.workspace.panes[0].tabs[0].path = { kind: 'relative', value: '' };
        result.workspace.panes[1].tabs[0].path = { kind: 'empty', value: '' };
        result.workspace.panes[2].tabs[0].path = { kind: 'relative', value: 'kept' };
      }
      if (['start_listing', 'read_listing', 'set_listing_view'].includes(command)) {
        result.batch.entries = result.batch.entries.map((entry: { id: string }) => ({ ...entry, name: names[Number(entry.id.replace('entry-', ''))] }));
      }
      return result;
    };
  });
  const errors: string[] = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.goto('/');
  await expect(list(page).getByRole('option').first()).toHaveAttribute('aria-selected', 'true');
  return errors;
}

test('new tabs use the project root while project switching and reopened tabs retain their paths', async ({ page }) => {
  await fixture(page);
  await address(page).fill('C:\\Alpha\\nested'); await address(page).press('Enter');
  await page.getByRole('region', { name: '패널 1', exact: true }).getByRole('button', { name: '새 탭', exact: true }).click();
  await expect(address(page)).toHaveValue('C:\\Alpha');
  await address(page).press('Control+w');
  await expect(address(page)).toHaveValue('C:\\Alpha\\nested');
  await address(page).press('Control+Shift+t');
  await expect(address(page)).toHaveValue('C:\\Alpha');
  await page.getByRole('tablist', { name: '프로젝트', exact: true }).getByRole('tab', { name: 'Beta', exact: true }).click();
  await address(page).press('Control+t');
  await expect(address(page)).toHaveValue('C:\\Beta');
  await page.getByRole('tablist', { name: '프로젝트', exact: true }).getByRole('tab', { name: 'Alpha', exact: true }).click();
  await expect(address(page)).toHaveValue('C:\\Alpha');
});

test('newly revealed empty panes use root and existing hidden panes preserve saved folders', async ({ page }) => {
  await fixture(page);
  await page.getByRole('button', { name: '4패널', exact: true }).click();
  await expect(address(page, 2)).toHaveValue('C:\\Alpha');
  await expect(address(page, 3)).toHaveValue('C:\\Alpha\\kept');
  await address(page, 2).fill('C:\\Alpha\\chosen'); await address(page, 2).press('Enter');
  await page.getByRole('button', { name: '1패널', exact: true }).click();
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await expect(address(page, 2)).toHaveValue('C:\\Alpha\\chosen');
});

test('unavailable root stays visible and reports an error instead of silently choosing another folder', async ({ page }) => {
  await fixture(page);
  await page.evaluate(() => Reflect.set(window, 'missingRoot', true));
  await page.getByRole('region', { name: '패널 1', exact: true }).getByRole('button', { name: '새 탭', exact: true }).click();
  await expect(address(page)).toHaveValue('C:\\Alpha');
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByRole('alert')).toBeVisible();
  await expect(page.getByRole('region', { name: '패널 1', exact: true }).getByRole('tab', { selected: true })).toHaveAttribute('data-source', 'real');
});

test('editing the project root preserves open folders and makes new tabs use the edited root', async ({ page }) => {
  await fixture(page);
  await address(page).fill('C:\\Alpha\\nested'); await address(page).press('Enter');
  await page.getByText('프로젝트 관리', { exact: true }).click();
  await page.getByRole('button', { name: '프로젝트 편집', exact: true }).click();
  await expect(page.getByRole('textbox', { name: '루트 경로', exact: true })).toHaveValue('C:\\Alpha');
  await page.getByRole('textbox', { name: '루트 경로', exact: true }).fill('D:\\Updated');
  await page.getByRole('button', { name: '변경하기', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Alpha', exact: true })).toBeEnabled();
  await expect(address(page)).toHaveValue('C:\\Alpha\\nested');
  await page.getByRole('button', { name: '2패널', exact: true }).click();
  await expect(address(page, 2)).toHaveValue('D:\\Updated');
  await address(page).press('Control+t');
  await expect(address(page)).toHaveValue('D:\\Updated');
  await page.getByRole('tab', { name: 'Beta', exact: true }).click();
  await page.getByRole('tab', { name: 'Alpha', exact: true }).click();
  await expect(address(page)).toHaveValue('D:\\Updated');
});
