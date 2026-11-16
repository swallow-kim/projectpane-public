import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { z } from 'zod';
import { desktop } from '../../shared/ipc';
import { rootKind, type ProjectRootKind } from './project-root-kind';

const reply = z.array(z.strictObject({ path: z.string(), kind: z.enum(['windows', 'wsl', 'network', 'unknown']) })).max(512);
export async function classifyRoots(paths: readonly string[], call: typeof invoke = invoke): Promise<ReadonlyMap<string, ProjectRootKind>> {
  const result = new Map<string, ProjectRootKind>();
  for (let offset = 0; offset < paths.length; offset += 512) {
    const batch = paths.slice(offset, offset + 512);
    const entries = reply.parse(await call('classify_project_roots', { request: { paths: batch } }));
    if (entries.length !== batch.length || entries.some((entry, index) => entry.path !== batch[index])) throw new Error('Invalid root classification reply');
    for (const entry of entries) result.set(entry.path, entry.kind);
  }
  return result;
}
/** Resolve mapped drives without accessing folder contents; ignore obsolete IPC replies. */
export function useProjectRootKinds(paths: readonly string[]) {
  const key = JSON.stringify([...new Set(paths)].sort());
  const [resolved, setResolved] = useState<{ key: string; kinds: ReadonlyMap<string, ProjectRootKind> } | null>(null);
  useEffect(() => {
    if (!desktop) return;
    let live = true;
    const roots: string[] = JSON.parse(key);
    void classifyRoots(roots).then(kinds => { if (live) setResolved({ key, kinds }); }).catch(() => { if (live) setResolved({ key, kinds: new Map() }); });
    return () => { live = false; };
  }, [key]);
  return (path: string): ProjectRootKind => {
    const lexical = rootKind(path);
    if (!desktop || lexical !== 'windows') return lexical;
    return resolved?.key === key ? resolved.kinds.get(path) ?? 'unknown' : 'unknown';
  };
}
