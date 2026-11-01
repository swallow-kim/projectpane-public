import { invoke, isTauri } from '@tauri-apps/api/core';
import { MAX_FRAME, startupSchema, type StartupConfig } from './contracts';
export type { StartupConfig } from './contracts';

export async function loadStartupConfig(): Promise<StartupConfig> {
  if (!isTauri()) return { folders: [], error: null, warnings: [] };
  try {
    const parsed = startupSchema.safeParse(await invoke<unknown>('startup_config'));
    if (parsed.success && new TextEncoder().encode(JSON.stringify(parsed.data)).byteLength < MAX_FRAME) return parsed.data;
  } catch (error) {
    // Startup is a boundary: no untrusted rejection text reaches the interface.
    if (error instanceof Error) return { folders: [], error: 'startup_unavailable', warnings: [] };
  }
  return { folders: [], error: 'startup_unavailable', warnings: [] };
}
