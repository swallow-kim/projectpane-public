import { t, useLanguage } from '../../shared/i18n';
import { useState } from 'react';
import { useFavorites } from './use-favorites';
import { favoriteKey } from './favorites';
import { FavoritesDialog } from './FavoritesDialog';
import { Star, FolderHeart } from 'lucide-react';

export function FavoriteButtons({ path, disabled, open, canNewTab }: { readonly path: string; readonly disabled: boolean; readonly canNewTab: boolean; readonly open: (path: string, newTab: boolean) => void }) {
  useLanguage();
  const store = useFavorites();
  const [show, setShow] = useState(false);
  if (!store) return null;
  const selected = store.find(path);
  let valid = false;
  try { favoriteKey(path); valid = true; } catch { /* Empty/synthetic addresses are not folders. */ }
  const toggle = () => {
    if (!valid || store.busy || !store.ready) return;
    void store.save(selected ? store.folders.filter(item => item !== selected)
      : [...store.folders, { path, name: [...(path.replace(/\\$/, '').split(/[/\\]/).at(-1) || path)].slice(0, 128).join('') }]);
  };
  return <>
    <button type="button" className="favorite-toggle" aria-label={selected ? t("현재 폴더 즐겨찾기 해제", "Remove current folder from favorites") : t("현재 폴더 즐겨찾기 등록", "Add current folder to favorites")} title={selected ? t("즐겨찾기 해제", "Remove from favorites") : t("현재 폴더 즐겨찾기", "Favorite current folder")} aria-pressed={Boolean(selected)} disabled={disabled || !valid || !store.ready || store.busy || (!selected && store.folders.length >= 128)} onClick={toggle}><Star size={15} fill={selected ? "currentColor" : "none"} aria-hidden="true" /></button>
    <button type="button" aria-label={t("폴더 즐겨찾기 열기", "Open favorite folders")} title={t("즐겨찾는 폴더 열기 / 새 탭", "Open a favorite folder / new tab")} disabled={disabled} onClick={() => setShow(true)}><FolderHeart size={15} aria-hidden="true" /></button>
    {show && <FavoritesDialog close={() => setShow(false)} open={open} canNewTab={canNewTab} />}
  </>;
}
