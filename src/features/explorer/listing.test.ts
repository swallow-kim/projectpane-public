import { describe, expect, it, vi } from 'vitest';
import { ListingController } from './listing';
import { PageCache } from './page-cache';
import { syntheticRow } from './synthetic';
import { CACHE_PAGES, PAGE_SIZE, parseIdentity, sameRequest, type Identity, type Reply, type Transport } from '../../shared/contracts';

const reply = (identity: Identity, name = 'file'): Reply => ({ identity, batch: { offset: 0, entries: [{ id: 'entry', name, kind: 'file', modifiedAtMs: null, sizeBytes: null }], total: 1, complete: true, capped: false, snapshotBytes: 256, capReason: null, revision: 0 } });
const settle = () => new Promise(resolve => setTimeout(resolve, 0));
describe('bounded listing state', () => {
  it('only captures file operation identity for the settled current tab and path', async () => {
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(), start: vi.fn(async identity => reply(identity)), read: vi.fn(), cancel: vi.fn(async () => {}) };
    const listing = new ListingController(0, api, () => {});
    expect(() => listing.operationIdentity('tab-1', '/one')).toThrow();
    listing.navigate('/one', 'tab-1'); await settle();
    expect(listing.operationIdentity('tab-1', '/one').tabId).toBe('tab-1');
    expect(() => listing.operationIdentity('tab-2', '/one')).toThrow();
    expect(() => listing.operationIdentity('tab-1', '/two')).toThrow();
    listing.state = { ...listing.state, viewPending: true };
    expect(() => listing.operationIdentity('tab-1', '/one')).toThrow();
    listing.dispose();
  });
  it('coalesces cancellation floods while retaining only the newest navigation', async () => {
    let finishStart: ((value: Reply) => void) | undefined;
    let firstIdentity: Identity | undefined;
    let finishCancel: (() => void) | undefined;
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(),
      start: vi.fn((identity, path) => path === '/first' ? new Promise<Reply>(resolve => { firstIdentity = identity; finishStart = resolve; }) : Promise.resolve(reply(identity, path))),
      read: vi.fn(),
      cancel: vi.fn(() => new Promise<void>(resolve => { finishCancel = resolve; })),
    };
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/first');
    for (let i = 0; i < 500; i++) listing.navigate(`/new-${i}`);
    expect(api.start).toHaveBeenCalledTimes(1);
    expect(api.cancel).toHaveBeenCalledTimes(1);
    if (!finishStart || !firstIdentity || !finishCancel) throw new Error('Missing pending operation');
    finishStart(reply(firstIdentity));
    await settle();
    expect(api.start).toHaveBeenCalledTimes(2);
    expect(listing.cache.row(0)?.name).toBe('/new-499');
    finishCancel();
    await settle();
    expect(api.cancel).toHaveBeenCalledTimes(2);
    finishCancel();
    await settle();
  });
  it('disposes without repainting or admitting new work after unmount', async () => {
    let finish: ((value: Reply) => void) | undefined;
    let request: Identity | undefined;
    const changed = vi.fn();
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(), start: vi.fn(identity => new Promise<Reply>(resolve => { request = identity; finish = resolve; })), read: vi.fn(), cancel: vi.fn(async () => {}) };
    const listing = new ListingController(0, api, changed);
    listing.navigate('/first');
    listing.dispose();
    const paints = changed.mock.calls.length;
    listing.navigate('/ignored'); listing.window(0, 100);
    if (!request || !finish) throw new Error('Missing pending listing');
    finish(reply(request)); await settle();
    expect(changed).toHaveBeenCalledTimes(paints);
    expect(api.start).toHaveBeenCalledTimes(1);
    expect(listing.cache.size).toBe(0);
  });
  it('surfaces a current cancellation failure without copying private rejection text', async () => {
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(), start: vi.fn(async identity => reply(identity)), read: vi.fn(), cancel: vi.fn(async () => { throw new Error('private-fixture-path'); }) };
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/first'); await settle();
    listing.stop(); await settle();
    expect(listing.state.status).toBe('error');
    expect(listing.state.error).toBe('ipc_error');
  });
  it('keeps display pages at 64 rows and checks partial-page completeness', () => {
    expect(PAGE_SIZE).toBe(64);
    const cache = new PageCache();
    cache.put({ offset: 0, entries: Array.from({ length: 256 }, (_, i) => syntheticRow(i)), total: 256, complete: false, capped: false, snapshotBytes: 25600, capReason: null, revision: 0 });
    expect(cache.row(63)?.id).toBe('synthetic-63');
    expect(cache.row(64)).toBeUndefined();
    cache.put({ offset: 64, entries: Array.from({ length: 10 }, (_, i) => syntheticRow(64 + i)), total: 74, complete: false, capped: false, snapshotBytes: 25600, capReason: null, revision: 0 });
    expect(cache.has(64, 10)).toBe(true);
    expect(cache.has(64, 64)).toBe(false);
  });
  it('fills four missing viewport pages and discards a late window after navigation', async () => {
    let finishWindow!: (value: Reply) => void;
    let pendingIdentity!: Identity;
    let defer = false;
    const page = (identity: Identity, offset: number): Reply => ({ identity, batch: { offset, entries: Array.from({ length: Math.min(64, 1001 - offset) }, (_, i) => syntheticRow(offset + i)), total: 1001, complete: true, capped: false, snapshotBytes: 256000, capReason: null, revision: 0 } });
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(),
      start: vi.fn(async identity => page(identity, 0)),
      read: vi.fn((identity, offset) => defer ? new Promise<Reply>(resolve => { finishWindow = resolve; pendingIdentity = identity; }) : Promise.resolve(page(identity, offset ?? 0))),
      cancel: vi.fn(async () => {}),
    };
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/A'); await settle();
    listing.window(64, 319); await settle();
    for (const row of [63, 64, 127, 128, 191, 192, 255, 256, 319]) expect(listing.cache.row(row)?.id).toBe(`synthetic-${row}`);
    expect(api.read).toHaveBeenCalledTimes(4);
    defer = true;
    listing.window(960, 1000);
    listing.navigate('/B');
    finishWindow(page(pendingIdentity, 960));
    await settle();
    expect(listing.cache.row(960)).toBeUndefined();
    expect(listing.cache.row(0)?.id).toBe('synthetic-0');
  });
  it('keeps at most eight pages and calculates synthetic rows on demand', () => {
    const cache = new PageCache();
    for (let i = 0; i < 40; i++) cache.put({ offset: i * PAGE_SIZE, entries: [syntheticRow(i)], total: 100_000, complete: false, capped: false, snapshotBytes: 8_100_000, capReason: null, revision: 0 });
    expect(cache.size).toBe(CACHE_PAGES);
    expect(cache.row(0)).toBeUndefined();
    expect(syntheticRow(99_999).id).toBe('synthetic-99999');
  });
  it('compares all identity fields, including tab and request', () => {
    const a = parseIdentity({ pane: 0, tabId: 'a', requestId: 'a', generation: 1 });
    expect(sameRequest(a, { ...a })).toBe(true);
    for (const b of [{ ...a, pane: 1 }, { ...a, tabId: 'b' }, { ...a, requestId: 'b' }, { ...a, generation: 2 }]) expect(sameRequest(a, parseIdentity(b))).toBe(false);
  });
  it('coalesces A→B→C and discards late A; idle tabs make no request', async () => {
    let finishA!: (value: Reply) => void;
    let idA!: Identity;
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(),
      start: vi.fn((identity: Identity, path: string) => path === '/A' ? new Promise<Reply>(resolve => { finishA = resolve; idA = identity; }) : Promise.resolve(reply(identity, path))),
      read: vi.fn(), cancel: vi.fn(async () => {}),
    };
    const listing = new ListingController(0, api, () => {});
    expect(api.start).not.toHaveBeenCalled();
    listing.navigate('/A'); listing.navigate('/B'); listing.navigate('/C');
    finishA(reply(idA, 'late-A'));
    await settle();
    expect(api.start).toHaveBeenCalledTimes(2);
    expect(listing.cache.row(0)?.name).toBe('/C');
    expect(listing.state.status).toBe('complete');
    listing.stop();
    expect(listing.cache.size).toBe(0);
  });
  it('does not resurrect a stopped tab after pending start resolves', async () => {
    let finish!: (value: Reply) => void;
    let id!: Identity;
    const api: Transport = { setView: vi.fn(), locate: vi.fn(), activate: vi.fn(), start: vi.fn((identity: Identity) => new Promise<Reply>(resolve => { id = identity; finish = resolve; })), read: vi.fn(), cancel: vi.fn(async () => {}) };
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/A'); listing.stop(); finish(reply(id)); await settle();
    expect(listing.state.status).toBe('idle');
    expect(listing.cache.size).toBe(0);
    expect(api.cancel).toHaveBeenCalledWith(id);
  });
});
