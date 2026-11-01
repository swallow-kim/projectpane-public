import { invoke } from '@tauri-apps/api/core';
import { toFaultCode } from './fault';
import { parseWatchUpdate, type WatchUpdate } from './watch-contracts';

export async function pollWatch(paths: readonly string[]): Promise<WatchUpdate | null> {
  try {
    return parseWatchUpdate(await invoke<unknown>('poll_watch', { paths }), paths.length);
  } catch (error) {
    // Another binding may still be retiring after a development remount.
    if (toFaultCode(error) === 'busy') return null;
    throw error;
  }
}
