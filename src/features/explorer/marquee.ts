import { ROW_HEIGHT } from './fixed-list';

export type MarqueePoint = { readonly x: number; readonly y: number };
export type MarqueeRange = { readonly first: number; readonly last: number };
export type MarqueeMode = 'replace' | 'add' | 'toggle';
export type MarqueeSelection = {
  readonly range: MarqueeRange | null;
  readonly baseline: ReadonlySet<string>;
  readonly mode: MarqueeMode;
};

export function marqueeRange(start: MarqueePoint, end: MarqueePoint, count: number, rowWidth: number): MarqueeRange | null {
  const left = Math.min(start.x, end.x), right = Math.max(start.x, end.x);
  const top = Math.min(start.y, end.y), bottom = Math.max(start.y, end.y);
  if (left >= rowWidth || right <= 0 || left === right || top === bottom || !count) return null;
  const first = Math.max(0, Math.floor(top / ROW_HEIGHT));
  const last = Math.min(count - 1, Math.ceil(bottom / ROW_HEIGHT) - 1);
  return first <= last ? { first, last } : null;
}

export function marqueeSelected(baseline: boolean, intersects: boolean, mode: MarqueeMode): boolean {
  switch (mode) {
    case 'replace': return intersects;
    case 'add': return baseline || intersects;
    case 'toggle': return baseline !== intersects;
  }
}
