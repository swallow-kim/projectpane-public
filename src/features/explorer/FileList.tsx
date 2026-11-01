import { useCallback, useEffect, useRef, useState } from 'react';
import type { ListingController } from './listing';
import type { ListPosition, PositionUpdate } from './navigation';
import { syntheticRow } from './synthetic';
import { formatModifiedAt, formatSize, typeLabel } from './metadata';
import { useFixedList } from './use-fixed-list';
import { useSelection } from './use-selection';
import { useListPosition } from './use-list-position';
import { EntryIcon } from './EntryIcon';
import { ROW_HEIGHT } from './fixed-list';
import { INTERNAL_TRANSFER } from '../operations/use-file-transfer';
import type { TransferMode } from '../../shared/operations';
import { FileContextMenu, type FileMenuAction } from './FileContextMenu';
import { useMarquee } from './use-marquee';
import './marquee.css';
import { filterTypingKey } from './filter-typing';

export function FileList({ pane, visible, synthetic, listing, composing, position, onPosition, onActivate, transfer, nativeTransfer, beginPointerDrag, beginDrag, canDrop, endDrag, drop, onSelectionPending, menuActions, cancelTransfer, typeFilter }: {
  readonly typeFilter?: (key: string) => boolean;
  readonly cancelTransfer?: () => boolean;
  readonly nativeTransfer?: (action: 'clipboard' | 'paste') => void;
  readonly beginPointerDrag?: (event: PointerEvent, entryId: string, started: () => void) => void;
  readonly menuActions?: readonly FileMenuAction[];
  readonly onSelectionPending?: (pending: boolean) => void;
  readonly transfer?: (action: 'copy' | 'move' | 'paste') => void;
  readonly beginDrag?: () => string | null;
  readonly canDrop?: () => boolean;
  readonly endDrag?: () => void;
  readonly drop?: (token: string, mode: TransferMode) => void;
  readonly pane: number;
  readonly visible: boolean;
  readonly synthetic: boolean;
  readonly listing: ListingController;
  readonly composing: () => boolean;
  readonly position: ListPosition;
  readonly onPosition: (update: PositionUpdate) => void;
  readonly onActivate: (entryId: string) => void;
}) {
  const scroller = useRef<HTMLDivElement>(null);
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const closeMenu = useCallback(() => { setMenu(null); scroller.current?.focus({ preventScroll: true }); }, []);
  const menuReady = visible && !synthetic && !composing() && !listing.state.viewPending
    && ['complete', 'capped'].includes(listing.state.status);
  useEffect(() => { if (!menuReady) setMenu(null); }, [menuReady]);
  const initialized = useRef(position.focusedId !== null);
  const count = synthetic ? 100_000 : listing.state.total;
  const virtual = useFixedList(count, scroller);
  const { rows } = virtual;
  const first = rows[0]?.index ?? 0;
  const last = rows.at(-1)?.index ?? 0;
  const selection = useSelection({ synthetic, listing, position, onPosition, count, reveal: virtual.scrollToIndex, onPending: onSelectionPending });
  const scroll = useListPosition({ visible, synthetic, listing, position, onPosition, scroller });
  const marquee = useMarquee({ scroller, enabled: visible && !composing() && !selection.pending && !menu
    && (synthetic || (!listing.state.viewPending && !listing.state.refreshing && ['complete', 'capped'].includes(listing.state.status))),
    revision: synthetic ? 0 : listing.state.revision, count, selected: position.selected,
    cancelSelection: selection.cancel, commit: selection.marquee });
  const firstEntry = synthetic ? syntheticRow(0) : listing.cache.row(0);
  const loadedAnchor = synthetic ? undefined : listing.cache.row(position.scrollIndex)?.id;
  useEffect(() => {
    // Scroll events already persist offsets. Only fill a missing anchor when
    // its page arrives; unconditional effect dispatches cascade during keys.
    if (position.scrollId === null && loadedAnchor !== undefined) scroll.changed();
  }, [loadedAnchor, position.scrollId]);
  useEffect(() => {
    if (!synthetic && listing.state.status !== 'error') listing.window(first, last);
  }, [synthetic, first, last, count, listing, listing.state.revision]);
  useEffect(() => {
    if (initialized.current || !firstEntry || !count) return;
    initialized.current = true;
    onPosition(current => ({ ...current, selected: new Set([firstEntry.id]), focusedId: firstEntry.id, anchorId: firstEntry.id }));
  }, [firstEntry, count, onPosition, position]);
  const activate = (id: string) => { if (!synthetic && !composing() && !selection.pending && !listing.state.viewPending && !listing.state.refreshing) onActivate(id); };

  return <><div className="listing-table">
    <div className="column-heading marquee-heading"><span>이름</span><span>유형</span><span>크기</span><span title="PC 현지 시간">수정한 날짜</span></div>
    <div ref={scroller} className="file-list marquee-list" data-marquee={marquee.active} role="listbox" aria-label={`패널 ${pane + 1} 파일 목록`}
      onBlur={() => marquee.cancel()}
      onPointerMove={marquee.move} onPointerUp={marquee.up} onPointerCancel={() => marquee.cancel()} onLostPointerCapture={() => marquee.cancel()}
      onClickCapture={event => { if (marquee.click()) { event.preventDefault(); event.stopPropagation(); } }}
      onContextMenu={event => {
        if (!menuActions) return;
        event.preventDefault();
        if (!menuReady || selection.pending) return;
        onPosition(current => ({ ...current, selected: new Set(), focusedId: null, anchorId: null }));
        setMenu({ x: event.clientX, y: event.clientY });
      }}
      aria-multiselectable="true"
      aria-busy={!synthetic && (listing.state.status === 'loading' || listing.state.viewPending === true || listing.state.refreshing === true)}
      aria-activedescendant={rows.some(row => row.index === position.focusedIndex) ? `row-${pane}-${position.focusedIndex}` : undefined}
      onDragOver={event => { if (!selection.pending && canDrop?.() && event.dataTransfer.types.includes(INTERNAL_TRANSFER)) { event.preventDefault(); event.dataTransfer.dropEffect = event.shiftKey ? 'move' : 'copy'; } }}
      onDrop={event => {
        event.preventDefault();
        if (selection.pending || !canDrop?.()) return;
        drop?.(event.dataTransfer.getData(INTERNAL_TRANSFER), event.shiftKey ? 'move' : 'copy');
      }}
      tabIndex={0} onScroll={() => { setMenu(null); scroll.changed(); marquee.scroll(); }} onWheel={scroll.interrupt}
      onPointerDown={event => { scroll.interrupt(); marquee.down(event); }} onKeyDown={event => {
        if (event.defaultPrevented || composing() || event.nativeEvent.isComposing || event.keyCode === 229) return;
        if (event.key === 'Escape' && marquee.cancel()) { event.preventDefault(); event.stopPropagation(); return; }
        if (marquee.active) { event.preventDefault(); event.stopPropagation(); return; }
        // Redirect one printable key, including when the current filter has no
        // results. The original key's default insertion must not run twice.
        if (visible && event.target === event.currentTarget && !document.querySelector('dialog[open]')
          && filterTypingKey(event) && typeFilter?.(event.key)) {
          event.preventDefault(); event.stopPropagation(); selection.cancel(); scroll.interrupt(); return;
        }
        if (!synthetic && (listing.state.viewPending || listing.state.refreshing)) {
          // The retained rows are not an actionable snapshot. Suppress the
          // browser's default End/Page scrolling as well as stale selection.
          if (['Home', 'End', 'ArrowDown', 'ArrowUp', 'PageDown', 'PageUp', ' '].includes(event.key)
            || ((event.ctrlKey || event.metaKey) && ['a', 'c', 'x', 'v'].includes(event.key.toLowerCase()))) event.preventDefault();
          return;
        }
        if (!synthetic && menuReady && !selection.pending && !event.ctrlKey && !event.metaKey && !event.shiftKey) {
          const shortcut = event.altKey ? event.key === 'Enter' ? 'Alt+Enter' : null : event.key;
          const action = menuActions?.find(action => action.shortcut === shortcut);
          if (action) { event.preventDefault(); event.stopPropagation(); if (!event.repeat && !action.disabled) action.run(); return; }
        }
        if (event.altKey) return;
        if (!synthetic && transfer && !selection.pending && event.ctrlKey && event.shiftKey && !event.metaKey && ['c', 'v'].includes(event.key.toLowerCase())) {
          event.preventDefault(); event.stopPropagation();
          if (!event.repeat) transfer(event.key.toLowerCase() === 'c' ? 'copy' : 'paste');
          return;
        }
        if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) {
          if (menuActions && menuReady && !selection.pending) {
            event.preventDefault();
            const element = scroller.current?.querySelector('[data-focused="true"]') ?? scroller.current;
            const bounds = element?.getBoundingClientRect();
            if (bounds) setMenu({ x: bounds.left + 16, y: bounds.top + 16 });
          }
          return;
        }
        if (event.key === 'Escape' && selection.pending) { event.preventDefault(); event.stopPropagation(); selection.cancel(); return; }
        if (event.key === 'Escape' && cancelTransfer?.()) { event.preventDefault(); event.stopPropagation(); return; }
        if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'a') { event.preventDefault(); void selection.all(); return; }
        if (!synthetic && transfer && !selection.pending && event.ctrlKey && !event.metaKey && !event.shiftKey) {
          const action = { c: 'copy', x: 'move', v: 'paste' } as const;
          const key = event.key.toLowerCase();
          if (key === 'c' || key === 'x' || key === 'v') {
            event.preventDefault(); event.stopPropagation();
            if (!event.repeat) {
              if (key === 'x') transfer('move');
              else if (nativeTransfer) nativeTransfer(key === 'c' ? 'clipboard' : 'paste');
              else transfer(action[key]);
            }
            return;
          }
        }
        if (!count) return;
        if (event.key === 'Enter') {
          if (!event.repeat && position.focusedId) { event.preventDefault(); activate(position.focusedId); }
          return;
        }
        let next: number;
        const focusedIndex = selection.focusIndex();
        const page = Math.max(1, Math.floor((scroller.current?.clientHeight ?? ROW_HEIGHT) / ROW_HEIGHT));
        switch (event.key) {
          case 'Home': next = 0; break;
          case 'End': next = count - 1; break;
          case 'ArrowDown': next = focusedIndex + 1; break;
          case 'ArrowUp': next = focusedIndex - 1; break;
          case 'PageDown': next = focusedIndex + page; break;
          case 'PageUp': next = focusedIndex - page; break;
          case ' ': next = focusedIndex; break;
          default: return;
        }
        event.preventDefault(); scroll.interrupt();
        void selection.select(next, event, event.ctrlKey && !event.shiftKey && event.key !== ' ');
      }}>
      <div className="marquee-content" style={{ height: virtual.totalSize, position: 'relative' }}>
        {rows.map(row => {
          const entry = synthetic ? syntheticRow(row.index) : listing.state.viewPending ? undefined : listing.cache.row(row.index);
          const type = entry ? typeLabel(entry.kind, entry.name) : '';
          const modified = entry ? formatModifiedAt(entry.modifiedAtMs) : '';
          const size = entry ? formatSize(entry.sizeBytes) : '';
          return <div role="option" id={`row-${pane}-${row.index}`} aria-selected={entry ? marquee.selected(row.index, entry.id) : false}
            onContextMenu={event => {
              if (!menuActions) return;
              event.preventDefault(); event.stopPropagation();
              if (!entry || !menuReady || selection.pending) return;
              onPosition(current => ({ ...current, selected: current.selected.has(entry.id) ? current.selected : new Set([entry.id]),
                focusedId: entry.id, focusedIndex: row.index, anchorId: current.selected.has(entry.id) ? current.anchorId : entry.id }));
              setMenu({ x: event.clientX, y: event.clientY });
            }}
            aria-posinset={row.index + 1} aria-setsize={count} className="file-row" key={row.key}
            data-entry-id={entry?.id} data-focused={entry?.id === position.focusedId}
            draggable={!beginPointerDrag && !synthetic && Boolean(transfer) && !selection.pending && Boolean(entry && position.selected.has(entry.id))}
            onPointerDown={event => {
              if (event.button === 0 && !synthetic && transfer && !selection.pending && !composing() && entry) beginPointerDrag?.(event.nativeEvent, entry.id, () => {
                scroller.current?.focus({ preventScroll: true });
                onPosition(current => ({ ...current, selected: current.selected.has(entry.id) ? current.selected : new Set([entry.id]),
                  focusedId: entry.id, focusedIndex: row.index, anchorId: current.selected.has(entry.id) ? current.anchorId : entry.id }));
              });
            }}
            onDragStart={event => {
              if (composing() || selection.pending || !transfer) { event.preventDefault(); return; }
              const token = beginDrag?.();
              if (!token) { event.preventDefault(); return; }
              event.dataTransfer.effectAllowed = 'copyMove'; event.dataTransfer.setData(INTERNAL_TRANSFER, token);
            }} onDragEnd={endDrag}
            onClick={event => { if (!synthetic && (listing.state.viewPending || listing.state.refreshing)) return; scroller.current?.focus(); scroll.interrupt(); void selection.select(row.index, event); }}
            onDoubleClick={() => { if (entry) activate(entry.id); }}
            style={{ height: row.size, transform: `translateY(${row.start}px)` }}>
            <span className="file-name" title={entry?.name}>
              {entry && <EntryIcon kind={entry.kind} />}
              {entry?.name ?? (listing.state.viewPending ? '정렬·필터 적용 중…' : listing.state.status === 'error' ? '항목을 불러오지 못했습니다' : '불러오는 중…')}
            </span>
            <span className="kind" title={type}>{type}</span>
            <span className="size" title={entry?.sizeBytes === null ? '폴더 또는 확인할 수 없는 크기' : entry?.sizeBytes !== undefined ? `${entry.sizeBytes.toLocaleString()} bytes` : undefined}>{size}</span>
            <span className="modified" title={modified === '—' ? '수정한 날짜를 확인할 수 없습니다' : modified ? `${modified} (PC 현지 시간)` : undefined}>{modified}</span>
          </div>;
        })}
        {marquee.rectangle && <div className="file-marquee" aria-hidden="true" style={marquee.rectangle} />}
      </div>
    </div>
  </div>{menu && menuReady && menuActions && <FileContextMenu x={menu.x} y={menu.y} actions={menuActions} close={closeMenu} />}
    {(selection.error || selection.pending) && <div className="selection-feedback" role={selection.error ? 'alert' : 'status'}>{selection.error ?? '선택 중…'}</div>}</>;
}
