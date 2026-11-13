import { afterEach, expect, it } from 'vitest';
import { setLanguage } from '../../shared/i18n';
import { formatModifiedAt, formatSize, typeLabel } from './metadata';
import { createTab, tabLabel } from './navigation';
import { listingStatus } from './ListingFeedback';
import type { ListingState } from './listing';

afterEach(() => { setLanguage('ko'); });

it('translates file type descriptions at call time without altering the filename', () => {
  const name = '보고서.PDF';
  setLanguage('en');
  expect(typeLabel('file', name)).toBe('PDF document');
  expect(typeLabel('folder', '폴더')).toBe('Folder');
  expect(typeLabel('link', '링크')).toBe('Link');
  expect(typeLabel('unknown', name)).toBe('Unknown');
  expect(typeLabel('file', 'custom.xyz')).toBe('XYZ file');
  expect(typeLabel('file', 'custom.constructor')).toBe('CONSTRUCTOR file');
  expect(typeLabel('file', '.gitignore')).toBe('File');
  expect(name).toBe('보고서.PDF');
  setLanguage('ko');
  expect(typeLabel('file', name)).toBe('PDF 문서');
});

it('uses localized reading and empty-folder statuses after a language change', () => {
  const loading = { status: 'loading', total: 2 } as ListingState;
  setLanguage('en');
  expect(listingStatus(loading)).toBe('Reading… · Showing partial results');
  expect(listingStatus({ status: 'complete', total: 0 } as ListingState)).toBe('Empty folder');
  expect(listingStatus({ status: 'complete', total: 2, viewPending: true } as ListingState)).toBe('Applying sort and filter…');
  setLanguage('ko');
  expect(listingStatus(loading)).toBe('읽는 중… · 일부 목록 표시');
});

it('keeps unambiguous local dates and binary size units in either language', () => {
  const time = new Date(2026, 10, 13, 9, 5).getTime();
  for (const language of ['ko', 'en'] as const) {
    setLanguage(language);
    expect(formatModifiedAt(time)).toBe('2026-11-13 09:05');
    expect(formatSize(1536)).toBe('1.5 KiB');
    expect(formatModifiedAt(null)).toBe('—');
  }
});


it('translates only generated tab labels and preserves user folder names', () => {
  const folder = createTab('C:\\Work\\한국어 폴더');
  const empty = createTab();
  const synthetic = createTab('', 'synthetic');
  setLanguage('en');
  expect(tabLabel(folder)).toBe('한국어 폴더');
  expect(tabLabel(empty)).toBe('Folder');
  expect(tabLabel(synthetic)).toBe('100,000 items');
  setLanguage('ko');
  expect(tabLabel(empty)).toBe('실제 폴더');
  expect(tabLabel(synthetic)).toBe('10만 항목');
});
