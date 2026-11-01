import { z } from 'zod';
import { IpcFault } from './fault';
import { entryIdSchema, type ListingView } from './listing-view';

export const MAX_ENTRIES = 100_000;
export const MAX_FRAME = 256 * 1024;
export const MAX_SNAPSHOT_BYTES = 32 * 1024 * 1024;
export const PAGE_SIZE = 64;
export const CACHE_PAGES = 8;
const integer = (max: number, min = 0) => z.number().int().min(min).max(max);
const token = z.string().regex(/^[A-Za-z0-9-]{1,80}$/);
const bytes = (value: string) => new TextEncoder().encode(value).byteLength;

export const identitySchema = z.strictObject({
  pane: integer(3), tabId: token.brand<'TabId'>(), requestId: token.brand<'RequestId'>(),
  generation: integer(Number.MAX_SAFE_INTEGER, 1),
}).readonly();
export type Identity = z.infer<typeof identitySchema>;

export function parseIdentity(value: unknown): Identity {
  const parsed = identitySchema.safeParse(value);
  if (!parsed.success) throw new IpcFault('invalid_request');
  return parsed.data;
}
export function sameRequest(a: Identity, b: Identity): boolean {
  return a.pane === b.pane && a.tabId === b.tabId && a.requestId === b.requestId && a.generation === b.generation;
}

export const entrySchema = z.strictObject({
  id: entryIdSchema, name: z.string().max(32_768),
  kind: z.enum(['file', 'folder', 'link', 'unknown']),
  modifiedAtMs: integer(8_640_000_000_000_000, -8_640_000_000_000_000).nullable(),
  sizeBytes: integer(Number.MAX_SAFE_INTEGER).nullable(),
}).readonly();
export type Entry = z.infer<typeof entrySchema>;
export const capReasonSchema = z.enum(['entry_limit', 'snapshot_budget', 'payload_budget']);
export type CapReason = z.infer<typeof capReasonSchema>;
export const batchSchema = z.strictObject({
  offset: integer(MAX_ENTRIES), entries: z.array(entrySchema).max(PAGE_SIZE).readonly(),
  total: integer(MAX_ENTRIES), complete: z.boolean(), capped: z.boolean(),
  snapshotBytes: integer(MAX_SNAPSHOT_BYTES), capReason: capReasonSchema.nullable(),
  revision: integer(Number.MAX_SAFE_INTEGER),
}).superRefine((batch, context) => {
  if (batch.offset + batch.entries.length > batch.total || (batch.capped && !batch.complete) || batch.capped !== (batch.capReason !== null)) {
    context.addIssue({ code: 'custom', message: 'Inconsistent listing bounds' });
  }
  if (new Set(batch.entries.map(entry => entry.id)).size !== batch.entries.length) {
    context.addIssue({ code: 'custom', message: 'Repeated row identity' });
  }
}).readonly();
export type Batch = z.infer<typeof batchSchema>;
const replySchema = z.strictObject({ identity: identitySchema, batch: batchSchema }).readonly();
export type Reply = z.infer<typeof replySchema>;

const locatedSchema = z.strictObject({ identity: identitySchema, revision: integer(Number.MAX_SAFE_INTEGER), index: integer(MAX_ENTRIES - 1).nullable() }).readonly();
export type LocatedEntry = z.infer<typeof locatedSchema>;
export function parseLocated(value: unknown, expected: Identity): LocatedEntry {
  const parsed = locatedSchema.safeParse(value);
  if (!parsed.success) throw new IpcFault('protocol_error');
  if (!sameRequest(parsed.data.identity, expected)) throw new IpcFault('stale_request');
  return parsed.data;
}
const activationSchema = z.discriminatedUnion('kind', [
  z.strictObject({ identity: identitySchema, kind: z.literal('file'), path: z.null() }),
  z.strictObject({ identity: identitySchema, kind: z.literal('folder'), path: z.string().min(1).max(32_768) }),
]).readonly();
export type Activation = z.infer<typeof activationSchema>;
export function parseActivation(value: unknown, expected: Identity): Activation {
  const parsed = activationSchema.safeParse(value);
  if (!parsed.success) throw new IpcFault('protocol_error');
  if (!sameRequest(parsed.data.identity, expected)) throw new IpcFault('stale_request');
  if (parsed.data.kind === 'folder') parsePath(parsed.data.path);
  return parsed.data;
}

export function parseReply(value: unknown, expected: Identity): Reply {
  const parsed = replySchema.safeParse(value);
  if (!parsed.success || bytes(JSON.stringify(parsed.data)) >= MAX_FRAME) throw new IpcFault('protocol_error');
  if (!sameRequest(parsed.data.identity, expected)) throw new IpcFault('stale_request');
  return parsed.data;
}

const pathSchema = z.string().min(1).max(32_768).refine(value => !value.includes('\0') && bytes(value) <= 32_768);
export function parsePath(value: unknown): string {
  const parsed = pathSchema.safeParse(value);
  if (!parsed.success) throw new IpcFault('invalid_path');
  return parsed.data;
}
export function parseOffset(value: unknown): number | null {
  const parsed = integer(MAX_ENTRIES).nullable().safeParse(value);
  if (!parsed.success) throw new IpcFault('invalid_request');
  return parsed.data;
}

export const startupWarningSchema = z.enum(['state_unavailable', 'state_invalid', 'log_unavailable', 'window_restore_failed']);
export type StartupWarning = z.infer<typeof startupWarningSchema>;
export const startupSchema = z.strictObject({
  folders: z.array(pathSchema).max(4).readonly(),
  error: z.enum(['invalid_startup_args', 'startup_unavailable']).nullable(),
  warnings: z.array(startupWarningSchema).max(4).readonly(),
}).refine(config => config.error === null || config.folders.length === 0).readonly();
export type StartupConfig = z.infer<typeof startupSchema>;

export interface Transport {
  start(identity: Identity, path: string, view?: ListingView): Promise<Reply>;
  read(identity: Identity, offset: number | null): Promise<Reply>;
  cancel(identity: Identity): Promise<void>;
  setView(identity: Identity, view: ListingView): Promise<Reply>;
  locate(identity: Identity, entryId: string): Promise<LocatedEntry>;
  activate(identity: Identity, entryId: string): Promise<Activation>;
}
