import { expect, it } from 'vitest';
import { clampColumn, columnMinimumWidth, columnTemplate, DEFAULT_COLUMNS, parseColumns } from './column-widths';
it('uses safe defaults for malformed or unavailable saved preferences', () => {
  for (const value of [null, '', 'null', '{}', '[10]', 'broken']) expect(parseColumns(value)).toEqual(DEFAULT_COLUMNS);
  expect(parseColumns('[null,80,96,140]')).toEqual(DEFAULT_COLUMNS);
  expect(parseColumns('["bad",-9,99999,null]')).toEqual([null,56,1600,140]);
});
it('clamps every column and retains a fluid name until explicitly resized', () => {
  expect(clampColumn(0, 1)).toBe(140); expect(clampColumn(3, 2000)).toBe(1600);
  expect(clampColumn(1, 80.7)).toBe(81);
  expect(columnTemplate(DEFAULT_COLUMNS)).toBe('minmax(140px, 1fr) 80px 96px 140px');
  expect(columnMinimumWidth([200,80,96,140])).toBe(585);
});
