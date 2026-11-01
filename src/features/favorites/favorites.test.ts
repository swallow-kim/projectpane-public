import { describe, expect, it } from 'vitest';
import { favoriteKey, parseFavorites } from './favorites';

describe('global favorites contract', () => {
  it('normalizes lexical Windows aliases without requiring a live folder', () => {
    expect(favoriteKey('c:/Work/../Docs/')).toBe(favoriteKey('C:\\docs'));
    expect(parseFavorites({ schemaVersion: 1, revision: 2, folders: [{ name: 'Offline', path: '\\\\offline\\share\\folder' }] }).folders).toHaveLength(1);
  });
  it('rejects duplicate paths, unsafe paths, unknown schema and oversized lists', () => {
    const make = (folders: unknown[]) => ({ schemaVersion: 1, revision: 0, folders });
    expect(() => parseFavorites(make([{ name: 'A', path: 'C:/Work' }, { name: 'B', path: 'c:\\work\\.' }]))).toThrow();
    expect(() => parseFavorites(make([{ name: 'A', path: 'C:relative' }]))).toThrow();
    expect(() => parseFavorites({ ...make([]), schemaVersion: 2 })).toThrow();
    expect(() => parseFavorites(make(Array.from({ length: 129 }, (_, i) => ({ name: 'A', path: `C:/Work${i}` }))))).toThrow();
  });
});
