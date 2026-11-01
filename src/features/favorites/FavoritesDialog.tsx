import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { favoriteKey, favoriteSchema, type Favorite } from './favorites';
import { normalizeAbsolutePath } from '../../shared/projects-path';
import { useFavorites } from './use-favorites';

export function FavoritesDialog({ close, open, canNewTab = true }: { readonly close: () => void; readonly open?: (path: string, newTab: boolean) => void; readonly canNewTab?: boolean }) {
  const store = useFavorites();
  const dialog = useRef<HTMLDialogElement>(null);
  const [editing, setEditing] = useState<number | 'new' | null>(null);
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  const [error, setError] = useState<string | null>(null);
  const composing = useRef(false);
  const id = useId();
  useEffect(() => { dialog.current?.showModal(); }, []);
  if (!store) return null;
  const edit = (index: number | 'new', folder?: Favorite) => { setEditing(index); setName(folder?.name ?? ''); setPath(folder?.path ?? ''); setError(null); };
  const submit = async () => {
    if (composing.current || store.busy || editing === null) return;
    const parsed = favoriteSchema.safeParse({ name: name.trim(), path });
    if (!parsed.success) { setError('이름과 올바른 절대 폴더 경로를 입력하세요.'); return; }
    const folder = { ...parsed.data, path: normalizeAbsolutePath(path) };
    if (store.folders.some((item, index) => index !== editing && favoriteKey(item.path) === favoriteKey(folder.path))) { setError('이미 등록된 폴더입니다. 기존 항목을 수정하세요.'); return; }
    const next = editing === 'new' ? [...store.folders, folder] : store.folders.map((item, index) => index === editing ? folder : item);
    if (await store.save(next)) setEditing(null);
  };
  return createPortal(<dialog ref={dialog} className="operation-dialog favorites-dialog" aria-labelledby={`${id}-title`}
    onCancel={event => { event.preventDefault(); if (!store.busy) close(); }}
    onCompositionStart={() => { composing.current = true; }} onCompositionEnd={() => { composing.current = false; }}
    onKeyDown={event => { event.stopPropagation(); if (event.nativeEvent.isComposing || event.keyCode === 229 || composing.current) { if (event.key === 'Enter' || event.key === 'Escape') event.preventDefault(); } }}>
    <h2 id={`${id}-title`}>폴더 즐겨찾기</h2>
    <p>모든 프로젝트에서 같은 목록을 사용합니다. 연결할 수 없는 폴더도 보존되며, 경로를 수정하거나 해제할 수 있습니다.</p>
    {store.error && <p role="alert">{store.error}</p>}
    {error && <p role="alert">{error}</p>}
    {!store.ready && !store.error && <p role="status">불러오는 중…</p>}
    {editing !== null ? <form onSubmit={event => { event.preventDefault(); event.stopPropagation(); void submit(); }}>
      <label className="operation-field">즐겨찾기 이름<input autoFocus value={name} maxLength={256} disabled={store.busy} onChange={event => setName(event.target.value)} /></label>
      <label className="operation-field">폴더 경로<input value={path} disabled={store.busy} spellCheck={false} onChange={event => setPath(event.target.value)} /></label>
      <div className="operation-dialog__actions"><button type="submit" disabled={store.busy}>저장</button><button type="button" disabled={store.busy} onClick={() => setEditing(null)}>편집 취소</button></div>
    </form> : <>
      <ul className="favorites-list">{store.folders.map((folder, index) => <li key={favoriteKey(folder.path)}>
        <div className="favorite-description"><strong>{folder.name}</strong><span title={folder.path}>{folder.path}</span></div>
        <div className="favorite-actions">
          {open && <><button disabled={store.busy} onClick={() => { open(folder.path, false); close(); }}>열기</button><button disabled={store.busy || !canNewTab} title={!canNewTab ? '패널당 최대 32개 탭' : '새 탭에서 열기'} onClick={() => { open(folder.path, true); close(); }}>새 탭</button></>}
          <button disabled={store.busy} onClick={() => edit(index, folder)}>수정</button><button disabled={store.busy} onClick={() => void store.save(store.folders.filter((_, item) => item !== index))}>해제</button>
        </div>
      </li>)}</ul>
      {store.ready && store.folders.length === 0 && <p>등록된 즐겨찾기가 없습니다. 패널 경로 옆 별 버튼으로 현재 폴더를 등록하세요.</p>}
      <div className="operation-dialog__actions"><button disabled={!store.ready || store.busy || store.folders.length >= 128} onClick={() => edit('new')}>즐겨찾기 추가</button>
        <button disabled={store.busy} onClick={() => void store.reload()}>다시 불러오기</button><button disabled={store.busy} onClick={close}>닫기</button></div>
    </>}
  </dialog>, document.body);
}

export function FavoriteDestination({ disabled, choose }: { readonly disabled: boolean; readonly choose: (path: string) => void }) {
  const store = useFavorites();
  return <label className="operation-field">즐겨찾는 대상 폴더<select aria-label="즐겨찾는 대상 폴더" disabled={disabled || !store?.ready || store.busy || !store.folders.length} value="" onChange={event => { if (event.target.value) choose(event.target.value); }}>
    <option value="">폴더를 선택하여 경로 채우기</option>{store?.folders.map(folder => <option key={favoriteKey(folder.path)} value={folder.path}>{folder.name} — {folder.path}</option>)}
  </select></label>;
}
