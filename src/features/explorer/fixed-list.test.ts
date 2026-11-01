import { expect, it } from 'vitest';
import { OVERSCAN, revealRow, ROW_HEIGHT, visibleRange } from './fixed-list';

it('bounds visible work at 100k entries, including resize and stale scroll after shrink', () => {
  for (const count of [0, 1, 256, 100_000]) {
    for (const height of [0, 1, 300, 901]) {
      for (const top of [0, 1, 300, 2_999_999]) {
        const { first, end } = visibleRange(count, top, height);
        expect(first).toBeGreaterThanOrEqual(0);
        expect(end).toBeLessThanOrEqual(count);
        expect(end - first).toBeLessThanOrEqual(Math.ceil(height / ROW_HEIGHT) + 2 * OVERSCAN + 1);
        if (height && count) expect(end).toBeGreaterThan(first);
      }
    }
  }
  expect(visibleRange(100_000, 3_000_000, 300).end).toBe(100_000);
  expect(visibleRange(1, 3_000_000, 300)).toEqual({ first: 0, end: 1 });
});
it('reveals keyboard selection without moving an already visible row', () => {
  expect(revealRow(99_999, 0, 300)).toBe(2_599_700);
  expect(revealRow(0, 2_999_700, 300)).toBe(0);
  expect(revealRow(4, 90, 300)).toBe(90);
  expect(revealRow(15, 90, 300)).toBe(116);
});
