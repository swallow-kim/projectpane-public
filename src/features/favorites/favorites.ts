import { z } from 'zod';
import { absoluteProjectPathSchema, normalizeAbsolutePath } from '../../shared/projects-path';
import { IpcFault } from '../../shared/fault';

export const favoriteSchema = z.strictObject({ name: z.string().min(1).refine(value => [...value].length <= 128 && value.trim().length > 0 && !/[\x00-\x1f\x7f-\x9f]/.test(value)), path: absoluteProjectPathSchema });
export const favoritesSchema = z.strictObject({ schemaVersion: z.literal(1), revision: z.number().int().min(0).max(Number.MAX_SAFE_INTEGER), folders: z.array(favoriteSchema).max(128) });
export type Favorite = z.infer<typeof favoriteSchema>;
export type Favorites = z.infer<typeof favoritesSchema>;
export function favoriteKey(path: string): string { return normalizeAbsolutePath(path).toLowerCase(); }
export function parseFavorites(input: unknown): Favorites {
  const parsed = favoritesSchema.safeParse(input);
  if (!parsed.success || new TextEncoder().encode(JSON.stringify(parsed.data)).length > 256 * 1024
    || new Set(parsed.data.folders.map(folder => favoriteKey(folder.path))).size !== parsed.data.folders.length) throw new IpcFault('protocol_error');
  return { ...parsed.data, folders: parsed.data.folders.map(folder => ({ ...folder, path: normalizeAbsolutePath(folder.path) })) };
}
