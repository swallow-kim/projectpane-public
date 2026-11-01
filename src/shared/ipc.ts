import { invoke, isTauri } from '@tauri-apps/api/core';
import { parseActivation, parseIdentity, parseLocated, parseOffset, parsePath, parseReply, type Identity, type Transport } from './contracts';
import { IpcFault, toFaultCode } from './fault';
import { beginIpcObservation, type IpcCommand } from './ipc-observation';
import { DEFAULT_VIEW, parseEntryId, parseView } from './listing-view';

export const desktop = isTauri();
async function request(command: IpcCommand, args: Readonly<Record<string, unknown>> & { readonly identity: Identity }): Promise<unknown> {
  const complete = beginIpcObservation(command, args.identity);
  try {
    const result = await invoke<unknown>(command, args);
    complete?.('success');
    return result;
  } catch (error) {
    complete?.('failure');
    throw new IpcFault(toFaultCode(error));
  }
}

export const transport: Transport = {
  async start(identity, path, view = DEFAULT_VIEW) {
    const expected = parseIdentity(identity);
    return parseReply(await request('start_listing', { identity: expected, path: parsePath(path), view: parseView(view) }), expected);
  },
  async read(identity, offset) {
    const expected = parseIdentity(identity);
    return parseReply(await request('read_listing', { identity: expected, offset: parseOffset(offset) }), expected);
  },
  async cancel(identity) {
    const result = await request('cancel_listing', { identity: parseIdentity(identity) });
    if (result !== null && result !== undefined) throw new IpcFault('protocol_error');
  },
  async setView(identity, view) {
    const expected = parseIdentity(identity);
    return parseReply(await request('set_listing_view', { identity: expected, view: parseView(view) }), expected);
  },
  async locate(identity, entryId) {
    const expected = parseIdentity(identity);
    return parseLocated(await request('locate_entry', { identity: expected, entryId: parseEntryId(entryId) }), expected);
  },
  async activate(identity, entryId) {
    const expected = parseIdentity(identity);
    return parseActivation(await request('activate_entry', { identity: expected, entryId: parseEntryId(entryId) }), expected);
  },
};
