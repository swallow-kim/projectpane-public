import { z } from 'zod';
import { IpcFault } from './fault';

const mask = z.number().int().min(0).max(15);
const updateSchema = z.strictObject({ changed: mask, unavailable: mask });
export type WatchUpdate = Readonly<z.infer<typeof updateSchema>>;
export function parseWatchUpdate(value: unknown, slots: number): WatchUpdate {
  const update = updateSchema.parse(value);
  if (!Number.isInteger(slots) || slots < 0 || slots > 4 || update.changed >= 2 ** slots
    || update.unavailable >= 2 ** slots || (update.changed & update.unavailable) !== 0) throw new IpcFault('protocol_error');
  return update;
}
