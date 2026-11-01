import { describe, expect, it } from 'vitest';
import { nativeDrop, normalizeExplorerPath } from './native-drop';
describe('native drop boundary', () => {
  it('normalizes only file prefixes without URI decoding or unsafe verbatim interpretation', () => {
    expect(normalizeExplorerPath(String.raw`\\?\UNC\wsl.localhost\Ubuntu\한글%20.txt`)).toBe(String.raw`\\wsl.localhost\Ubuntu\한글%20.txt`);
    expect(normalizeExplorerPath(String.raw`\\?\C:\test\a.txt`)).toBe(String.raw`C:\test\a.txt`);
    for (const bad of [String.raw`\\?\GLOBALROOT\Device\x`, String.raw`\\.\PhysicalDrive0`, String.raw`\\?\UNC\wsl$\Ubuntu\a\..\b`, 'file://wsl.localhost/Ubuntu/a']) expect(() => normalizeExplorerPath(bad)).toThrow();
  });
  it('preserves WSL Unicode and verbatim paths and scales physical coordinates once', () => {
    const paths = [String.raw`\\wsl.localhost\Ubuntu\home\example-user\한글 space.txt`, String.raw`\\?\UNC\wsl$\Ubuntu\home\example-user\literal%20.txt`];
    expect(nativeDrop({ position: { x: 180, y: 300 }, paths }, 1.5, true)).toEqual({ x: 120, y: 200, paths });
  });
  it.each([null, {}, { position: { x: NaN, y: 1 } }, { position: { x: 1, y: 2 } },
    { position: { x: 1, y: 2 }, paths: [] }, { position: { x: 1, y: 2 }, paths: [null] },
    { position: { x: 1, y: 2 }, paths: ['bad\0path'] }])('rejects malformed drop without clipboard fallback: %j', payload => {
    expect(nativeDrop(payload, 1, true)).toBeNull();
  });
  it('allows pathless hover but rejects invalid DPI', () => {
    expect(nativeDrop({ position: { x: 2, y: 4 } }, 2, false)).toEqual({ x: 1, y: 2, paths: [] });
    expect(nativeDrop({ position: { x: 2, y: 4 }, paths: ['C:\\ok'] }, 0, true)).toBeNull();
  });
});
