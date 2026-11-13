import { useRef, useState, type CSSProperties, type PointerEvent } from 'react';
import { clampColumn, COLUMN_LABELS, COLUMN_MAX, COLUMN_MIN, columnMinimumWidth, columnStorageKey, columnTemplate, DEFAULT_COLUMNS, parseColumns, type ColumnWidths } from './column-widths';
import './columns.css';

export function useColumnWidths(pane: number) {
  const [widths, setWidths] = useState<ColumnWidths>(() => {
    try { return parseColumns(localStorage.getItem(columnStorageKey(pane))); } catch { return DEFAULT_COLUMNS; }
  });
  const change = (next: ColumnWidths, persist: boolean) => {
    setWidths(next);
    if (persist) { try { localStorage.setItem(columnStorageKey(pane), JSON.stringify(next)); } catch { /* Resizing remains usable when storage is unavailable. */ } }
  };
  return { widths, change, style: { '--file-columns': columnTemplate(widths), '--file-columns-min': `${columnMinimumWidth(widths)}px` } as CSSProperties };
}

export function ColumnHeading({ widths, change }: { widths: ColumnWidths; change: (next: ColumnWidths, persist: boolean) => void }) {
  const drag = useRef<{ pointerId: number; startX: number; startWidth: number; index: number; original: ColumnWidths; next: ColumnWidths } | null>(null);
  const [resizing, setResizing] = useState(false);
  const finish = (event: PointerEvent<HTMLElement>, commit: boolean) => {
    const active = drag.current;
    if (!active || active.pointerId !== event.pointerId) return;
    drag.current = null; setResizing(false);
    change(commit ? active.next : active.original, commit);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return <div className="column-heading marquee-heading" data-resizing={resizing}>
    {COLUMN_LABELS.map((label, index) => <span className="column-label" key={label} title={index === 3 ? 'PC 현지 시간' : undefined}>
      {label}
      <span className="column-resizer" role="separator" aria-orientation="vertical" tabIndex={0}
        aria-label={`${label} 열 너비`} aria-valuemin={COLUMN_MIN[index]} aria-valuemax={COLUMN_MAX} aria-valuenow={widths[index] ?? COLUMN_MIN[index]} aria-valuetext={widths[index] === null ? '자동 너비' : `${widths[index]} 픽셀`}
        title="드래그 또는 좌우 화살표로 너비 조절 · 두 번 클릭하면 기본 너비"
        onPointerDown={event => {
          if (event.button !== 0 || drag.current) return;
          event.preventDefault(); event.stopPropagation(); event.currentTarget.focus();
          const startWidth = event.currentTarget.parentElement?.getBoundingClientRect().width ?? COLUMN_MIN[index];
          drag.current = { pointerId: event.pointerId, startX: event.clientX, startWidth, index, original: widths, next: widths };
          event.currentTarget.setPointerCapture(event.pointerId); setResizing(true);
        }}
        onPointerMove={event => {
          const active = drag.current;
          if (!active || active.pointerId !== event.pointerId) return;
          const next = [...active.original]; next[active.index] = clampColumn(active.index, active.startWidth + event.clientX - active.startX);
          active.next = next; change(next, false);
        }}
        onPointerUp={event => finish(event, true)} onPointerCancel={event => finish(event, false)} onLostPointerCapture={event => finish(event, false)}
        onDoubleClick={event => { event.preventDefault(); event.stopPropagation(); const next = [...widths]; next[index] = DEFAULT_COLUMNS[index]; change(next, true); }}
        onKeyDown={event => {
          if (event.key === 'Escape' && drag.current) {
            event.preventDefault(); event.stopPropagation(); const active = drag.current; drag.current = null; setResizing(false); change(active.original, false);
            if (event.currentTarget.hasPointerCapture(active.pointerId)) event.currentTarget.releasePointerCapture(active.pointerId);
            return;
          }
          if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key) || drag.current) return;
          event.preventDefault(); event.stopPropagation();
          const current = event.currentTarget.parentElement?.getBoundingClientRect().width ?? COLUMN_MIN[index];
          const width = event.key === 'Home' ? COLUMN_MIN[index] : event.key === 'End' ? COLUMN_MAX : current + (event.key === 'ArrowRight' ? 1 : -1) * (event.shiftKey ? 1 : 10);
          const next = [...widths]; next[index] = clampColumn(index, width); change(next, true);
        }} />
    </span>)}
  </div>;
}
