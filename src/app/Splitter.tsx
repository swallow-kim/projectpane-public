import { t, useLanguage } from '../shared/i18n';
import { useRef, type PointerEvent } from 'react';

export type LayoutCount = 1 | 2 | 4;
export function Splitter({ axis, value, change }: {
  readonly axis: 'columns' | 'rows';
  readonly value: number;
  readonly change: (value: number) => void;
}) {
  useLanguage();
  const dragging = useRef<number | null>(null);
  const stop = (element: HTMLDivElement, pointerId: number) => {
    if (dragging.current !== pointerId) return;
    dragging.current = null;
    if (element.hasPointerCapture(pointerId)) element.releasePointerCapture(pointerId);
  };
  const update = (event: PointerEvent<HTMLDivElement>) => {
    const bounds = event.currentTarget.parentElement?.getBoundingClientRect();
    if (!bounds) return;
    const percent = axis === 'columns' ? (event.clientX - bounds.left) / bounds.width : (event.clientY - bounds.top) / bounds.height;
    change(Math.max(20, Math.min(80, Math.round(percent * 100))));
  };
  return <div role="separator" tabIndex={0} className={`splitter splitter-${axis}`} aria-label={axis === 'columns' ? t('좌우 패널 비율', 'Horizontal pane ratio') : t('상하 패널 비율', 'Vertical pane ratio')}
    aria-orientation={axis === 'columns' ? 'vertical' : 'horizontal'} aria-valuemin={20} aria-valuemax={80} aria-valuenow={value}
    aria-valuetext={`${value}% / ${100 - value}%`}
    onPointerDown={event => { if (event.button !== 0 || dragging.current !== null) return; dragging.current = event.pointerId; event.currentTarget.setPointerCapture(event.pointerId); update(event); }}
    onPointerMove={event => {
      if (dragging.current !== event.pointerId) return;
      if (!(event.buttons & 1)) { stop(event.currentTarget, event.pointerId); return; }
      update(event);
    }}
    onPointerUp={event => stop(event.currentTarget, event.pointerId)}
    onPointerCancel={event => stop(event.currentTarget, event.pointerId)}
    onLostPointerCapture={event => { if (dragging.current === event.pointerId) dragging.current = null; }}
    onKeyDown={event => {
      if (event.defaultPrevented || event.nativeEvent.isComposing || event.keyCode === 229 || event.ctrlKey || event.altKey || event.metaKey) return;
      if (event.key === 'Escape' && dragging.current !== null) { event.preventDefault(); stop(event.currentTarget, dragging.current); return; }
      const backward = axis === 'columns' ? 'ArrowLeft' : 'ArrowUp';
      const forward = axis === 'columns' ? 'ArrowRight' : 'ArrowDown';
      const next = event.key === backward ? value - (event.shiftKey ? 10 : 1) : event.key === forward ? value + (event.shiftKey ? 10 : 1)
        : event.key === 'Home' ? 20 : event.key === 'End' ? 80 : null;
      if (next !== null) { event.preventDefault(); change(Math.max(20, Math.min(80, next))); }
    }} />;
}
