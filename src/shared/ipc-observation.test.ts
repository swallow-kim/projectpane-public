import { afterAll, beforeAll, beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import fixture from '../../tests/fixtures/ipc-contract.json';
import { parseIdentity, type Transport } from './contracts';
import { IpcFault } from './fault';
import { DEFAULT_VIEW } from './listing-view';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
const browser: Partial<Window> = {};
const identity = parseIdentity(fixture.validReply.identity);
let transport: Transport;
beforeAll(async () => {
  vi.stubGlobal('window', browser);
  transport = (await import('./ipc')).transport;
});
beforeEach(() => {
  vi.resetAllMocks();
  browser.__PROJECTPANE_IPC_OBSERVATION__?.disable();
});
afterAll(() => vi.unstubAllGlobals());
function observer() {
  const control = browser.__PROJECTPANE_IPC_OBSERVATION__;
  if (!control) throw new Error('Observer control surface unavailable');
  return control;
}
function deferred() {
  let resolve: (value: unknown) => void = () => { throw new Error('Uninitialized promise'); };
  let reject: (error: unknown) => void = () => { throw new Error('Uninitialized promise'); };
  const promise = new Promise<unknown>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}
function failEventAppend(): () => void {
  const original = Array.prototype.push;
  Array.prototype.push = function <T>(this: T[], ...rows: T[]): number {
    for (const row of rows) {
      if (typeof row === 'object' && row !== null && 'command' in row && 'phase' in row) {
        throw new Error('observer-only');
      }
    }
    return original.apply(this, rows);
  };
  return () => { Array.prototype.push = original; };
}

it('retains no events or counters when disabled normal requests complete', async () => {
  vi.mocked(invoke).mockResolvedValue(null);
  await transport.cancel(identity);
  expect(observer().snapshot()).toMatchObject({ enabled: false, events: [], dropped: 0 });
  expect(observer().snapshot().commands.every(row => row.begin + row.success + row.failure === 0)).toBe(true);
  expect(invoke).toHaveBeenCalledExactlyOnceWith('cancel_listing', { identity });
});

it('counts begin immediately and completion only after invoke settles', async () => {
  const pending = deferred();
  vi.mocked(invoke).mockReturnValue(pending.promise);
  observer().enable();
  const request = transport.cancel(identity);
  expect(observer().snapshot().events.map(row => row.phase)).toEqual(['begin']);
  pending.resolve(null);
  await request;
  const rows = observer().snapshot().events;
  expect(rows.map(row => row.phase)).toEqual(['begin', 'success']);
  expect(rows[1]?.callId).toBe(rows[0]?.seq);
  expect(observer().snapshot().commands.find(row => row.command === 'cancel_listing')).toMatchObject({ begin: 1, success: 1, failure: 0 });
});

it('records actual completion order across pane and tab identities', async () => {
  const first = deferred(), second = deferred();
  const other = parseIdentity({ ...identity, pane: 3, tabId: 'inactive-tab', generation: 2 });
  vi.mocked(invoke).mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  observer().enable();
  const requests = [transport.cancel(identity), transport.cancel(other)];
  second.resolve(null);
  await requests[1];
  first.resolve(null);
  await requests[0];
  const rows = observer().snapshot().events;
  expect(rows.map(row => [row.phase, row.tabId, row.generation])).toEqual([
    ['begin', identity.tabId, identity.generation], ['begin', other.tabId, 2],
    ['success', other.tabId, 2], ['success', identity.tabId, identity.generation],
  ]);
  expect(rows.map(row => row.seq)).toEqual([...rows.map(row => row.seq)].sort((a, b) => a - b));
  expect(rows[2]?.callId).toBe(rows[1]?.callId);
  expect(rows[3]?.callId).toBe(rows[0]?.callId);
});

it('records all six normal commands without retaining sensitive arguments or replies', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValueOnce(fixture.validReply).mockResolvedValueOnce(fixture.validReply)
    .mockResolvedValueOnce(null).mockResolvedValueOnce(fixture.validReply)
    .mockResolvedValueOnce({ identity, revision: 0, index: null })
    .mockResolvedValueOnce({ identity, kind: 'folder', path: '/private-reply-path' });
  await transport.start(identity, '/private-input-path');
  await transport.read(identity, null);
  await transport.cancel(identity);
  await transport.setView(identity, DEFAULT_VIEW);
  await transport.locate(identity, 'private-entry-id');
  await transport.activate(identity, 'private-entry-id');
  const snapshot = observer().snapshot();
  expect(snapshot.events).toHaveLength(12);
  expect(snapshot.commands).toHaveLength(6);
  expect(snapshot.commands.every(row => row.begin === 1 && row.success === 1 && row.failure === 0)).toBe(true);
  for (const row of snapshot.events) {
    expect(Object.keys(row).sort()).toEqual(['callId', 'command', 'generation', 'pane', 'phase', 'seq', 'tabId']);
  }
  expect(JSON.stringify(snapshot)).not.toMatch(/private-|requestId|entryId|filename|path|entries|payload/);
  expect(invoke).toHaveBeenCalledTimes(6);
});

