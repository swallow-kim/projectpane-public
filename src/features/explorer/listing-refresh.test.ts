import { describe, expect, it, vi } from 'vitest';
import { ListingController } from './listing';
import { PAGE_SIZE, type Identity, type Reply, type Transport } from '../../shared/contracts';
import { DEFAULT_VIEW } from '../../shared/listing-view';

const tick = () => new Promise(resolve => setTimeout(resolve, 0));
const page = (identity: Identity, offset = 0, total = 640, revision = 0, complete = true, prefix = 'old'): Reply => ({ identity,
  batch: { offset, total, revision, complete, capped: false, capReason: null, snapshotBytes: total * 100,
    entries: Array.from({ length: Math.min(PAGE_SIZE, Math.max(0, total - offset)) }, (_, index) => ({ id: `${prefix}-${offset + index}`, name: `${prefix}-${offset + index}.txt`, kind: 'file', sizeBytes: 12, modifiedAtMs: null })) } });
function fixture() {
  const api: Transport = { start: vi.fn(async identity => page(identity)), read: vi.fn(async (identity, offset) => page(identity, offset ?? 0)),
    cancel: vi.fn(async () => {}), setView: vi.fn(), locate: vi.fn(), activate: vi.fn() };
  const changed = vi.fn();
  return { api, changed, listing: new ListingController(0, api, changed) };
}

