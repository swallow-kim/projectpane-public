import { expect, it } from 'vitest';
import { parseWatchUpdate } from './watch-contracts';

it('rejects stale numeric protocol, overlapping/out-of-bounds masks and excess fields', () => {
  for (const value of [1, { changed: 1, unavailable: 1 }, { changed: 4, unavailable: 0 },
    { changed: 0, unavailable: 4 }, { changed: 0, unavailable: 0, path: 'private' }]) {
    expect(() => parseWatchUpdate(value, 2)).toThrow();
  }
  expect(parseWatchUpdate({ changed: 1, unavailable: 2 }, 2)).toEqual({ changed: 1, unavailable: 2 });
  expect(() => parseWatchUpdate({ changed: 0, unavailable: 0 }, 5)).toThrow();
});
