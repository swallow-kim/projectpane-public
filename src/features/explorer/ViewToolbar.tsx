import { t, useLanguage } from '../../shared/i18n';
import { ArrowDownAZ, ArrowUpAZ } from 'lucide-react';
import { useEffect, useImperativeHandle, useLayoutEffect, useRef, useState, type Ref } from 'react';
import type { TabView } from './navigation';

export type ViewToolbarHandle = { typeFilter: (key: string) => boolean };

export function ViewToolbar({ pane, view, disabled, change, ref, returnToList }: {
  readonly ref?: Ref<ViewToolbarHandle>;
  readonly returnToList?: () => void;
  readonly pane: number;
  readonly view: TabView;
  readonly disabled: boolean;
  readonly change: (view: TabView) => void;
}) {
  useLanguage();
  const [filter, setFilter] = useState(view.filter);
  const composing = useRef(false);
  const input = useRef<HTMLInputElement>(null);
  const placeCaret = useRef(false);
  const latest = useRef({ view, change });
  latest.current = { view, change };
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancelFilter = () => {
    if (timer.current !== null) clearTimeout(timer.current);
    timer.current = null;
  };
  const commitFilter = (value: string, immediate = false) => {
    cancelFilter();
    const apply = () => { timer.current = null; latest.current.change({ ...latest.current.view, filter: value }); };
    if (immediate) apply(); else timer.current = setTimeout(apply, 120);
  };
  useEffect(() => () => cancelFilter(), []);
  useImperativeHandle(ref, () => ({ typeFilter(key) {
    if (disabled || composing.current || !input.current) return false;
    // Starting from the list starts a fresh query; direct input editing keeps
    // its normal caret/selection behavior.
    placeCaret.current = true;
    setFilter(key); commitFilter(key);
    input.current.focus({ preventScroll: true });
    return true;
  } }));
  useLayoutEffect(() => {
    if (!placeCaret.current) return;
    placeCaret.current = false;
    input.current?.setSelectionRange(filter.length, filter.length);
  });
  useEffect(() => {
    const reset = () => { composing.current = false; };
    window.addEventListener('blur', reset);
    return () => window.removeEventListener('blur', reset);
  }, []);
  return <div className="view-toolbar" aria-label={t('패널 {number} 정렬 및 필터', 'Pane {number} sorting and filtering', { number: pane + 1 })}>
    <select aria-label={t("정렬 기준", "Sort by")} value={view.sort} disabled={disabled} onChange={event => {
      const sort = event.target.value;
      if (sort === 'name' || sort === 'type' || sort === 'size' || sort === 'modified') { cancelFilter(); change({ ...view, filter, sort }); }
    }}>
      <option value="name">{t("이름", "Name")}</option><option value="type">{t("종류", "Type")}</option><option value="size">{t("크기", "Size")}</option><option value="modified">{t("수정일", "Date modified")}</option>
    </select>
    <button disabled={disabled} aria-label={t("정렬 방향", "Sort direction")} title={view.direction === 'asc' ? t("오름차순 · 내림차순으로 변경", "Ascending · Switch to descending") : t("내림차순 · 오름차순으로 변경", "Descending · Switch to ascending")} onClick={() => (cancelFilter(), change({ ...view, filter, direction: view.direction === 'asc' ? 'desc' : 'asc' }))}>{view.direction === 'asc' ? <ArrowDownAZ size={15} aria-hidden="true" /> : <ArrowUpAZ size={15} aria-hidden="true" />}</button>
    <label className="folders-first"><input type="checkbox" checked={view.foldersFirst} disabled={disabled} onChange={event => (cancelFilter(), change({ ...view, filter, foldersFirst: event.target.checked }))} />{t("폴더 우선", "Folders first")}</label>
    <input ref={input} type="search" aria-label={t('패널 {number} 이름 필터', 'Pane {number} name filter', { number: pane + 1 })} placeholder={t("이름 필터", "Filter by name")} title={t("목록에서 글자를 입력하면 새 필터 시작 · Esc: 비우고 목록으로", "Type in the list to start a new filter · Esc: clear and return to the list")} value={filter} disabled={disabled}
      onChange={event => { setFilter(event.target.value); if (!composing.current) commitFilter(event.target.value); }}
      onKeyDown={event => {
        if (event.key !== 'Escape' || event.defaultPrevented || composing.current || event.nativeEvent.isComposing || event.keyCode === 229
          || event.ctrlKey || event.altKey || event.metaKey) return;
        event.preventDefault(); event.stopPropagation();
        setFilter(''); commitFilter('', true); returnToList?.();
      }}
      onCompositionStart={() => { cancelFilter(); composing.current = true; }}
      onCompositionEnd={event => { composing.current = false; setFilter(event.currentTarget.value); commitFilter(event.currentTarget.value); }} />
  </div>;
}