it('records rejection before fault conversion without retaining error strings', async () => {
  observer().enable();
  vi.mocked(invoke).mockRejectedValue({ code: 'timeout' });
  await expect(transport.read(identity, null)).rejects.toEqual(new IpcFault('timeout'));
  vi.mocked(invoke).mockRejectedValue(new Error('/private-error-path'));
  await expect(transport.cancel(identity)).rejects.toEqual(new IpcFault('ipc_error'));
  expect(observer().snapshot().events.map(row => row.phase)).toEqual(['begin', 'failure', 'begin', 'failure']);
  expect(JSON.stringify(observer().snapshot())).not.toMatch(/timeout|private|ipc_error|message|stack/);
});

it('counts resolved malformed replies as transport successes', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValue({ invalid: 'private-payload' });
  await expect(transport.read(identity, null)).rejects.toEqual(new IpcFault('protocol_error'));
  expect(observer().snapshot().events.map(row => row.phase)).toEqual(['begin', 'success']);
});

it('counts synchronous invoke throws as failures', async () => {
  observer().enable();
  vi.mocked(invoke).mockImplementation(() => { throw new IpcFault('busy'); });
  await expect(transport.cancel(identity)).rejects.toEqual(new IpcFault('busy'));
  expect(observer().snapshot().events.map(row => row.phase)).toEqual(['begin', 'failure']);
});

it('ignores invalid input rejected before the invoke boundary', async () => {
  observer().enable();
  await expect(transport.start(identity, '')).rejects.toEqual(new IpcFault('invalid_path'));
  await expect(transport.read(identity, -1)).rejects.toEqual(new IpcFault('invalid_request'));
  expect(invoke).not.toHaveBeenCalled();
  expect(observer().snapshot().events).toEqual([]);
});

it('bounds rows at 1024 and keeps exact finite aggregates after dropping rows', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValue(null);
  for (let index = 0; index < 600; index++) await transport.cancel(identity);
  const snapshot = observer().snapshot();
  expect(snapshot.events).toHaveLength(1024);
  expect(snapshot.dropped).toBe(176);
  expect(snapshot.lastSeq - (snapshot.events[0]?.seq ?? 0)).toBe(1199);
  expect(snapshot.commands.find(row => row.command === 'cancel_listing')).toMatchObject({ begin: 600, success: 600, failure: 0 });
  expect(snapshot.commands.every(row => [row.begin, row.success, row.failure].every(Number.isSafeInteger))).toBe(true);
});

it.each(['reset', 'disable'] as const)('discards old pending completions after %s', async action => {
  const pending = deferred();
  vi.mocked(invoke).mockReturnValueOnce(pending.promise).mockResolvedValue(null);
  observer().enable();
  const old = transport.cancel(identity);
  if (action === 'disable') observer().disable();
  observer().enable();
  await transport.cancel(identity);
  const before = observer().snapshot();
  pending.resolve(null);
  await old;
  expect(observer().snapshot()).toEqual(before);
});

it('ignores a request begun disabled even when it completes after enable', async () => {
  const pending = deferred();
  vi.mocked(invoke).mockReturnValue(pending.promise);
  const old = transport.cancel(identity);
  observer().enable();
  pending.resolve(null);
  await old;
  expect(observer().snapshot().events).toEqual([]);
});

it('releases all retained rows and counters on disable', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValue(null);
  await transport.cancel(identity);
  const lastSeq = observer().snapshot().lastSeq;
  observer().disable();
  expect(observer().snapshot()).toMatchObject({ enabled: false, events: [], dropped: 0, lastSeq });
  expect(observer().snapshot().commands.every(row => row.begin + row.success + row.failure === 0)).toBe(true);
});

it('returns detached frozen snapshots that cannot mutate observer state', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValue(null);
  await transport.cancel(identity);
  const snapshot = observer().snapshot();
  expect(Reflect.set(snapshot, 'dropped', 999)).toBe(false);
  expect(Reflect.set(snapshot.events, '0', null)).toBe(false);
  for (const row of snapshot.events) expect(Reflect.set(row, 'tabId', 'changed')).toBe(false);
  for (const row of snapshot.commands) expect(Reflect.set(row, 'begin', 999)).toBe(false);
  await transport.cancel(identity);
  expect(snapshot.events).toHaveLength(2);
  expect(observer().snapshot().events).toHaveLength(4);
  expect(observer().snapshot().dropped).toBe(0);
});

it('does not observe direct benchmark invokes', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValue(null);
  await invoke('cancel_listing', { identity });
  expect(observer().snapshot().events).toEqual([]);
});

it.each(['success', 'failure'] as const)('preserves %s when completion observation throws', async phase => {
  const pending = deferred();
  vi.mocked(invoke).mockReturnValue(pending.promise);
  observer().enable();
  const request = transport.cancel(identity);
  const outcome = phase === 'success'
    ? expect(request).resolves.toBeUndefined() : expect(request).rejects.toEqual(new IpcFault('timeout'));
  const restore = failEventAppend();
  if (phase === 'success') pending.resolve(null); else pending.reject({ code: 'timeout' });
  try { await outcome; } finally { restore(); }
  expect(observer().snapshot()).toMatchObject({ enabled: false, events: [], observerErrors: 1 });
});

it('still invokes the application transport when begin observation throws', async () => {
  observer().enable();
  vi.mocked(invoke).mockResolvedValue(null);
  const restore = failEventAppend();
  try { await transport.cancel(identity); } finally { restore(); }
  expect(invoke).toHaveBeenCalledExactlyOnceWith('cancel_listing', { identity });
  expect(observer().snapshot()).toMatchObject({ enabled: false, events: [], observerErrors: 1 });
});
