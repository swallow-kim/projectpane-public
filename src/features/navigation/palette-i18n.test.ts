import { afterEach, expect, it } from 'vitest';
import { setLanguage } from '../../shared/i18n';
import { createPane } from '../explorer/navigation';
import { localizePalette, paletteItems, searchPalette } from './palette';

afterEach(() => { setLanguage('ko'); });

it('translates a previously opened palette without changing user names, paths, or actions', () => {
  setLanguage('ko');
  const pane = createPane('C:\\한글 폴더');
  const items = paletteItems([{ id: 'a', name: '한국어 프로젝트' }], [pane], [{ name: '자료', path: 'C:\\자료' }], 0);
  const actions = items.map(item => item.action);
  setLanguage('en');
  const english = localizePalette(items);
  expect(english.find(item => item.id === 'project-a')).toMatchObject({ group: 'Projects', label: '한국어 프로젝트', detail: 'Switch to a saved project' });
  expect(english.find(item => item.action.kind === 'tab')?.detail).toContain('Panel 1');
  expect(english.filter(item => item.action.kind === 'tab').map(item => item.detail).some(detail => detail.includes('C:\\한글 폴더'))).toBe(true);
  expect(english.find(item => item.id.startsWith('favorite-'))).toMatchObject({ group: 'Favorites', label: '자료', detail: 'C:\\자료' });
  expect(searchPalette(english, 'reopen closed tab')[0]?.action).toEqual({ kind: 'command', command: 'reopen-tab' });
  expect(english.map(item => item.action)).toEqual(actions);
  setLanguage('ko');
  const korean = localizePalette(items);
  expect(searchPalette(korean, '닫은 탭 복원')[0]?.action).toEqual({ kind: 'command', command: 'reopen-tab' });
  expect(korean.find(item => item.id === 'project-a')?.group).toBe('프로젝트');
});

