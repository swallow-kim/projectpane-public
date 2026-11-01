import { afterEach, beforeEach, expect, it, vi } from 'vitest';
import { bindFocusRefresh, FOCUS_REFRESH_DELAY_MS } from './focus-refresh';

function fixture(hidden = false) {
  const host = new EventTarget();
  Object.assign(host, { setTimeout, clearTimeout });
  const page = Object.assign(new EventTarget(), { hidden });
  let ready = true;
  const refresh = vi.fn();
  const binding = bindFocusRefresh(host as Window, page as unknown as Document, refresh, () => ready);
  return { host, page, refresh, binding, busy: (value: boolean) => { ready = !value; },
    event: (type: string) => host.dispatchEvent(new Event(type)),
    visibility: (value: boolean) => { page.hidden = value; page.dispatchEvent(new Event('visibilitychange')); },
    tick: () => vi.advanceTimersByTime(FOCUS_REFRESH_DELAY_MS),
  };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

it('ignores initial and duplicate focus, coalesces a return burst into one refresh', () => {
  const f = fixture();
  f.event('focus'); f.tick(); expect(f.refresh).not.toHaveBeenCalled();
  f.event('blur');
  for (let i = 0; i < 100; i++) { f.event('focus'); f.visibility(false); }
  expect(vi.getTimerCount()).toBe(1);
  f.tick(); expect(f.refresh).toHaveBeenCalledTimes(1);
  f.event('focus'); f.tick(); expect(f.refresh).toHaveBeenCalledTimes(1);
  f.binding.dispose();
});

it('cancels pending work on leaving and waits until visible again', () => {
  const f = fixture();
  f.event('blur'); f.event('focus'); f.visibility(true); f.tick();
  expect(f.refresh).not.toHaveBeenCalled();
  f.event('focus'); f.tick(); expect(f.refresh).not.toHaveBeenCalled();
  f.visibility(false); f.tick(); expect(f.refresh).toHaveBeenCalledTimes(1);
  f.binding.dispose();
});

it('does not poll busy workflows and resumes only from an explicit state change', () => {
  const f = fixture(); f.busy(true);
  f.event('blur'); f.event('focus'); f.tick();
  expect(f.refresh).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
  f.busy(false); f.binding.resume(); f.tick(); expect(f.refresh).toHaveBeenCalledTimes(1);
  f.binding.dispose();
});

it('waits for composition completion without consuming the pending return', () => {
  const f = fixture(); f.busy(true); f.event('blur'); f.event('focus'); f.tick();
  f.busy(false); f.event('compositionend'); f.tick();
  expect(f.refresh).toHaveBeenCalledTimes(1); f.binding.dispose();
});

it('removes timers and listeners on disposal, including initial hidden windows', () => {
  const f = fixture(true); f.visibility(false);
  f.binding.dispose(); f.tick(); f.event('blur'); f.event('focus'); f.binding.resume(); f.tick();
  expect(f.refresh).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
});

it('an operation refresh consumes a pending return without losing a future return', () => {
  const f = fixture(); f.event('blur'); f.event('focus');
  f.binding.acknowledge(); f.tick();
  expect(f.refresh).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
  f.event('blur'); f.binding.acknowledge(); f.event('focus'); f.tick();
  expect(f.refresh).toHaveBeenCalledTimes(1); f.binding.dispose();
});
