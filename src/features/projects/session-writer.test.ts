import { deferred } from './test-deferred';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { SessionWriter } from './session-writer';

afterEach(() => vi.useRealTimers());
describe('serial session writer', () => {
  it('debounces edits for 750ms and saves only the newest workspace', async () => {
    vi.useFakeTimers();
    const save = vi.fn(async (revision: number, _value: string) => revision + 1);
    const writer = new SessionWriter(save, 4);
    writer.changed('old'); writer.changed('new');
    await vi.advanceTimersByTimeAsync(749);
    expect(save).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(save).toHaveBeenCalledExactlyOnceWith(4, 'new');
    writer.dispose();
  });
  it('serializes changes arriving during a save with the returned revision', async () => {
    const first = deferred<number>();
    const save = vi.fn<(revision: number, value: string) => Promise<number>>()
      .mockReturnValueOnce(first.promise).mockResolvedValueOnce(12);
    const writer = new SessionWriter(save, 10);
    writer.changed('first');
    const flushing = writer.flush();
    writer.changed('second'); writer.changed('latest');
    expect(save).toHaveBeenCalledTimes(1);
    first.resolve(11);
    await flushing;
    expect(save.mock.calls).toEqual([[10, 'first'], [11, 'latest']]);
    writer.dispose();
  });
  it('retains dirty data after a failed save for an explicit retry', async () => {
    const save = vi.fn<(revision: number, value: string) => Promise<number>>()
      .mockRejectedValueOnce(new Error('conflict')).mockResolvedValueOnce(2);
    const writer = new SessionWriter(save, 1);
    writer.changed('keep');
    await expect(writer.flush()).rejects.toThrow('conflict');
    await writer.flush();
    expect(save.mock.calls).toEqual([[1, 'keep'], [1, 'keep']]);
    writer.dispose();
  });
  it('bounds closing time without pretending an unfinished save succeeded', async () => {
    vi.useFakeTimers();
    const pending = deferred<number>();
    const writer = new SessionWriter(() => pending.promise, 0);
    writer.changed('pending');
    const closing = writer.flushWithin(1000);
    await vi.advanceTimersByTimeAsync(1000);
    expect(await closing).toBe(false);
    pending.resolve(1);
    writer.dispose();
  });
  it('does not send queued old workspace edits after disposal', async () => {
    const pending = deferred<number>();
    const save = vi.fn(() => pending.promise);
    const writer = new SessionWriter(save, 0);
    writer.changed('old');
    const flushing = writer.flush();
    writer.changed('queued'); writer.dispose(); pending.resolve(1);
    await flushing;
    expect(save).toHaveBeenCalledTimes(1);
  });
});
