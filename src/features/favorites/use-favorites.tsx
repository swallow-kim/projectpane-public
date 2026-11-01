import { createContext, useCallback, useContext, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { desktop } from '../../shared/ipc';
import { favoriteKey, parseFavorites, type Favorite, type Favorites } from './favorites';

export function useFavoritesStore() {
  const [value, setValue] = useState<Favorites | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pending = useRef<Promise<void> | null>(null);
  const failedSave = useRef(false);
  const mounted = useRef(true);
  const current = useRef(value); current.current = value;
  const run = useCallback((command: string, folders?: readonly Favorite[]) => {
    if (!desktop || pending.current) return Promise.resolve(false);
    const previous = current.current;
    if (folders && !previous) return Promise.resolve(false);
    setBusy(true); setError(null);
    let success = false;
    const work = (async () => {
      try {
        const result = parseFavorites(await invoke(command, folders ? { request: { expectedRevision: previous!.revision, folders } } : undefined));
        if (!mounted.current) return;
        current.current = result; setValue(result); failedSave.current = false; success = true;
      } catch {
        if (!mounted.current) return;
        if (folders) failedSave.current = true;
        setError(folders ? '즐겨찾기 저장을 확인하지 못했습니다. 다시 불러와 저장 상태를 확인하세요. 기존 목록은 보존됩니다.' : '즐겨찾기를 불러오지 못했습니다. 저장 파일은 변경하지 않았습니다. 다시 불러오거나 저장 경로의 권한을 확인하세요.');
      } finally { pending.current = null; if (mounted.current) setBusy(false); }
    })();
    pending.current = work;
    return work.then(() => success);
  }, []);
  const reload = useCallback(() => run('list_favorites'), [run]);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const save = (folders: readonly Favorite[]) => run('save_favorites', folders);
  const flush = useCallback(async (milliseconds: number) => {
    const work = pending.current;
    if (!work) return !failedSave.current;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const done = await Promise.race([work.then(() => true), new Promise<boolean>(resolve => { timer = setTimeout(() => resolve(false), milliseconds); })]);
    clearTimeout(timer);
    return done && !failedSave.current;
  }, []);
  return { folders: value?.folders ?? [], ready: value !== null, busy, error, reload, save, flush,
    find: (path: string) => { try { return value?.folders.find(folder => favoriteKey(folder.path) === favoriteKey(path)); } catch { return undefined; } } };
}
export type FavoritesStore = ReturnType<typeof useFavoritesStore>;
export const FavoritesContext = createContext<FavoritesStore | null>(null);
export const useFavorites = () => useContext(FavoritesContext);
