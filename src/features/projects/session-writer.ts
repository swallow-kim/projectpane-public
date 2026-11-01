export type SaveSession<T> = (revision: number, workspace: T) => Promise<number>;
export type SaveStatus = 'dirty' | 'saving' | 'saved' | 'error';

/** Owns one project's mutable write queue and revision for its entire lifetime. */
export class SessionWriter<T> {
  private pending: { readonly value: T } | null = null;
  private running: Promise<void> | null = null;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private disposed = false;
  private revision: number;

  constructor(private readonly save: SaveSession<T>, revision: number,
    private readonly status: (status: SaveStatus, error?: unknown) => void = () => {}) {
    this.revision = revision;
  }
  get savedRevision(): number { return this.revision; }
  changed(workspace: T): void {
    if (this.disposed) return;
    this.pending = { value: workspace };
    this.status('dirty');
    clearTimeout(this.timer);
    this.timer = setTimeout(() => { void this.flush().catch((error: unknown) => this.status('error', error)); }, 750);
  }
  flush(): Promise<void> {
    clearTimeout(this.timer);
    if (this.running) return this.running;
    this.running = this.drain().finally(() => { this.running = null; });
    return this.running;
  }
  private async drain(): Promise<void> {
    while (this.pending && !this.disposed) {
      const next = this.pending;
      this.pending = null;
      this.status('saving');
      try { this.revision = await this.save(this.revision, next.value); }
      catch (error) {
        if (!this.disposed) { this.pending ??= next; this.status('error', error); }
        throw error;
      }
    }
    if (!this.disposed) this.status('saved');
  }
  async flushWithin(milliseconds: number): Promise<boolean> {
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      return await Promise.race([
        this.flush().then(() => true, () => false),
        new Promise<boolean>(resolve => { timeout = setTimeout(() => resolve(false), milliseconds); }),
      ]);
    } finally { clearTimeout(timeout); }
  }
  dispose(): void { this.disposed = true; this.pending = null; clearTimeout(this.timer); }
}
