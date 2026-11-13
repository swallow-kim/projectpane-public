export const COLUMN_LABELS = ['이름', '유형', '크기', '수정한 날짜'] as const;
export const COLUMN_MIN = [140, 56, 64, 112] as const;
export const COLUMN_MAX = 1600;
export const DEFAULT_COLUMNS: readonly (number | null)[] = [null, 80, 96, 140];
export type ColumnWidths = readonly (number | null)[];
export const columnStorageKey = (pane: number) => `projectpane.columns.v1.${pane}`;
export function clampColumn(index: number, width: number): number {
  return Math.round(Math.min(COLUMN_MAX, Math.max(COLUMN_MIN[index], width)));
}
export function parseColumns(value: string | null): ColumnWidths {
  try {
    const parsed: unknown = JSON.parse(value ?? 'null');
    if (!Array.isArray(parsed) || parsed.length !== 4) return DEFAULT_COLUMNS;
    return parsed.map((width, index) => index === 0 && width === null ? null
      : typeof width === 'number' && Number.isFinite(width) ? clampColumn(index, width) : DEFAULT_COLUMNS[index]);
  } catch { return DEFAULT_COLUMNS; }
}
export function columnTemplate(widths: ColumnWidths): string {
  return widths.map((width, index) => width === null ? `minmax(${COLUMN_MIN[index]}px, 1fr)` : `${width}px`).join(' ');
}
export function columnMinimumWidth(widths: ColumnWidths): number {
  // 16px row padding + 24px gaps + 12px trailing space + 17px scrollbar gutter.
  return widths.reduce<number>((sum, width, index) => sum + (width ?? COLUMN_MIN[index]), 69);
}
