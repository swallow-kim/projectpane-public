import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { bindWatchRefresh, watchablePath, WATCH_POLL_MS, type WatchTarget } from './watch-refresh';
import type { WatchUpdate } from '../../shared/watch-contracts';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());
const tick = () => vi.advanceTimersByTimeAsync(WATCH_POLL_MS);
function fixture(poll: (paths: readonly string[]) => Promise<number | WatchUpdate>) {
  let target: WatchTarget | null = { key: 'a', path: 'C:\\a', ready: true };
  let ready = true;
  let enabled = true;
  const refreshFiles = vi.fn();
  const failed = vi.fn();
  const watchStatus = vi.fn();
  const binding = bindWatchRefresh(() => [{ watchTarget: () => target, refreshFiles, watchStatus }], () => enabled, () => ready,
    async paths => { const result = await poll(paths); return typeof result === 'number' ? { changed: result, unavailable: 0 } : result; }, failed);
  return { binding, refreshFiles, failed, watchStatus, busy: () => { ready = false; }, resume: () => { ready = true; },
    target: (value: WatchTarget | null) => { target = value; }, disable: () => { enabled = false; } };
}

it('defers a burst while busy then refreshes once', async () => {
  const poll = vi.fn().mockResolvedValue(1);
  const f = fixture(poll); f.busy();
  await tick(); await tick(); expect(f.refreshFiles).not.toHaveBeenCalled();
  poll.mockResolvedValue(0); f.resume(); await tick();
  expect(f.refreshFiles).toHaveBeenCalledTimes(1); f.binding.dispose();
});

it('discards a delayed response after the tab changes without overlapping requests', async () => {
  let finish: ((mask: number) => void) | undefined;
  const poll = vi.fn(() => new Promise<number>(resolve => { finish = resolve; }));
  const f = fixture(poll); await tick(); await tick();
  expect(poll).toHaveBeenCalledTimes(1);
  f.target({ key: 'b', path: 'C:\\b', ready: true }); finish?.(1); await Promise.resolve();
  expect(f.refreshFiles).not.toHaveBeenCalled(); f.binding.dispose();
});

it('keeps invalidations until the listing finishes and clears subscriptions when disabled', async () => {
  const poll = vi.fn().mockResolvedValue(1); const f = fixture(poll);
  f.target({ key: 'a', path: 'C:\\a', ready: false }); await tick();
  poll.mockResolvedValue(0); f.target({ key: 'a', path: 'C:\\a', ready: true }); await tick();
  expect(f.refreshFiles).toHaveBeenCalledTimes(1);
  f.disable(); await tick(); expect(poll).toHaveBeenLastCalledWith([]); f.binding.dispose();
});

it('stops after failure or disposal instead of restarting', async () => {
  const poll = vi.fn().mockRejectedValue(new Error('worker exited')); const f = fixture(poll);
  await tick(); await tick(); expect(poll).toHaveBeenCalledTimes(1); expect(f.failed).toHaveBeenCalledOnce();
  f.binding.dispose(); expect(vi.getTimerCount()).toBe(0);
});

it('never applies an outstanding result after disposal', async () => {
  let finish: ((mask: number) => void) | undefined;
  const f = fixture(() => new Promise(resolve => { finish = resolve; }));
  await tick(); f.binding.dispose(); finish?.(1); await Promise.resolve();
  expect(f.refreshFiles).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
});

it('admits bounded drive paths and excludes unsupported network or ambiguous paths', () => {
  for (const path of ['C:\\', 'D:\\한글\\folder']) expect(watchablePath(path)).toBe(true);
  for (const path of ['\\\\server\\share', 'C:\\a\\', 'C:\\a\\..', 'C:/a', 'C:\\a:stream', 'C:\\a\0', 'C:\\' + 'a'.repeat(32768)]) expect(watchablePath(path)).toBe(false);
});

it('discards stale failure status after a tab changes', async () => {
  let finish: ((update: WatchUpdate) => void) | undefined;
  const f = fixture(() => new Promise<WatchUpdate>(resolve => { finish = resolve; }));
  await tick(); f.target({ key: 'b', path: 'C:\\b', ready: true });
  finish?.({ changed: 0, unavailable: 1 }); await Promise.resolve(); await Promise.resolve();
  expect(f.watchStatus).not.toHaveBeenCalled(); expect(f.failed).not.toHaveBeenCalled(); f.binding.dispose();
});

it('keeps a healthy pane refreshing while another subscription is unavailable', async () => {
  const healthy = vi.fn(), unavailable = vi.fn(), failed = vi.fn();
  const binding = bindWatchRefresh(() => [
    { watchTarget: () => ({ key: 'a', path: 'C:\\a', ready: true }), refreshFiles: healthy },
    { watchTarget: () => ({ key: 'b', path: 'C:\\b', ready: true }), watchStatus: unavailable },
  ], () => true, () => true, async () => ({ changed: 1, unavailable: 2 }), failed);
  await tick(); await tick();
  expect(healthy).toHaveBeenCalledTimes(2); expect(unavailable).toHaveBeenLastCalledWith('b', true);
  expect(failed).not.toHaveBeenCalled(); binding.dispose();
});
