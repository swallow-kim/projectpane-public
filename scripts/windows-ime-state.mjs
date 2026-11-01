import fs from 'node:fs/promises';
import { randomUUID } from 'node:crypto';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';

export const STATE_MAX_ATTEMPTS = 20;
export const STATE_RETRY_DELAY_MS = 25;

// At most 19 asynchronous waits (475 ms total, excluding filesystem latency).
// Windows can also report EPERM for permanent denial; the cap still surfaces it.
// Await sequential calls when publication order matters. Dependencies are a test seam.
export async function writeStateJson(path, value, {
  io = fs, platform = process.platform, wait = delay,
} = {}) {
  const json = JSON.stringify(value, null, 2);
  if (json === undefined) throw new TypeError('State must serialize to JSON');
  const destination = resolve(path);
  const temporary = `${destination}.${randomUUID()}.tmp`;
  const handle = await io.open(temporary, 'wx');
  try {
    try {
      await handle.writeFile(json, 'utf8');
    } catch (error) {
      try { await handle.close(); } catch (closeError) {
        throw new AggregateError([error, closeError], 'State write and close failed', { cause: error });
      }
      throw error;
    }
    await handle.close();
    for (let attempts = 1; attempts <= STATE_MAX_ATTEMPTS; attempts++) {
      try {
        await io.rename(temporary, destination);
        return { attempts, retries: attempts - 1 };
      } catch (error) {
        if (platform !== 'win32' || !['EPERM', 'EBUSY'].includes(error?.code)
          || attempts === STATE_MAX_ATTEMPTS) throw error;
        await wait(STATE_RETRY_DELAY_MS);
      }
    }
  } catch (error) {
    try { await io.unlink(temporary); } catch (cleanupError) {
      if (cleanupError.code !== 'ENOENT') {
        throw new AggregateError([error, cleanupError], 'State publish and temp cleanup failed', { cause: error });
      }
    }
    throw error;
  }
}
