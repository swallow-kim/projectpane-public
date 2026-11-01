import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { z } from 'zod';

const attemptId = z.number().int().min(1).max(4294967295);
const closePayload = z.strictObject({ attempt: attemptId, timeoutMs: z.number().int().min(1).max(2500), forUpdate: z.boolean().optional() });
const failedPayload = z.strictObject({ attempt: attemptId });
export async function listenForProjectClose(capture: (requirePersisted?: boolean) => boolean, flush: (milliseconds: number) => Promise<boolean>, failed: () => void): Promise<() => void> {
  let latest = 0;
  let closing: number | null = null;
  const target = { target: { kind: 'WebviewWindow', label: 'main' } } as const;
  const stopFailed = await listen<unknown>('projectpane-close-failed', event => {
    const parsed = failedPayload.safeParse(event.payload);
    if (parsed.success && parsed.data.attempt === latest) { closing = null; failed(); }
  }, target);
  try {
    const stopRequested = await listen<unknown>('projectpane-close-requested', event => {
      const parsed = closePayload.safeParse(event.payload);
      if (!parsed.success || parsed.data.attempt <= latest) return;
      const { attempt, timeoutMs } = parsed.data;
      latest = attempt; closing = attempt;
      void (async () => {
        let saved = false;
        try {
          if (capture(parsed.data.forUpdate)) saved = await flush(Math.max(1, timeoutMs - 500));
        } catch { saved = false; }
        if (closing !== attempt) return;
        if (!saved) failed();
        try { await invoke('project_close_ready', { attempt, saved }); }
        catch { if (closing === attempt) failed(); }
        finally { if (closing === attempt) closing = null; }
      })();
    }, target);
    return () => { closing = null; stopRequested(); stopFailed(); };
  } catch (error) { stopFailed(); throw error; }
}
