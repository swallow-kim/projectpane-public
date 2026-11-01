import { describe, expect, it, vi } from 'vitest';
import type { Activation, Identity, Reply, Transport } from '../../shared/contracts';
import { IpcFault } from '../../shared/fault';
import { DEFAULT_VIEW } from '../../shared/listing-view';
import { ListingController } from './listing';
import { syntheticRow } from './synthetic';

const settle = () => new Promise(resolve => setTimeout(resolve, 0));
function page(identity: Identity, revision = 0, offset = 0): Reply {
  return { identity, batch: { offset, revision, total: 130, complete: true, capped: false, snapshotBytes: 32_000, capReason: null,
    entries: Array.from({ length: Math.min(64, 130 - offset) }, (_, index) => syntheticRow(offset + index)) } };
}
function apiWith(overrides: Partial<Transport> = {}): Transport {
  return {
    start: vi.fn(async identity => page(identity)), read: vi.fn(async (identity, offset) => page(identity, 0, offset ?? 0)),
    cancel: vi.fn(async () => {}), setView: vi.fn(async identity => page(identity, 1)),
    locate: vi.fn(async identity => ({ identity, revision: 0, index: 0 })),
    activate: vi.fn(async (identity: Identity): Promise<Activation> => ({ identity, kind: 'file', path: null })), ...overrides,
  };
}

describe('view revisions and auxiliary listing operations', () => {
  it('rejects the final selection page when a view is requested before its revision settles', async () => {
    let finishPage: ((reply: Reply) => void) | undefined;
    let finishView: ((reply: Reply) => void) | undefined;
    const finalPage = new Promise<Reply>(resolve => { finishPage = resolve; });
    const viewReply = new Promise<Reply>(resolve => { finishView = resolve; });
    let current: Identity | undefined;
    const api = apiWith({
      read: vi.fn(async (identity, offset) => {
        current = identity;
        return offset === 128 ? finalPage : page(identity, 0, offset ?? 0);
      }),
      setView: vi.fn(() => viewReply),
    });
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/fixture'); await settle();
    const selected = listing.selectRange(0, 129);
    await settle();
    expect(api.read).toHaveBeenLastCalledWith(expect.objectContaining({ pane: 0 }), 128);
    const change = listing.setView({ ...DEFAULT_VIEW, filter: 'keep' });
    const rejected = expect(selected).rejects.toEqual(new IpcFault('stale_request'));
    if (!current || !finishPage || !finishView) throw new Error('Missing selection identity or response');
    finishPage(page(current, 0, 128));
    try { await rejected; }
    finally { finishView(page(current, 1)); await change; listing.dispose(); }
  });

  it('serializes view changes behind an in-flight window and invalidates old cached rows', async () => {
    let finish: ((reply: Reply) => void) | undefined;
    let request: Identity | undefined;
    const api = apiWith({ read: vi.fn(identity => new Promise<Reply>(resolve => { request = identity; finish = resolve; })) });
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/fixture', 'tab-one'); await settle();
    listing.window(64, 127);
    const change = listing.setView({ ...DEFAULT_VIEW, direction: 'desc' });
    expect(api.setView).not.toHaveBeenCalled();
    if (!finish || !request) throw new Error('missing in-flight read');
    finish(page(request, 0, 64)); await change;
    expect(listing.state.revision).toBe(1);
    expect(listing.cache.row(64)).toBeUndefined();
    expect(api.start).toHaveBeenCalledWith(expect.objectContaining({ tabId: 'tab-one' }), '/fixture', DEFAULT_VIEW);
  });

  it('rejects queued old-tab activation before it can reach the native API', async () => {
    let finish: ((reply: Reply) => void) | undefined;
    let request: Identity | undefined;
    const api = apiWith({ start: vi.fn(identity => new Promise<Reply>(resolve => { request = identity; finish = resolve; })) });
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/A', 'tab-a');
    const activated = listing.activate('entry-a');
    const rejected = expect(activated).rejects.toEqual(new IpcFault('cancelled'));
    listing.stop();
    if (!finish || !request) throw new Error('missing in-flight start');
    finish(page(request)); await rejected; await settle();
    expect(api.activate).not.toHaveBeenCalled();
    expect(listing.cache.size).toBe(0);
  });

  it('stops large selection reads after cancellation and rejects changed view revisions', async () => {
    const controller = new AbortController();
    const api = apiWith({ read: vi.fn(async (identity, offset) => { controller.abort(); return page(identity, 0, offset ?? 0); }) });
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/fixture'); await settle();
    await expect(listing.selectRange(0, 129, controller.signal)).rejects.toEqual(new IpcFault('cancelled'));
    expect(api.read).toHaveBeenCalledTimes(1);
    const newer = apiWith({ read: vi.fn(async (identity, offset) => page(identity, 1, offset ?? 0)) });
    const other = new ListingController(1, newer, () => {});
    other.navigate('/fixture'); await settle();
    await expect(other.selectRange(0, 129)).rejects.toEqual(new IpcFault('stale_request'));
  });

  it('retains only the newest pending view without letting a stopped controller apply it', async () => {
    let finish: ((reply: Reply) => void) | undefined;
    let request: Identity | undefined;
    const api = apiWith({ start: vi.fn(identity => new Promise<Reply>(resolve => { request = identity; finish = resolve; })) });
    const listing = new ListingController(0, api, () => {});
    listing.navigate('/fixture');
    const first = listing.setView({ ...DEFAULT_VIEW, filter: 'old' });
    const cancelled = expect(first).rejects.toEqual(new IpcFault('cancelled'));
    const last = listing.setView({ ...DEFAULT_VIEW, filter: 'new' });
    if (!finish || !request) throw new Error('missing pending start');
    finish(page(request)); await cancelled; await last;
    expect(api.setView).toHaveBeenCalledTimes(1);
    expect(api.setView).toHaveBeenCalledWith(request, { ...DEFAULT_VIEW, filter: 'new' });
  });
});
