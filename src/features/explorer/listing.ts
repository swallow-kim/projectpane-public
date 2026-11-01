import { CACHE_PAGES, PAGE_SIZE, parseIdentity, sameRequest, type Entry, type Identity, type Reply, type Transport } from '../../shared/contracts';
import { IpcFault, toFaultCode, type FaultCode } from '../../shared/fault';
import { DEFAULT_VIEW, type ListingView } from '../../shared/listing-view';
import { PageCache } from './page-cache';
import { ListingQueue } from './listing-queue';

export type ListingState = Readonly<{ total: number; revision: number; refreshing?: boolean; viewPending?: boolean; firstBatchMs?: number; elapsedMs?: number } & (
  { status: 'idle' | 'loading' | 'complete' | 'capped'; error?: never } | { status: 'error'; error: FaultCode }
)>;

// One runner per pane. Navigation is coalesced to the latest request, and window
// reads take priority over enumeration. Inactive tabs never start a worker.
export class ListingController {
  private displayCache = new PageCache();
  get cache() { return this.displayCache; }
  // Keep at most eight displayed and eight replacement pages while revalidating.
  // Old entries are presentation only; actions are blocked until the swap.
  private replacement?: { cache: PageCache; reply?: Reply };
  private viewport = { first: 0, last: 0 };
  state: ListingState = { total: 0, revision: 0, status: 'idle' };
  private current?: Identity;
  get requestId() { return this.current?.requestId; }
  private currentPath = '';
  private desired?: { identity: Identity; path: string; view: ListingView };
  private running = false;
  private windows: number[] = [];
  private generation = Date.now();
  private started = 0;
  private disposed = false;
  private latestCancellation?: Identity;
  private cancelling = false;
  private pendingCancellation?: Identity;
  private viewEpoch = 0;
  private readonly actions = new ListingQueue(() => this.current, () => { void this.run(); });
  constructor(private pane: number, private api: Transport, private changed: () => void) {}

  navigate(path: string, tabId = `pane-${this.pane}-real`, view: ListingView = DEFAULT_VIEW) {
    this.begin(path, tabId, view, false);
  }

  refresh(path: string, tabId: string, view: ListingView = DEFAULT_VIEW) {
    if (this.replacement && this.currentPath === path && this.current?.tabId === tabId) return;
    const retain = this.currentPath === path && this.current?.tabId === tabId && !this.state.viewPending
      && ['complete', 'capped'].includes(this.state.status);
    this.begin(path, tabId, view, retain);
  }

  private begin(path: string, tabId: string, view: ListingView, retain: boolean) {
    if (this.disposed) return;
    const identity = parseIdentity({ pane: this.pane, tabId, requestId: crypto.randomUUID(), generation: ++this.generation });
    const previous = this.current;
    this.current = identity;
    this.currentPath = path;
    this.desired = { identity, path, view };
    this.actions.clear();
    this.viewEpoch++;
    this.windows = [];
    this.replacement = retain ? { cache: new PageCache() } : undefined;
    if (!retain) { this.cache.clear(); this.viewport = { first: 0, last: 0 }; }
    this.state = retain ? { ...this.state, error: undefined, status: 'loading', refreshing: true }
      : { total: 0, revision: 0, status: 'loading' };
    this.changed();
    if (previous) this.cancelDetached(previous);
    void this.run();
  }

  stop() {
    const previous = this.current;
    this.current = undefined;
    this.currentPath = '';
    this.desired = undefined;
    this.actions.clear();
    this.viewEpoch++;
    this.windows = [];
    this.replacement = undefined;
    this.cache.clear();
    this.state = { total: 0, revision: 0, status: 'idle' };
    if (previous) this.cancelDetached(previous);
    if (!this.disposed) this.changed();
  }

  dispose() { this.disposed = true; this.stop(); }

