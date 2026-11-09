import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { identitySchema } from '../../shared/contracts';
import { entryIdSchema } from '../../shared/listing-view';
import { IpcFault } from '../../shared/fault';
import { operationCountsSchema, type OperationSelection } from '../../shared/operations';
import { normalizeAbsolutePath, normalizeRelativePath } from '../../shared/projects-path';
import { normalizeExplorerPath } from './native-drop';

const token = z.string().regex(/^[A-Za-z0-9-]{1,80}$/);
const count = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const captureSchema = z.strictObject({ id: token, count, sealed: z.boolean() });
export type CopyCapture = z.infer<typeof captureSchema>;
export const copyJobSchema = z.strictObject({ id: token, projectId: token.nullable(), status: z.enum(['running', 'cancel_requested', 'succeeded', 'failed', 'cancelled', 'interrupted_unknown']), counts: operationCountsSchema, rootsTotal: count, rootsCompleted: count, outcomeCount: count, journalSaved: z.boolean(), failure: z.strictObject({ code: z.string() }).optional() }).refine(v => v.rootsCompleted <= v.rootsTotal);
export type CopyJob = z.infer<typeof copyJobSchema>;
const outcomeSchema = z.strictObject({ source: z.string().max(32768), destination: z.string().max(32768).nullable(), status: z.enum(['changed', 'skipped', 'failed', 'unknown']), hresult: z.number().int() });
const pageSchema = z.strictObject({ items: z.array(outcomeSchema).max(128), nextOffset: count, done: z.boolean(), failure: z.strictObject({ code: z.string() }).optional() });
export type CopyOutcomePage = z.infer<typeof pageSchema>;
async function call<T>(command: string, request: unknown, schema: z.ZodType<T>): Promise<T> {
  const value: unknown = await invoke(command, request === undefined ? {} : { request });
  const result = schema.safeParse(value);
  if (!result.success || new TextEncoder().encode(JSON.stringify(value)).byteLength >= 256 * 1024) throw new IpcFault('protocol_error');
  return result.data;
}
export const discardCopyCapture = (id: string) => invoke('discard_copy_capture', { request: { id } });
// Bound each IPC packet, not the total operation. Long UNC paths may need fewer
// than 128 roots. A failed append never starts copying or retries automatically.
export function copyChunks<T>(values: readonly T[], payload: (chunk: T[]) => unknown): T[][] {
  const chunks: T[][] = [];
  const encoder = new TextEncoder();
  const append = (chunk: T[]) => {
    if (encoder.encode(JSON.stringify(payload(chunk))).byteLength < 60 * 1024) { chunks.push(chunk); return; }
    if (chunk.length <= 1) throw new IpcFault('payload_limit');
    const middle = Math.floor(chunk.length / 2);
    append(chunk.slice(0, middle)); append(chunk.slice(middle));
  };
  for (let offset = 0; offset < values.length; offset += 128) append(values.slice(offset, offset + 128));
  return chunks;
}
export async function stageCopy(source: OperationSelection | readonly string[], current: () => boolean = () => true): Promise<CopyCapture> {
  const selection = !Array.isArray(source) ? source as OperationSelection : null;
  if (selection && (!identitySchema.safeParse(selection.identity).success || !selection.entryIds.every(id => entryIdSchema.safeParse(id).success) || new Set(selection.entryIds).size !== selection.entryIds.length)) throw new IpcFault('invalid_request');
  const values = selection ? selection.entryIds : (source as readonly string[]).map(normalizeExplorerPath);
  if (!values.length) throw new IpcFault('invalid_request');
  const capture = await call('begin_copy_capture', undefined, captureSchema);
  try {
    const request = (chunk: string[]) => ({ id: capture.id, ...(selection ? { selection: { identity: selection.identity, entryIds: chunk } } : { paths: chunk }) });
    let appended = 0;
    for (const chunk of copyChunks(values, request)) {
      if (!current()) throw new IpcFault('cancelled');
      const result = await call('append_copy_capture', request(chunk), captureSchema);
      appended += chunk.length;
      if (result.id !== capture.id || result.count !== appended || result.sealed) throw new IpcFault('protocol_error');
    }
    if (!current()) throw new IpcFault('cancelled');
    const sealed = await call('seal_copy_capture', { id: capture.id }, captureSchema);
    if (!sealed.sealed || sealed.count !== values.length || sealed.id !== capture.id) throw new IpcFault('protocol_error');
    return sealed;
  } catch (error) { await discardCopyCapture(capture.id).catch(() => {}); throw error; }
}
export async function captureCopyClipboard() {
  const capture = await call('capture_copy_clipboard', undefined, captureSchema);
  if (!capture.sealed || !capture.count) throw new IpcFault('protocol_error');
  return capture;
}
export const exportCopyCapture = (id: string, action: 'clipboard' | 'drag') => call('export_copy_capture', { id, action }, z.boolean());
export async function beginCopyJob(captureId: string, destination: string, projectId: string | null, name?: string, skipExisting = false) {
  try {
    normalizeAbsolutePath(destination);
    if (name !== undefined) {
      if (!name || name.length > 255 || /[/\\]/.test(name) || normalizeRelativePath(name) !== name || skipExisting) throw new Error();
      normalizeRelativePath(name.split('.')[0]?.replace(/ +$/, '') ?? '');
      normalizeAbsolutePath(`${destination}\\${name}`);
    }
  } catch { throw new IpcFault('invalid_path'); }
  return call('begin_copy_job', { captureId, destination, projectId, conflictPolicy: skipExisting ? 'skip' : 'fail', ...(name !== undefined ? { name } : {}) }, copyJobSchema);
}

export const listCopyJobs = () => call('list_copy_jobs', undefined, z.array(copyJobSchema).max(256));
export const cancelCopyJob = (id: string) => call('cancel_copy_job', { id }, copyJobSchema);
export async function readCopyOutcomes(id: string, offset: number) {
  const page = await call('read_copy_job_outcomes', { id, offset, limit: 16 }, pageSchema);
  if (page.nextOffset < offset || (page.items.length > 0 && page.nextOffset === offset)) throw new IpcFault('protocol_error');
  return page;
}
export const copyJobActive = (job: CopyJob) => job.status === 'running' || job.status === 'cancel_requested';
