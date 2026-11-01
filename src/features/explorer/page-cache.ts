import { CACHE_PAGES, PAGE_SIZE, type Batch, type Entry } from '../../shared/contracts';

export class PageCache {
  private pages = new Map<number, readonly Entry[]>();
  clear() { this.pages.clear(); }
  get size() { return this.pages.size; }
  has(offset: number, length = PAGE_SIZE) { return (this.pages.get(offset)?.length ?? 0) >= length; }
  put(batch: Batch) {
    if (batch.entries.length === 0) return;
    this.pages.delete(batch.offset);
    this.pages.set(batch.offset, batch.entries.length > PAGE_SIZE ? batch.entries.slice(0, PAGE_SIZE) : batch.entries);
    while (this.pages.size > CACHE_PAGES) {
      const oldest = this.pages.keys().next();
      if (oldest.done) break;
      this.pages.delete(oldest.value);
    }
  }
  row(index: number) { return this.pages.get(Math.floor(index / PAGE_SIZE) * PAGE_SIZE)?.[index % PAGE_SIZE]; }
}