  private identity(): Identity {
    if (!this.current) throw new IpcFault('stale_request');
    return this.current;
  }
  operationIdentity(tabId: string, path: string): Identity {
    if (this.state.viewPending || !['complete', 'capped'].includes(this.state.status)) throw new IpcFault('busy');
    if (this.current?.tabId !== tabId || this.currentPath !== path) throw new IpcFault('stale_request');
    return this.identity();
  }
  async setView(view: ListingView): Promise<void> {
    if (this.replacement) throw new IpcFault('busy');
    const identity = this.identity(), epoch = ++this.viewEpoch;
    this.state = { ...this.state, viewPending: true };
    this.changed();
    try {
      await this.actions.enqueue(identity, async () => {
        const reply = await this.api.setView(identity, view);
        if (epoch === this.viewEpoch) this.accept(reply, true, true);
      }, 'view');
    } catch (error) {
      if (epoch === this.viewEpoch && this.current && sameRequest(this.current, identity)) {
        this.state = { ...this.state, viewPending: false };
        this.changed();
      }
      throw error;
    }
  }
  async locate(entryId: string): Promise<number | null> {
    if (this.replacement) throw new IpcFault('busy');
    const identity = this.identity(), revision = this.state.revision;
    return this.actions.enqueue(identity, async () => {
      const result = await this.api.locate(identity, entryId);
      if (result.revision !== revision || this.state.revision !== revision) throw new IpcFault('stale_request');
      return result.index;
    });
  }
  async activate(entryId: string) {
    if (this.state.viewPending || this.replacement) throw new IpcFault('busy');
    const identity = this.identity();
    return this.actions.enqueue(identity, () => this.api.activate(identity, entryId), 'activate');
  }
  cachedEntryAt(index: number): Entry | undefined {
    if (this.state.viewPending || this.replacement) throw new IpcFault('busy');
    return this.cache.row(index);
  }
  async entryAt(index: number, latestOnly = false): Promise<Entry | undefined> {
    const cached = this.cachedEntryAt(index);
    if (cached) return cached;
    const identity = this.identity(), revision = this.state.revision, epoch = this.viewEpoch;
    return this.actions.enqueue(identity, async () => {
      const offset = Math.floor(index / PAGE_SIZE) * PAGE_SIZE;
      const reply = await this.api.read(identity, offset);
      if (epoch !== this.viewEpoch) throw new IpcFault('stale_request');
      if (reply.batch.revision !== revision) { this.accept(reply, true); throw new IpcFault('stale_request'); }
      if (!this.accept(reply, true)) throw new IpcFault('stale_request');
      return reply.batch.entries[index - offset];
    }, latestOnly ? 'selection-entry' : undefined);
  }
  async selectRange(first: number, last: number, signal?: AbortSignal): Promise<ReadonlySet<string>> {
    const identity = this.identity(), revision = this.state.revision, epoch = this.viewEpoch;
    if (this.state.status === 'loading' || this.state.viewPending) throw new IpcFault('busy');
    const start = Math.max(0, Math.min(first, last)), end = Math.min(this.state.total - 1, Math.max(first, last));
    const selected = new Set<string>();
    let bytes = 0;
    for (let offset = Math.floor(start / PAGE_SIZE) * PAGE_SIZE; offset <= end; offset += PAGE_SIZE) {
      if (signal?.aborted) throw new IpcFault('cancelled');
      const reply = await this.actions.enqueue(identity, () => this.api.read(identity, offset));
      if (signal?.aborted) throw new IpcFault('cancelled');
      if (epoch !== this.viewEpoch || reply.batch.revision !== revision || this.state.revision !== revision) throw new IpcFault('stale_request');
      for (const [index, entry] of reply.batch.entries.entries()) {
        if (offset + index < start || offset + index > end) continue;
        bytes += entry.id.length * 2;
        if (bytes > 16 * 1024 * 1024) throw new IpcFault('resource_limit');
        selected.add(entry.id);
      }
    }
    return selected;
  }

  private cancelDetached(identity: Identity) {
    this.latestCancellation = identity;
    this.pendingCancellation = identity;
    void this.drainCancellations();
  }

  private async drainCancellations() {
    if (this.cancelling) return;
    this.cancelling = true;
    try {
      while (this.pendingCancellation) {
        const identity = this.pendingCancellation;
        this.pendingCancellation = undefined;
        try { await this.api.cancel(identity); }
        catch (error) {
          if (this.disposed || this.current || this.latestCancellation !== identity) continue;
          const code = toFaultCode(error);
          if (code === 'cancelled' || code === 'stale_request') continue;
          this.state = { total: 0, revision: 0, status: 'error', error: code };
          this.changed();
        }
      }
    } finally { this.cancelling = false; }
  }

