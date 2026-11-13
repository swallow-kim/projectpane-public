import { t, useLanguage } from '../../shared/i18n';
import { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { localizePalette, searchPalette, type PaletteAction, type PaletteItem } from './palette';
import './palette.css';

export function QuickPalette({ items, dismiss, run }: { readonly items: readonly PaletteItem[]; readonly dismiss: () => void; readonly run: (action: PaletteAction) => void }) {
  useLanguage();
  const dialog = useRef<HTMLDialogElement>(null);
  const opener = useRef(document.activeElement);
  const input = useRef<HTMLInputElement>(null);
  const composition = useRef(false), settling = useRef(false), leaving = useRef(false);
  const [query, setQuery] = useState(''), [index, setIndex] = useState(0);
  const id = useId();
  const results = searchPalette(localizePalette(items), query);
  const active = Math.min(index, Math.max(0, results.length - 1));
  useEffect(() => { dialog.current?.showModal(); input.current?.focus(); }, []);
  useEffect(() => { document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: 'nearest' }); }, [id, active, query]);
  const close = (action?: PaletteAction) => {
    if (leaving.current) return;
    leaving.current = true; dialog.current?.close(); dismiss();
    requestAnimationFrame(() => {
      if (opener.current instanceof HTMLElement && opener.current.isConnected && !opener.current.closest('[inert]')) opener.current.focus({ preventScroll: true });
      if (action) run(action);
    });
  };
  return createPortal(<dialog ref={dialog} className="operation-dialog quick-palette" aria-labelledby={`${id}-title`}
    onCancel={event => { event.preventDefault(); if (!composition.current && !settling.current) close(); }}
    onCompositionStart={() => { composition.current = true; }} onCompositionEnd={() => { composition.current = false; settling.current = true; }}
    onKeyUp={() => { if (!composition.current) settling.current = false; }}
    onPointerDown={() => { if (!composition.current) settling.current = false; }}
    onKeyDown={event => {
      event.stopPropagation();
      // Candidate navigation and commit belong to the IME, not the palette.
      if (event.defaultPrevented || event.nativeEvent.isComposing || event.keyCode === 229 || composition.current) return;
      if (settling.current) {
        if (['Enter', 'Escape', 'ArrowUp', 'ArrowDown'].includes(event.key)) event.preventDefault();
        return;
      }
      if (event.key === 'Escape') { event.preventDefault(); close(); }
      if (event.target === input.current) {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
          event.preventDefault(); setIndex(results.length ? (active + (event.key === 'ArrowDown' ? 1 : results.length - 1)) % results.length : 0);
        }
        if (event.key === 'Enter') { event.preventDefault(); if (!event.repeat && results[active]) close(results[active].action); }
      }
    }}>
    <h2 id={`${id}-title`}>{t("빠른 이동과 명령", "Quick navigation and commands")}</h2>
    <input ref={input} role="combobox" aria-label={t("빠른 이동 검색", "Search quick navigation")} aria-autocomplete="list" aria-expanded="true" aria-controls={`${id}-results`}
      aria-activedescendant={results.length ? `${id}-${active}` : undefined} autoComplete="off" maxLength={256} value={query}
      placeholder={t("프로젝트, 탭, 즐겨찾기, 최근 폴더, 명령", "Projects, tabs, favorites, recent folders, commands")} onChange={event => { setQuery(event.target.value); setIndex(0); }} />
    <p>{t("열린 작업 공간의 정보만 검색합니다. 최근 폴더는 열린 탭의 탐색 이력이며, 디스크를 검색하지 않습니다.", "Searches only the open workspace. Recent folders come from open tabs’ navigation history; this does not search the disk.")}</p>
    <div role="listbox" aria-label={t("빠른 이동 결과", "Quick navigation results")} id={`${id}-results`} className="palette-results">
      {results.map((item, position) => <button type="button" role="option" id={`${id}-${position}`} key={item.id} tabIndex={-1} aria-selected={position === active}
        title={`${item.label} · ${item.detail}`} onMouseDown={event => event.preventDefault()} onClick={() => { if (!composition.current) close(item.action); }}>
        <span className="palette-group">{item.group}</span><strong>{item.label}</strong><span className="palette-detail">{item.detail}</span>
      </button>)}
    </div>
    <div className="palette-footer"><span role="status">{results.length ? t('{count}개 표시 · 최대 100개', '{count} shown · Up to 100', { count: results.length }) : t("일치하는 항목이 없습니다.", "No matching items.")}</span><button onClick={() => close()}>{t("닫기", "Close")}</button></div>
  </dialog>, document.body);
}
