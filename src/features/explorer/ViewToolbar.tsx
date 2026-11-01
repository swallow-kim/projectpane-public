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
  const [filter, setFilter] = useState(view.filter);
  const composing = useRef(false);
  const input = useRef<HTMLInputElement>(null);
  const placeCaret = useRef(false);
  useImperativeHandle(ref, () => ({ typeFilter(key) {
    if (disabled || composing.current || !input.current) return false;
    // Starting from the list starts a fresh query; direct input editing keeps
    // its normal caret/selection behavior.
    placeCaret.current = true;
    setFilter(key); change({ ...view, filter: key });
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
  return <div className="view-toolbar" aria-label={`패널 ${pane + 1} 정렬 및 필터`}>
    <select aria-label="정렬 기준" value={view.sort} disabled={disabled} onChange={event => {
      const sort = event.target.value;
      if (sort === 'name' || sort === 'type' || sort === 'size' || sort === 'modified') change({ ...view, sort });
    }}>
      <option value="name">이름</option><option value="type">종류</option><option value="size">크기</option><option value="modified">수정일</option>
    </select>
    <button disabled={disabled} aria-label="정렬 방향" title={view.direction === 'asc' ? '오름차순 · 내림차순으로 변경' : '내림차순 · 오름차순으로 변경'} onClick={() => change({ ...view, direction: view.direction === 'asc' ? 'desc' : 'asc' })}>{view.direction === 'asc' ? <ArrowDownAZ size={15} aria-hidden="true" /> : <ArrowUpAZ size={15} aria-hidden="true" />}</button>
    <label className="folders-first"><input type="checkbox" checked={view.foldersFirst} disabled={disabled} onChange={event => change({ ...view, foldersFirst: event.target.checked })} />폴더 우선</label>
    <input ref={input} type="search" aria-label={`패널 ${pane + 1} 이름 필터`} placeholder="이름 필터" title="목록에서 글자를 입력하면 새 필터 시작 · Esc: 비우고 목록으로" value={filter} disabled={disabled}
      onChange={event => { setFilter(event.target.value); if (!composing.current) change({ ...view, filter: event.target.value }); }}
      onKeyDown={event => {
        if (event.key !== 'Escape' || event.defaultPrevented || composing.current || event.nativeEvent.isComposing || event.keyCode === 229
          || event.ctrlKey || event.altKey || event.metaKey) return;
        event.preventDefault(); event.stopPropagation();
        setFilter(''); change({ ...view, filter: '' }); returnToList?.();
      }}
      onCompositionStart={() => { composing.current = true; }}
      onCompositionEnd={event => { composing.current = false; setFilter(event.currentTarget.value); change({ ...view, filter: event.currentTarget.value }); }} />
  </div>;
}