  window(first: number, last: number) {
    if (this.disposed) return;
    this.viewport = { first, last };
    if (this.replacement) { this.prepareReplacement(); void this.run(); return; }
    // Coalesce scroll bursts: keep only the current viewport plus overscan.
    this.windows = [];
    for (let offset = Math.floor(first / PAGE_SIZE) * PAGE_SIZE; offset <= last && offset < this.state.total; offset += PAGE_SIZE) {
      if (!this.cache.has(offset, Math.min(PAGE_SIZE, this.state.total - offset))) this.windows.push(offset);
      if (this.windows.length === CACHE_PAGES) break;
    }
    void this.run();
  }

  private accept(reply: Reply, cache: boolean, settledView = false) {
    if (!this.current || !sameRequest(this.current, reply.identity)) return false;
    if (this.replacement) {
      const previous = this.replacement.reply;
      if (previous && reply.batch.revision < previous.batch.revision) return false;
      if (previous && reply.batch.revision !== previous.batch.revision) this.replacement.cache.clear();
      if (cache) this.replacement.cache.put(reply.batch);
      this.replacement.reply = reply;
      this.prepareReplacement();
      return true;
    }
    if (reply.batch.revision < this.state.revision) return false;
    if (reply.batch.revision !== this.state.revision) { this.cache.clear(); this.windows = []; }
    if (cache) this.cache.put(reply.batch);
    this.state = {
      total: reply.batch.total,
      revision: reply.batch.revision,
      viewPending: settledView ? false : this.state.viewPending ?? false,
      status: reply.batch.capped ? 'capped' : reply.batch.complete ? 'complete' : 'loading',
      firstBatchMs: this.state.firstBatchMs ?? performance.now() - this.started,
      elapsedMs: performance.now() - this.started,
    };
    this.changed();
    return true;
  }

  private prepareReplacement() {
    const pending = this.replacement, reply = pending?.reply;
    if (!pending || !reply || !reply.batch.complete) return;
    const total = reply.batch.total;
    // Shrinking the scroll extent clamps the whole viewport, not just its last
    // row. Prepare every page that will become visible before swapping caches.
    const span = Math.max(1, this.viewport.last - this.viewport.first + 1);
    const first = Math.max(0, Math.min(this.viewport.first, total - span));
    const last = Math.min(first + span - 1, total - 1);
    this.windows = [];
    for (let offset = Math.floor(first / PAGE_SIZE) * PAGE_SIZE; offset <= last; offset += PAGE_SIZE) {
      if (!pending.cache.has(offset, Math.min(PAGE_SIZE, total - offset))) this.windows.push(offset);
      if (this.windows.length === CACHE_PAGES) break;
    }
    if (this.windows.length) return;
    this.displayCache = pending.cache;
    this.replacement = undefined;
    // Revisions belong to a request, so reset the old request's revision too.
    this.state = { total: 0, revision: reply.batch.revision, status: 'loading' };
    this.accept(reply, false);
  }

  private async run() {
    if (this.running) return;
    this.running = true;
    try {
      while (this.current) {
        const identity = this.current;
        const action = this.desired ? undefined : this.actions.take();
        try {
          if (this.desired) {
            const next = this.desired;
            this.desired = undefined;
            this.started = performance.now();
            const reply = await this.api.start(next.identity, next.path, next.view);
            if (!this.accept(reply, true)) this.cancelDetached(next.identity);
          } else if (action) {
            await action.run();
          } else if (this.windows.length) {
            const offset = this.windows.shift();
            if (offset !== undefined) this.accept(await this.api.read(identity, offset), true);
          } else if (this.state.status === 'loading') {
            this.accept(await this.api.read(identity, null), false);
          } else break;
        } catch (error) {
          if (this.current && sameRequest(this.current, identity)) {
            if (this.replacement) { this.cache.clear(); this.state = { total: 0, revision: 0, status: 'loading' }; }
            this.replacement = undefined;
            this.actions.clear();
            this.windows = [];
            this.state = { ...this.state, refreshing: false, status: 'error', error: toFaultCode(error) };
            this.changed();
            break;
          }
        }
      }
    } finally { this.running = false; }
  }

}
