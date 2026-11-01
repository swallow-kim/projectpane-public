import { describe, expect, it } from 'vitest';
import { formatModifiedAt, formatSize, typeLabel } from './metadata';

describe('file detail columns', () => {
  it('uses the native kind before the filename extension', () => {
    expect(typeLabel('folder', 'archive.zip')).toBe('폴더');
    expect(typeLabel('link', 'report.pdf')).toBe('링크');
    expect(typeLabel('unknown', 'report.pdf')).toBe('확인 불가');
    expect(typeLabel('file', '보고서.PDF')).toBe('PDF 문서');
    expect(typeLabel('file', 'notes.txt')).toBe('텍스트 문서');
    expect(typeLabel('file', 'custom.xyz')).toBe('XYZ 파일');
    expect(typeLabel('file', 'custom.constructor')).toBe('CONSTRUCTOR 파일');
    expect(typeLabel('file', 'custom.__proto__')).toBe('__PROTO__ 파일');
    expect(typeLabel('file', '.gitignore')).toBe('파일');
    expect(typeLabel('file', 'no-extension.')).toBe('파일');
  });
  it('formats local calendar fields without treating epoch zero as missing', () => {
    expect(formatModifiedAt(new Date(2026, 9, 4, 13, 7).getTime())).toBe('2026-10-04 13:07');
    expect(formatModifiedAt(0)).not.toBe('—');
    expect(formatModifiedAt(new Date(1969, 0, 2, 3, 4).getTime())).toBe('1969-01-02 03:04');
  });
  it('does not invent a date for missing or unusable metadata', () => {
    for (const value of [null, NaN, Infinity, 8_640_000_000_000_001]) expect(formatModifiedAt(value)).toBe('—');
  });
});
it('formats only file metadata sizes and preserves zero bytes', () => {
  expect(formatSize(null)).toBe('—');
  expect(formatSize(0)).toBe('0 B');
  expect(formatSize(1024)).toBe('1 KiB');
  expect(formatSize(1024 * 1024)).toBe('1 MiB');
});
