import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { getLanguage, LANGUAGE_STORAGE_KEY, locale, parseLanguage, setLanguage, t } from './i18n';
import { faultMessage } from './fault';

beforeEach(() => { vi.stubGlobal('localStorage', { setItem: vi.fn(), getItem: vi.fn(() => null) }); setLanguage('ko'); });
afterEach(() => { setLanguage('ko'); vi.unstubAllGlobals(); });
describe('language preference', () => {
  it('keeps Korean as the existing-user default and rejects unsupported values', () => {
    for (const value of [null, undefined, '', 'fr', 'EN', {}, 123]) expect(parseLanguage(value)).toBe('ko');
    expect(parseLanguage('en')).toBe('en'); expect(parseLanguage('ko')).toBe('ko');
  });
  it('switches and persists both supported languages', () => {
    expect(setLanguage('en')).toBe(true); expect(getLanguage()).toBe('en'); expect(locale()).toBe('en-US');
    expect(localStorage.setItem).toHaveBeenCalledWith(LANGUAGE_STORAGE_KEY, 'en');
    expect(t('이름', 'Name')).toBe('Name');
    setLanguage('ko'); expect(t('이름', 'Name')).toBe('이름'); expect(locale()).toBe('ko-KR');
  });
  it('interpolates values verbatim without interpreting replacement syntax', () => {
    setLanguage('en'); expect(t('{name} {count}', '{count}: {name}', { name: '$&<한국어>{count}', count: 2 })).toBe('2: $&<한국어>{count}');
    expect(t('{missing}', '{missing}', {})).toBe('{missing}');
  });
  it('applies for the session even when storage is blocked', () => {
    vi.stubGlobal('localStorage', { setItem: () => { throw new Error('unavailable'); } });
    expect(setLanguage('en')).toBe(false); expect(t('이름', 'Name')).toBe('Name');
  });
  it('localizes structured errors without exposing native exception text', () => {
    expect(faultMessage('not_found')).toContain('폴더');
    setLanguage('en'); expect(faultMessage('not_found')).toBe('The folder was not found. Check the path.');
  });
});
