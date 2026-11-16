import type { Workspace } from '../../shared/projects';
import type { LoadedProject, ProjectClient, ProjectMetadata } from './project-client';
import { SessionWriter, type SaveStatus } from './session-writer';
const reorderError = '프로젝트 순서 저장을 확인하지 못했습니다. 다시 시도하여 저장된 순서를 확인하세요.';
const sessionSaveError = '세션을 저장하지 못했습니다. 다시 시도하세요.';
export type ProjectState = {
  readonly available: boolean;
  readonly readOnly: boolean;
  readonly ready: boolean;
  readonly busy: boolean;
  readonly selected: ProjectMetadata | null;
  readonly projects: readonly ProjectMetadata[];
  readonly status: SaveStatus;
  readonly error: string | null;
  readonly warnings: readonly string[];
};
export class ProjectWorkspaceController {
  state: ProjectState = { available: false, readOnly: false, ready: false, busy: false, selected: null, projects: [], status: 'saved', error: null, warnings: [] };
  private reordering: Promise<void> | null = null;
  private retryCatalog = false;
  private writer: SessionWriter<Workspace> | null = null;
  private generation = 0;
  private disposed = false;
  private fingerprint = '';
  private workspace: Workspace | null = null;
  private ephemeral = false;
  private retryLoad: { readonly id: string; readonly source: 'session' | 'preset' } | null = null;
  constructor(readonly client: ProjectClient, readonly apply: (loaded: LoadedProject | null) => void, readonly notify: () => void) {}
  private update(patch: Partial<ProjectState>): void {
    if (this.disposed) return;
    this.state = { ...this.state, ...patch, ...(this.retryCatalog && patch.error === null ? { error: reorderError } : {}) }; this.notify();
  }
  private accept(loaded: LoadedProject, restore = true): void {
    this.writer?.dispose();
    const project = loaded.project;
    this.workspace = loaded.workspace;
    this.fingerprint = JSON.stringify(loaded.workspace);
    this.writer = loaded.readOnly ? null : new SessionWriter((revision, workspace) => this.client.saveSession(project, revision, workspace), loaded.sessionRevision,
      (status) => {
        if (this.state.selected !== project) return;
        const error = status === 'error' ? sessionSaveError
          : status === 'saved' && this.state.error === sessionSaveError ? null : this.state.error;
        this.update({ status, error });
      });
    this.update({ selected: project, readOnly: loaded.readOnly, status: 'saved', warnings: loaded.warnings });
    if (restore) this.apply(loaded);
  }
  private async refreshList(): Promise<void> {
    const list = await this.client.list();
    this.update({ available: true, projects: list.projects, warnings: list.warnings });
  }
  async initialize(ephemeral: boolean): Promise<void> {
    this.ephemeral = ephemeral;
    const ticket = ++this.generation;
    this.update({ busy: true, error: null });
    try {
      const list = await this.client.list();
      if (ticket !== this.generation || this.disposed) return;
      this.update({ available: true, projects: list.projects, warnings: list.warnings });
      if (!ephemeral && list.lastProjectId) {
        const loaded = await this.client.load(list.lastProjectId, 'session');
        if (ticket !== this.generation || this.disposed) return;
        this.accept(loaded);
      }
    } catch (error) { this.failure(ticket, error); }
    finally { if (ticket === this.generation) this.update({ ready: true, busy: false }); }
  }
  private failure(ticket: number, error: unknown): void {
    if (ticket !== this.generation) return;
    const code = error instanceof Error ? error.message : String(error);
    this.update({ error: code.includes('revision_conflict') ? '다른 저장과 충돌했습니다. 현재 작업을 유지했습니다. 다시 불러오기 전에 변경을 확인하세요.' : '프로젝트 작업에 실패했습니다. 현재 작업을 유지했습니다. 다시 시도하세요.' });
  }
  async select(id: string, source: 'session' | 'preset' = 'session'): Promise<void> {
    this.retryLoad = null;
    let loading = false;
    const ticket = ++this.generation;
    this.update({ busy: true, error: null });
    try {
      await this.writer?.flush();
      if (ticket !== this.generation || this.disposed) return;
      loading = true;
      const loaded = await this.client.load(id, source);
      if (ticket !== this.generation || this.disposed) return;
      this.accept(loaded);
      if (source === 'preset') { this.fingerprint = ''; this.changed(loaded.workspace); }
    } catch (error) {
      if (loading && ticket === this.generation) this.retryLoad = { id, source };
      this.failure(ticket, error);
    }
    finally { if (ticket === this.generation) this.update({ busy: false }); }
  }
  changed(workspace: Workspace): void {
    if (!this.writer || this.disposed) return;
    const fingerprint = JSON.stringify(workspace);
    if (fingerprint === this.fingerprint) return;
    this.workspace = workspace; this.fingerprint = fingerprint; this.writer.changed(workspace);
  }
  async mutate(action: (project: ProjectMetadata | null) => Promise<LoadedProject | ProjectMetadata | null>, clear = false): Promise<void> {
    if (this.state.busy) return;
    const ticket = ++this.generation;
    this.update({ busy: true, error: null });
    try {
      await this.writer?.flush();
      if (ticket !== this.generation || this.disposed) return;
      const result = await action(this.state.selected);
      if (ticket !== this.generation || this.disposed) return;
      if (result && 'workspace' in result) this.accept(result);
      else if (result) {
        const workspace = this.workspace;
        if (workspace) {
          const revision = this.writer?.savedRevision ?? 0;
          this.accept({ project: result, workspace, sessionRevision: revision, readOnly: false, warnings: [] }, false);
          this.fingerprint = '';
          this.changed(workspace);
          await this.writer?.flush();
        }
      } else if (clear) {
        this.writer?.dispose(); this.writer = null; this.fingerprint = '';
        this.update({ selected: null, readOnly: false, status: 'saved' }); this.apply(null);
      }
      await this.refreshList();
    } catch (error) { this.failure(ticket, error); }
    finally { if (ticket === this.generation) this.update({ busy: false }); }
  }
  reorder(ids: readonly string[]): Promise<void> {
    if (this.disposed || this.state.busy || !this.state.ready || this.state.readOnly) return Promise.resolve();
    const expected: readonly string[] = this.state.projects.map(project => project.id);
    const next = [...ids];
    if (next.length !== expected.length || new Set(next).size !== next.length || next.some(id => !expected.includes(id))) {
      this.update({ error: reorderError }); return Promise.resolve();
    }
    if (next.every((id, index) => id === expected[index])) return Promise.resolve();
    const ticket = ++this.generation;
    this.update({ busy: true, error: null });
    const work = (async () => {
      try {
        await this.writer?.flush();
        if (ticket !== this.generation || this.disposed) return;
        const saved = await this.client.reorder(expected, next);
        if (ticket !== this.generation || this.disposed) return;
        if (saved.length !== next.length || saved.some((id, index) => id !== next[index])) throw new Error('invalid_document');
        const byId = new Map<string, ProjectMetadata>(this.state.projects.map(project => [project.id, project]));
        this.retryCatalog = false;
        this.update({ projects: saved.map(id => byId.get(id)!), error: null });
      } catch {
        if (ticket === this.generation && !this.disposed) {
          // A lost acknowledgement may follow a successful write. Retry reads back
          // the saved catalog; it never blindly sends the move a second time.
          this.retryCatalog = true; this.update({ error: reorderError });
        }
      } finally { if (ticket === this.generation) this.update({ busy: false }); }
    })();
    this.reordering = work;
    void work.finally(() => { if (this.reordering === work) this.reordering = null; });
    return work;
  }
  async retry(): Promise<void> {
    if (this.state.busy) return;
    if (this.retryCatalog) {
      this.update({ busy: true, error: null });
      try { await this.refreshList(); this.retryCatalog = false; this.update({ error: null }); }
      catch { this.update({ error: reorderError }); }
      finally { this.update({ busy: false }); }
      return;
    }
    if (this.retryLoad) return this.select(this.retryLoad.id, this.retryLoad.source);
    if (!this.writer) return this.initialize(this.ephemeral);
    this.update({ error: null });
    try { await this.writer.flush(); } catch (error) { this.failure(this.generation, error); }
  }
  async closeWithin(milliseconds: number): Promise<boolean> {
    const started = performance.now();
    if (this.reordering) {
      let timer: ReturnType<typeof setTimeout> | undefined;
      const completed = await Promise.race([this.reordering.then(() => true), new Promise<boolean>(resolve => { timer = setTimeout(() => resolve(false), milliseconds); })]);
      clearTimeout(timer);
      if (!completed) return false;
    }
    if (this.retryCatalog) return false;
    return this.writer ? this.writer.flushWithin(Math.max(1, milliseconds - (performance.now() - started))) : true;
  }
  dispose(): void { this.disposed = true; ++this.generation; this.writer?.dispose(); }
}