describe('atomic refresh of a displayed directory', () => {
  it('keeps visible pages and total until replacement viewport pages are ready; blocks stale actions', async () => {
    const { listing, api } = fixture();
    listing.navigate('/one', 'tab'); await tick(); listing.window(256, 330); await tick();
    let finish!: (reply: Reply) => void, identity!: Identity;
    vi.mocked(api.start).mockImplementation(id => { identity = id; return new Promise(resolve => { finish = resolve; }); });
    vi.mocked(api.read).mockImplementation(async (id, offset) => page(id, offset ?? 0, 640, 2, true, 'new'));
    listing.refresh('/one', 'tab');
    expect(listing.state).toMatchObject({ status: 'loading', refreshing: true, total: 640 });
    expect(listing.cache.row(256)?.id).toBe('old-256');
    expect(() => listing.operationIdentity('tab', '/one')).toThrow();
    await expect(listing.entryAt(256)).rejects.toThrow();
    await expect(listing.activate('old-256')).rejects.toThrow();
    await expect(listing.setView(DEFAULT_VIEW)).rejects.toThrow();
    finish(page(identity, 0, 640, 2, true, 'new')); await tick();
    expect(listing.state).toMatchObject({ status: 'complete', total: 640, revision: 2 });
    expect(listing.state.refreshing).toBeUndefined();
    expect(listing.cache.row(256)?.id).toBe('new-256');
    expect(listing.cache.row(330)?.id).toBe('new-330');
    expect(listing.cache.size).toBeLessThanOrEqual(8);
  });
  it('waits for enumeration completion without publishing partial totals', async () => {
    const { listing, api, changed } = fixture();
    listing.navigate('/one', 'tab'); await tick();
    let complete!: (reply: Reply) => void, id!: Identity;
    vi.mocked(api.start).mockImplementation(async identity => { id = identity; return page(identity, 0, 64, 0, false, 'new'); });
    vi.mocked(api.read).mockImplementation(() => new Promise(resolve => { complete = resolve; }));
    listing.refresh('/one', 'tab'); await tick();
    const renders = changed.mock.calls.length;
    expect(listing.state.total).toBe(640); expect(listing.cache.row(0)?.id).toBe('old-0');
    complete({ ...page(id, 0, 641), batch: { ...page(id, 0, 641).batch, entries: [] } }); await tick();
    expect(listing.state).toMatchObject({ total: 641, status: 'complete' });
    expect(listing.cache.row(0)?.id).toBe('new-0');
    expect(changed.mock.calls.length).toBe(renders + 1);
  });
  it('coalesces repeated refresh requests, but navigation discards a delayed replacement', async () => {
    const { listing, api } = fixture();
    listing.navigate('/one', 'tab'); await tick();
    let finish!: (reply: Reply) => void, id!: Identity;
    vi.mocked(api.start).mockImplementationOnce(identity => { id = identity; return new Promise(resolve => { finish = resolve; }); });
    listing.refresh('/one', 'tab');
    for (let index = 0; index < 20; index++) listing.refresh('/one', 'tab');
    expect(api.start).toHaveBeenCalledTimes(2);
    listing.navigate('/two', 'other');
    expect(listing.cache.size).toBe(0); expect(listing.state.total).toBe(0);
    finish(page(id, 0, 641, 1, true, 'obsolete')); await tick();
    expect(listing.cache.row(0)?.id).toBe('old-0'); expect(listing.state.total).toBe(640);
    expect(listing.operationIdentity('other', '/two').tabId).toBe('other');
  });
  it('publishes a genuinely empty directory and clears retained pages', async () => {
    const { listing, api } = fixture(); listing.navigate('/one', 'tab'); await tick();
    vi.mocked(api.start).mockImplementation(async id => page(id, 0, 0));
    listing.refresh('/one', 'tab'); await tick();
    expect(listing.state).toMatchObject({ status: 'complete', total: 0 }); expect(listing.cache.size).toBe(0);
  });
  it('shrinking directories fetch the final viewport and lower revisions of the new request are valid', async () => {
    const { listing, api } = fixture(); listing.navigate('/one', 'tab'); await tick();
    listing.state = { ...listing.state, revision: 9 };
    listing.window(512, 550); await tick();
    vi.mocked(api.start).mockImplementation(async id => page(id, 0, 80, 0, true, 'small'));
    vi.mocked(api.read).mockImplementation(async (id, offset) => page(id, offset ?? 0, 80, 0, true, 'small'));
    listing.refresh('/one', 'tab'); await tick();
    expect(listing.state).toMatchObject({ total: 80, revision: 0, status: 'complete' });
    expect(listing.cache.row(79)?.id).toBe('small-79');
  });
  it('failure is explicit and stop releases both generations without late repaint', async () => {
    const { listing, api, changed } = fixture(); listing.navigate('/one', 'tab'); await tick();
    vi.mocked(api.start).mockRejectedValueOnce({ code: 'not_found' });
    listing.refresh('/one', 'tab'); await tick();
    expect(listing.state.status).toBe('error'); expect(listing.cache.size).toBe(0);
    listing.navigate('/one', 'tab'); await tick();
    let finish!: (reply: Reply) => void, id!: Identity;
    vi.mocked(api.start).mockImplementationOnce(identity => { id = identity; return new Promise(resolve => { finish = resolve; }); });
    listing.refresh('/one', 'tab'); listing.dispose(); const paints = changed.mock.calls.length;
    finish(page(id)); await tick(); expect(changed.mock.calls.length).toBe(paints); expect(listing.cache.size).toBe(0);
  });
  it('fills the whole clamped viewport after a large directory shrink', async () => {
    const { listing, api } = fixture();
    vi.mocked(api.start).mockImplementation(async id => page(id, 0, 1000));
    vi.mocked(api.read).mockImplementation(async (id, offset) => page(id, offset ?? 0, 1000));
    listing.navigate('/one', 'tab'); await tick(); listing.window(768, 830); await tick();
    vi.mocked(api.start).mockImplementation(async id => page(id, 0, 280, 0, true, 'small'));
    vi.mocked(api.read).mockImplementation(async (id, offset) => page(id, offset ?? 0, 280, 0, true, 'small'));
    listing.refresh('/one', 'tab'); await tick();
    expect(listing.state).toMatchObject({ total: 280, status: 'complete' });
    for (let index = 217; index < 280; index++) expect(listing.cache.row(index)?.id).toBe(`small-${index}`);
    expect(listing.cache.size).toBeLessThanOrEqual(8);
  });
});
