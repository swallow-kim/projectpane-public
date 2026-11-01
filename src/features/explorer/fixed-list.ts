// Fixed 26px rows need no per-entry measurement array. Work and allocation scale
// with the viewport, including while the backend grows a 100k-entry snapshot.
export const ROW_HEIGHT = 26;
export const OVERSCAN = 6;
export function visibleRange(count: number, scrollTop: number, height: number) {
  if (count === 0 || height <= 0) return { first: 0, end: 0 };
  const top = Math.max(0, Math.min(scrollTop, Math.max(0, count * ROW_HEIGHT - height)));
  return {
    first: Math.max(0, Math.floor(top / ROW_HEIGHT) - OVERSCAN),
    end: Math.min(count, Math.ceil((top + height) / ROW_HEIGHT) + OVERSCAN),
  };
}
export function revealRow(index: number, top: number, height: number) {
  const start = index * ROW_HEIGHT;
  if (start < top) return start;
  if (start + ROW_HEIGHT > top + height) return Math.max(0, start + ROW_HEIGHT - height);
  return top;
}
