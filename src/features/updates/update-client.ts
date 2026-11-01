import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';

export const updateStatusSchema = z.object({
  currentVersion: z.string(), installed: z.boolean(),
  phase: z.enum(['idle', 'checking', 'current', 'available', 'downloading', 'ready', 'preparing', 'installing', 'error']),
  version: z.string().nullable(), notes: z.string().nullable(), error: z.string().nullable(),
  received: z.number().nonnegative(), total: z.number().nonnegative().nullable(),
  lastCheck: z.number().nonnegative().nullable(), previousInstall: z.enum(['completed', 'unconfirmed']).nullable(),
});
export type UpdateStatus = z.infer<typeof updateStatusSchema>;
export type UpdateCommand = 'update_status' | 'check_update' | 'download_update' | 'install_update';
export const updateClient = async (command: UpdateCommand, args?: Record<string, unknown>) =>
  updateStatusSchema.parse(await invoke(command, args));

// One admission lane for UI requests; backend independently enforces the same rule.
export class UpdateController {
  status: UpdateStatus | null = null;
  busy = false;
  error: string | null = null;
  private disposed = false;
  private revision = 0;
  private refreshing = false;
  constructor(private readonly request = updateClient, private readonly changed: () => void = () => {}) {}
  async run(command: UpdateCommand, args?: Record<string, unknown>) {
    if (this.busy || this.disposed) return;
    this.busy = true; this.error = null;
    const revision = ++this.revision; this.changed();
    try {
      const result = await this.request(command, args);
      if (!this.disposed && revision === this.revision) this.status = result;
    } catch { if (!this.disposed && revision === this.revision) this.error = 'request_failed'; }
    finally { if (!this.disposed && revision === this.revision) { this.revision += 1; this.busy = false; this.changed(); } }
  }
  async refresh() {
    if (this.disposed || this.refreshing) return;
    this.refreshing = true;
    const revision = this.revision;
    try { const result = await this.request('update_status');
      if (!this.disposed && revision === this.revision) { this.status = result; this.changed(); }
    } catch { /* An in-flight request reports its own error. Never overwrite it with polling. */ }
    finally { this.refreshing = false; }
  }
  dispose() { this.disposed = true; this.revision += 1; }
}
