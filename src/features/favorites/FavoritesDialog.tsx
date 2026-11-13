import { t, useLanguage } from '../../shared/i18n';
import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { favoriteKey, favoriteSchema, type Favorite } from './favorites';
import { normalizeAbsolutePath } from '../../shared/projects-path';
import { useFavorites } from './use-favorites';

export function FavoritesDialog({ close, open, canNewTab = true }: { readonly close: () => void; readonly open?: (path: string, newTab: boolean) => void; readonly canNewTab?: boolean }) {
  useLanguage();
  const store = useFavorites();
  const dialog = useRef<HTMLDialogElement>(null);
  const [editing, setEditing] = useState<number | 'new' | null>(null);
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState<'invalid' | 'duplicate' | null>(null);
  const composing = useRef(false);
  const id = useId();
  useEffect(() => { dialog.current?.showModal(); }, []);
  if (!store) return null;
  const edit = (index: number | 'new', folder?: Favorite) => { setEditing(index); setName(folder?.name ?? ''); setPath(folder?.path ?? ''); setError(null); };
  const submit = async () => {
    if (composing.current || store.busy || editing === null) return;
    const parsed = favoriteSchema.safeParse({ name: name.trim(), path });
    if (!parsed.success) { setError('invalid'); return; }
    const folder = { ...parsed.data, path: normalizeAbsolutePath(path) };
    if (store.folders.some((item, index) => index !== editing && favoriteKey(item.path) === favoriteKey(folder.path))) { setError('duplicate'); return; }
    const next = editing === 'new' ? [...store.folders, folder] : store.folders.map((item, index) => index === editing ? folder : item);
    if (await store.save(next)) setEditing(null);
  };
  return createPortal(<dialog ref={dialog} className="operation-dialog favorites-dialog" aria-labelledby={`${id}-title`}
    onCancel={event => { event.preventDefault(); if (!store.busy) close(); }}
    onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
    onKeyDown={event => { event.stopPropagation(); if (event.nativeEvent.isComposing || event.keyCode === 229 || composing.current) { if (event.key === 'Enter' || event.key === 'Escape') event.preventDefault(); } }}>
    <h2 id={`${id}-title`}>{t("폴더 즐겨찾기", "Favorite folders")}</h2>
    <p>{t("모든 프로젝트에서 같은 목록을 사용합니다. 연결할 수 없는 폴더도 보존되며, 경로를 수정하거나 해제할 수 있습니다.", "All projects share this list. Unavailable folders stay in the list; you can edit their paths or remove them.")}</p>
    {store.error && <p role="alert">{store.error}</p>}
    {error && <p role="alert">{error === 'invalid' ? t('이름과 올바른 절대 폴더 경로를 입력하세요.', 'Enter a name and a valid absolute folder path.') : t('이미 등록된 폴더입니다. 기존 항목을 수정하세요.', 'This folder is already a favorite. Edit the existing entry.')}</p>}
    {!store.ready && !store.error && <p role="status">{t("불러오는 중…", "Loading…")}</p>}
    {editing !== null ? <form onSubmit={event => { event.preventDefault(); event.stopPropagation(); void submit(); }}>
      <label className="operation-field">{t("즐겨찾기 이름", "Favorite name")}<input autoFocus value={name} maxLength={256} disabled={store.busy} onChange={event => setName(event.target.value)} /></label>
      <label className="operation-field">{t("폴더 경로", "Folder path")}<input value={path} disabled={store.busy} spellCheck={false} onChange={event => setPath(event.target.value)} /></label>
      <div className="operation-dialog__actions"><button type="submit" disabled={store.busy}>{t("저장", "Save")}</button><button type="button" disabled={store.busy} onClick={() => setEditing(null)}>{t("편집 취소", "Cancel editing")}</button></div>
    </form> : <>
      <ul className="favorites-list">{store.folders.map((folder, index) => <li key={favoriteKey(folder.path)}>
        <div className="favorite-description"><strong>{folder.name}</strong><span title={folder.path}>{folder.path}</span></div>
        <div className="favorite-actions">
          {open && <><button disabled={store.busy} onClick={() => { open(folder.path, false); close(); }}>{t("열기", "Open")}</button><button disabled={store.busy || !canNewTab} title={!canNewTab ? t("패널당 최대 32개 탭", "Up to 32 tabs per panel") : t("새 탭에서 열기", "Open in a new tab")} onClick={() => { open(folder.path, true); close(); }}>{t("새 탭", "New tab")}</button></>}
          <button disabled={store.busy} onClick={() => edit(index, folder)}>{t("수정", "Edit")}</button><button disabled={store.busy} onClick={() => void store.save(store.folders.filter((_, item) => item !== index))}>{t("해제", "Remove")}</button>
        </div>
      </li>)}</ul>
      {store.ready && store.folders.length === 0 && <p>{t("등록된 즐겨찾기가 없습니다. 패널 경로 옆 별 버튼으로 현재 폴더를 등록하세요.", "No favorites yet. Use the star beside a panel’s path to add the current folder.")}</p>}
      <div className="operation-dialog__actions"><button disabled={!store.ready || store.busy || store.folders.length >= 128} onClick={() => edit('new')}>{t("즐겨찾기 추가", "Add favorite")}</button>
        <button disabled={store.busy} onClick={() => void store.reload()}>{t("다시 불러오기", "Reload")}</button><button disabled={store.busy} onClick={close}>{t("닫기", "Close")}</button></div>
    </>}
  </dialog>, document.body);
}

export function FavoriteDestination({ disabled, choose }: { readonly disabled: boolean; readonly choose: (path: string) => void }) {
  useLanguage();
  const store = useFavorites();
  return <label className="operation-field">{t("즐겨찾는 대상 폴더", "Favorite destination folder")}<select aria-label={t("즐겨찾는 대상 폴더", "Favorite destination folder")} disabled={disabled || !store?.ready || store.busy || !store.folders.length} value="" onChange={event => { if (event.target.value) choose(event.target.value); }}>
    <option value="">{t("폴더를 선택하여 경로 채우기", "Choose a folder to fill in the path")}</option>{store?.folders.map(folder => <option key={favoriteKey(folder.path)} value={folder.path}>{folder.name} — {folder.path}</option>)}
  </select></label>;
}
