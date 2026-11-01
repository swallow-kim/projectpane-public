import { expect, it } from 'vitest';
import { marqueeRange, marqueeSelected } from './marquee';
import { ROW_HEIGHT as H } from './fixed-list';

it('maps intersected content rows independently of virtual DOM and drag direction', () => {
  expect(marqueeRange({ x: 390, y: H * 100 }, { x: 10, y: H * 102 }, 100000, 388)).toEqual({ first: 100, last: 101 });
  expect(marqueeRange({ x: 10, y: H * 102 }, { x: 390, y: H * 100 }, 100000, 388)).toEqual({ first: 100, last: 101 });
  expect(marqueeRange({ x: 390, y: -20 }, { x: 1, y: H * 20 }, 3, 388)).toEqual({ first: 0, last: 2 });
});
it('does not select for a gutter-only rectangle, zero area, or empty/outside rows', () => {
  for (const [start, end, count] of [
    [{ x: 390, y: 0 }, { x: 400, y: 100 }, 10],
    [{ x: 5, y: 10 }, { x: 5, y: 100 }, 10],
    [{ x: 390, y: H * 10 }, { x: 10, y: H * 11 }, 10],
    [{ x: 390, y: 0 }, { x: 10, y: 100 }, 0],
  ] as const) expect(marqueeRange(start, end, count, 388)).toBeNull();
});
it('previews replace, Shift union and Ctrl symmetric difference', () => {
  expect([false, true].flatMap(base => [false, true].map(hit => marqueeSelected(base, hit, 'replace')))).toEqual([false, true, false, true]);
  expect([false, true].flatMap(base => [false, true].map(hit => marqueeSelected(base, hit, 'add')))).toEqual([false, true, true, true]);
  expect([false, true].flatMap(base => [false, true].map(hit => marqueeSelected(base, hit, 'toggle')))).toEqual([false, true, true, false]);
});
