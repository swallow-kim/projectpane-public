import { expect, it } from 'vitest';
import { createPane, navigateTab, updateTab } from '../explorer/navigation';
import { paletteItems, searchPalette } from './palette';

it('takes only current state and deduplicates bounded recent paths without changing that state', () => {
  let pane = createPane('C:\\한글 폴더');
  let tab = pane.tabs[1]!;
  for (let n = 0; n < 40; n++) tab = navigateTab(tab, `C:\\폴더 ${n}`);
  pane = updateTab(pane, tab);
  const before = JSON.stringify(pane);
  const items = paletteItems([{ id: 'a', name: '개발 프로젝트' }], [pane, createPane('c:/폴더 39/')], [{ name: '공유', path: '\\\\server\\share' }], 0);
  expect(items.filter(x => x.group === '최근 폴더')).toHaveLength(32);
  expect(items.filter(x => x.group === '최근 폴더' && x.detail.toLowerCase().includes('폴더 39'))).toHaveLength(1);
  expect(items.find(x => x.group === '즐겨찾기')?.action).toEqual({ kind: 'folder', path: '\\\\server\\share' });
  expect(searchPalette(items, '프로젝트 개발')).toHaveLength(1);
  expect(searchPalette(items, '공유'.normalize('NFD'))[0]?.label).toBe('공유');
  expect(JSON.stringify(pane)).toBe(before);
});
it('limits display while still searching items beyond the first display page', () => {
  const items = paletteItems(Array.from({ length: 300 }, (_, n) => ({ id: String(n), name: `project-${n}` })), [], [], 0);
  expect(searchPalette(items, '')).toHaveLength(100);
  expect(searchPalette(items, 'project-299')[0]?.action).toEqual({ kind: 'project', id: '299' });
  expect(searchPalette(items, 'missing')).toEqual([]);
});
