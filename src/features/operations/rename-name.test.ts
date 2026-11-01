import { expect, it } from 'vitest';
import { joinRenameName, splitRenameName, validRenameName } from './rename-name';

it.each([
  ['report.txt', 'report', 'txt'], ['한글 파일.TXT', '한글 파일', 'TXT'],
  ['.env', '.env', ''], ['README', 'README', ''], ['.env.local', '.env', 'local'],
  ['backup.tar.gz', 'backup', 'tar.gz'], ['archive.TAR.XZ', 'archive', 'TAR.XZ'],
  ['meeting.v2.final.docx', 'meeting.v2.final', 'docx'],
])('keeps the intended name and extension for %s', (name, stem, extension) => {
  expect(splitRenameName(name)).toEqual({ stem, extension });
  expect(joinRenameName(stem, extension)).toBe(name);
});
it('treats folders as one name and removing an extension literally removes it', () => {
  expect(splitRenameName('folder.with.dots', true)).toEqual({ stem: 'folder.with.dots', extension: '' });
  expect(joinRenameName('report', '')).toBe('report');
  expect(joinRenameName('folder', 'ignored', true)).toBe('folder');
});
it.each(['', '.', '..', 'CON', 'con.txt', 'CON .txt', 'LPT1', 'bad/name', 'bad\\name', 'file:stream', 'trailing.', 'trailing ', 'a?b', '\0', 'x'.repeat(256)])('rejects unsafe Windows filename %j', name => expect(validRenameName(name)).toBe(false));
it.each(['.env', 'archive.tar.gz', '대소문자.TXT', '😀.txt', 'x'.repeat(255)])('accepts a literal Windows leaf %j', name => expect(validRenameName(name)).toBe(true));
