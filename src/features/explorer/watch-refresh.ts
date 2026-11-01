import type { WatchUpdate } from '../../shared/watch-contracts';
export type WatchTarget = { readonly key: string; readonly path: string; readonly ready: boolean };
export type WatchPane = { readonly watchTarget?: () => WatchTarget | null; readonly refreshFiles?: () => void;
  readonly watchStatus?: (key: string, unavailable: boolean) => void };
export const WATCH_POLL_MS = 500;

// One request and at most four pending invalidations; no event/filename queue.
export function bindWatchRefresh(
  panes: () => readonly (WatchPane | null)[],
  enabled: () => boolean,
  ready: () => boolean,
  poll: (paths: readonly string[]) => Promise<WatchUpdate | null>,
  failed: () => void,
) {
  let disposed = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const pending = new Map<number, string>();
  const capture = () => panes().slice(0, 4).map(pane => enabled() ? pane?.watchTarget?.() ?? null : null);
  const tick = async () => {
    const before = capture();
    const slots = before.flatMap((target, index) => target ? [{ target, index }] : []);
    try {
      const update = await poll(slots.map(({ target }) => target.path));
      if (disposed) return;
      const current = capture();
      if (update) slots.forEach(({ target, index }, slot) => {
        if (current[index]?.key !== target.key) return;
        const unavailable = (update.unavailable & (1 << slot)) !== 0;
        panes()[index]?.watchStatus?.(target.key, unavailable);
        if (unavailable) pending.delete(index);
        else if (update.changed & (1 << slot)) pending.set(index, target.key);
      });
      for (const [index, key] of pending) {
        const target = current[index];
        if (!target || target.key !== key) pending.delete(index);
        else if (ready() && target.ready) {
          pending.delete(index);
          panes()[index]?.refreshFiles?.();
        }
      }
    } catch {
      if (!disposed) failed();
      // Terminal for this binding: no restart loop, existing F5/focus refresh remains available.
      return;
    }
    if (!disposed) timer = setTimeout(() => { void tick(); }, WATCH_POLL_MS);
  };
  timer = setTimeout(() => { void tick(); }, WATCH_POLL_MS);
  return { dispose() { disposed = true; clearTimeout(timer); pending.clear(); } };
}

export function watchablePath(path: string): boolean {
  return /^[a-z]:\\/i.test(path) && new TextEncoder().encode(path).length <= 32768
    && !/[\0/]/.test(path) && (path.length === 3 || path.slice(3).split('\\').every(part => part !== '' && part !== '.' && part !== '..' && !part.includes(':')));
}
