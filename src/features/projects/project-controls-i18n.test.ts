import { afterEach, expect, it } from 'vitest';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { setLanguage } from '../../shared/i18n';
import { ProjectToolbar, type ProjectToolbarProps } from './ProjectToolbar';
import { UpdateControl } from '../updates/UpdateControl';

afterEach(() => { setLanguage('ko'); });
const noop = () => {};
const props: ProjectToolbarProps = {
  projects: [{ id: 'p', name: '업데이트', rootPath: 'C:\\작업' }],
  selectedId: 'p', onSelect: noop, onCreate: noop, onEdit: noop, onDuplicate: noop,
  onRemoveFromList: noop, onSaveCurrentAsPreset: noop, onRestorePreset: noop, onRetry: noop,
};
it('renders project controls in either language without translating a user project name', () => {
  setLanguage('en');
  const english = renderToStaticMarkup(createElement(ProjectToolbar, props));
  expect(english).toContain('Manage projects');
  expect(english).toContain('New project');
  expect(english).toContain('Edit project');
  expect(english).toContain('Restore preset');
  expect(english).toContain('업데이트');
  setLanguage('ko');
  const korean = renderToStaticMarkup(createElement(ProjectToolbar, props));
  expect(korean).toContain('프로젝트 관리');
  expect(korean).toContain('새 프로젝트');
});
it('renders update safety instructions and controls in English', () => {
  setLanguage('en');
  const html = renderToStaticMarkup(createElement(UpdateControl, { ready: false, persisted: false, onPreparing: noop }));
  expect(html).toContain('Check for updates');
  expect(html).toContain('Installation not confirmed');
  expect(html).toContain('In-app installation is unavailable for ZIP');
  expect(html).toContain('Downloads and installs only when you choose to.');
  expect(html).not.toMatch(/[가-힣]/);
});
