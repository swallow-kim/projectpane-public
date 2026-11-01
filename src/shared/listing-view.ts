import { z } from 'zod';
import { IpcFault } from './fault';

export const viewSchema = z.strictObject({
  sort: z.enum(['name', 'type', 'size', 'modified']),
  direction: z.enum(['asc', 'desc']),
  foldersFirst: z.boolean(),
  filter: z.string().max(256).refine(value => !value.includes('\0') && new TextEncoder().encode(value).byteLength <= 1024),
}).readonly();
export type ListingView = z.infer<typeof viewSchema>;
export const DEFAULT_VIEW: ListingView = { sort: 'name', direction: 'asc', foldersFirst: true, filter: '' };

export function parseView(value: unknown): ListingView {
  const parsed = viewSchema.safeParse(value);
  if (!parsed.success) throw new IpcFault('invalid_request');
  return parsed.data;
}
export const entryIdSchema = z.string().min(1).max(2048);
export function parseEntryId(value: unknown): string {
  const parsed = entryIdSchema.safeParse(value);
  if (!parsed.success) throw new IpcFault('invalid_request');
  return parsed.data;
}
