import { beforeEach, expect, it, vi } from 'vitest';
import { deferred } from './test-deferred';
import { listenForProjectClose } from './project-close';

const ipc = vi.hoisted(() => ({
  handlers: new Map<string, (event: { payload: unknown }) => void>(),
  invoke: vi.fn().mockResolvedValue(undefined),
}));
vi.mock('@tauri-apps/api/core', () => ({ invoke: ipc.invoke }));
vi.mock('@tauri-apps/api/event', () => ({ listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
  ipc.handlers.set(name, handler);
  return () => ipc.handlers.delete(name);
} }));
const request = (attempt: number) => ipc.handlers.get('projectpane-close-requested')?.({ payload: { attempt, timeoutMs: 2500 } });
const fail = (attempt: number) => ipc.handlers.get('projectpane-close-failed')?.({ payload: { attempt } });
beforeEach(() => { ipc.handlers.clear(); ipc.invoke.mockClear(); });

it('a timeout invalidates a delayed successful flush; a new request can retry', async () => {
  const first = deferred<boolean>();
  const flush = vi.fn().mockReturnValueOnce(first.promise).mockResolvedValueOnce(true);
  const failed = vi.fn();
  const stop = await listenForProjectClose(() => true, flush, failed);
  request(1); fail(1); request(2);
  first.resolve(true);
  await vi.waitFor(() => expect(ipc.invoke).toHaveBeenCalledExactlyOnceWith('project_close_ready', { attempt: 2, saved: true }));
  expect(failed).toHaveBeenCalledTimes(1);
  expect(flush).toHaveBeenCalledTimes(2);
  stop();
});

it('late failure notices and duplicate requests cannot invalidate the newer flush', async () => {
  const pending = deferred<boolean>();
  const flush = vi.fn().mockResolvedValueOnce(false).mockReturnValueOnce(pending.promise);
  const stop = await listenForProjectClose(() => true, flush, vi.fn());
  request(1);
  await vi.waitFor(() => expect(ipc.invoke).toHaveBeenCalledTimes(1));
  request(2); request(2); fail(1); request(1);
  pending.resolve(true);
  await vi.waitFor(() => expect(ipc.invoke).toHaveBeenLastCalledWith('project_close_ready', { attempt: 2, saved: true }));
  expect(flush).toHaveBeenCalledTimes(2);
  stop();
});

it('disposed listeners never acknowledge a pending save', async () => {
  const pending = deferred<boolean>();
  const stop = await listenForProjectClose(() => true, () => pending.promise, vi.fn());
  request(1); stop(); pending.resolve(true);
  await pending.promise;
  expect(ipc.invoke).not.toHaveBeenCalled();
  expect(ipc.handlers.size).toBe(0);
});
