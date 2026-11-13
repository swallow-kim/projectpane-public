import { afterEach, expect, it } from 'vitest';
import { setLanguage } from '../../shared/i18n';
import { projectErrorText } from './project-error-text';

afterEach(() => { setLanguage('ko'); });
it('translates a stored project error at render time and preserves unknown text', () => {
  const stored = '세션을 저장하지 못했습니다. 다시 시도하세요.';
  setLanguage('en');
  expect(projectErrorText(stored)).toBe('Could not save the session. Try again.');
  expect(projectErrorText('unchanged details')).toBe('unchanged details');
  expect(projectErrorText(null)).toBeNull();
  setLanguage('ko');
  expect(projectErrorText(stored)).toBe(stored);
});
