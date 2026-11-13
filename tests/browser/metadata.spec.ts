import { expect, test } from '@playwright/test';

test.use({ timezoneId: 'Asia/Seoul' });

test('shows readable file types and local modified dates with aligned scrollable columns', async ({ page }) => {
  await page.addInitScript(() => {
    const entries = [
      { id: 'file', name: '긴 한글 이름의 프로젝트 최종 보고서.PDF', kind: 'file', modifiedAtMs: Date.UTC(2020, 0, 2, 3, 4), sizeBytes: 1536 },
      { id: 'folder', name: '자료.txt', kind: 'folder', modifiedAtMs: null, sizeBytes: null },
      { id: 'epoch', name: '확장자없음', kind: 'file', modifiedAtMs: 0, sizeBytes: 0 },
    ];
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: { identity: unknown }) => {
      if (command === 'startup_config') return { folders: ['/fixture'], error: null, warnings: [] };
      if (command === 'cancel_listing') return;
      return { identity: args.identity, batch: { revision: 0, offset: 0, entries, total: entries.length, complete: true, capped: false, snapshotBytes: 4096, capReason: null } };
    } } });
  });
  await page.goto('/');
  const pane = page.getByRole('region', { name: '패널 1', exact: true });
  await expect(pane.locator('.column-heading > span')).toHaveText(['이름', '유형', '크기', '수정한 날짜']);
  const rows = pane.getByRole('listbox').getByRole('option');
  await expect(rows.nth(0).locator('.size')).toHaveText('1.5 KiB');
  await expect(rows.nth(1).locator('.size')).toHaveText('—');
  await expect(rows.nth(2).locator('.size')).toHaveText('0 B');
  await expect(rows.nth(0).locator('.kind')).toHaveText('PDF 문서');
  await expect(rows.nth(0).locator('.modified')).toHaveText('2020-01-02 12:04');
  await expect(rows.nth(0).locator('.file-name')).toHaveAttribute('title', '긴 한글 이름의 프로젝트 최종 보고서.PDF');
  await expect(rows.nth(1).locator('.kind')).toHaveText('폴더');
  await expect(rows.nth(1).locator('.modified')).toHaveText('—');
  await expect(rows.nth(1).locator('.modified')).toHaveAttribute('title', '수정한 날짜를 확인할 수 없습니다');
  await expect(rows.nth(2).locator('.modified')).toHaveText('1970-01-01 09:00');
  for (const width of [1280, 768]) {
    await page.setViewportSize({ width, height: 900 });
    const scroll = pane.locator('.listing-table');
    if (width === 768) {
      expect(await scroll.evaluate(element => element.scrollWidth > element.clientWidth)).toBe(true);
      await scroll.evaluate(element => { element.scrollLeft = element.scrollWidth; });
    }
    await expect(rows.nth(0).locator('.modified')).toBeInViewport();
    const positions = await pane.evaluate(element => ({
      headers: Array.from(element.querySelectorAll('.column-heading > span')).map(cell => cell.getBoundingClientRect().left),
      cells: Array.from(element.querySelectorAll('.file-row:first-child > span')).map(cell => cell.getBoundingClientRect().left),
    }));
    expect(positions.headers).toHaveLength(4);
    expect(positions.cells).toHaveLength(4);
    positions.headers.forEach((left, index) => expect(Math.abs(left - (positions.cells[index] ?? NaN))).toBeLessThanOrEqual(1));
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({ path: `artifacts/p2-20261005/browser-tests/ui-metadata-${width}.png`, fullPage: true });
  }
});
