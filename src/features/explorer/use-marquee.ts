import { useLayoutEffect, useRef, useState, type PointerEvent, type RefObject } from 'react';
import { marqueeRange, marqueeSelected, type MarqueePoint, type MarqueeSelection } from './marquee';

type Options = {
  readonly scroller: RefObject<HTMLDivElement | null>;
  readonly enabled: boolean;
  readonly revision: number;
  readonly count: number;
  readonly selected: ReadonlySet<string>;
  readonly cancelSelection: () => void;
  readonly commit: (selection: MarqueeSelection) => Promise<void>;
};
type Gesture = {
  readonly pointerId: number;
  readonly origin: MarqueePoint;
  readonly baseline: ReadonlySet<string>;
  readonly mode: MarqueeSelection['mode'];
  client: MarqueePoint;
  moved: boolean;
};
type Preview = MarqueeSelection & { readonly start: MarqueePoint; readonly end: MarqueePoint };
const GUTTER = 12;

export function useMarquee(options: Options) {
  const latest = useRef(options);
  latest.current = options;
  const gesture = useRef<Gesture | null>(null);
  const frame = useRef(0);
  const suppressClick = useRef(false);
  const [preview, setPreview] = useState<Preview | null>(null);
  const point = (element: HTMLDivElement, client: MarqueePoint): MarqueePoint => {
    const bounds = element.getBoundingClientRect();
    return { x: Math.max(0, Math.min(element.clientWidth, client.x - bounds.left)),
      y: Math.max(0, Math.min(element.clientHeight, client.y - bounds.top)) + element.scrollTop };
  };
  const snapshot = (current: Gesture, element: HTMLDivElement): Preview => {
    const end = point(element, current.client);
    return { start: current.origin, end, baseline: current.baseline, mode: current.mode,
      range: marqueeRange(current.origin, end, latest.current.count, element.clientWidth - GUTTER) };
  };
  const clear = () => {
    const current = gesture.current;
    gesture.current = null;
    cancelAnimationFrame(frame.current);
    frame.current = 0;
    const element = latest.current.scroller.current;
    if (current && element?.hasPointerCapture(current.pointerId)) element.releasePointerCapture(current.pointerId);
    setPreview(null);
  };
  const cancel = () => {
    if (!gesture.current) return false;
    suppressClick.current = true;
    clear();
    return true;
  };
  useLayoutEffect(() => {
    cancel();
    const blur = () => { cancel(); };
    const visibility = () => { if (document.hidden) cancel(); };
    window.addEventListener('blur', blur);
    document.addEventListener('visibilitychange', visibility);
    return () => { cancel(); window.removeEventListener('blur', blur); document.removeEventListener('visibilitychange', visibility); };
  }, [options.enabled, options.revision]);
  const autoScroll = () => {
    frame.current = 0;
    const current = gesture.current, element = latest.current.scroller.current;
    if (!current?.moved || !element) return;
    const bounds = element.getBoundingClientRect();
    const y = current.client.y - bounds.top;
    const delta = y < 24 ? -Math.min(18, 24 - y) : y > element.clientHeight - 24 ? Math.min(18, y - element.clientHeight + 24) : 0;
    const previous = element.scrollTop;
    if (delta) element.scrollTop += delta;
    if (previous !== element.scrollTop) setPreview(snapshot(current, element));
    frame.current = requestAnimationFrame(autoScroll);
  };
  const down = (event: PointerEvent<HTMLDivElement>) => {
    suppressClick.current = false;
    if (!latest.current.enabled || event.button !== 0 || !event.isPrimary || event.pointerType === 'touch' || event.altKey) return;
    if (event.target instanceof Element && event.target.closest('[role="option"]')) return;
    const element = event.currentTarget, bounds = element.getBoundingClientRect();
    if (event.clientX - bounds.left >= element.clientWidth || event.clientY - bounds.top >= element.clientHeight) return;
    latest.current.cancelSelection();
    clear();
    event.preventDefault();
    element.focus({ preventScroll: true });
    const client = { x: event.clientX, y: event.clientY };
    gesture.current = { pointerId: event.pointerId, origin: point(element, client), client,
      baseline: latest.current.selected, mode: event.shiftKey ? 'add' : event.ctrlKey || event.metaKey ? 'toggle' : 'replace', moved: false };
    element.setPointerCapture(event.pointerId);
  };
  const move = (event: PointerEvent<HTMLDivElement>) => {
    const current = gesture.current;
    if (!current || current.pointerId !== event.pointerId) return;
    current.client = { x: event.clientX, y: event.clientY };
    const end = point(event.currentTarget, current.client);
    if (!current.moved && Math.hypot(end.x - current.origin.x, end.y - current.origin.y) < 4) return;
    current.moved = true;
    event.preventDefault();
    setPreview(snapshot(current, event.currentTarget));
    if (!frame.current) frame.current = requestAnimationFrame(autoScroll);
  };
  const up = (event: PointerEvent<HTMLDivElement>) => {
    const current = gesture.current;
    if (!current || current.pointerId !== event.pointerId) return;
    current.client = { x: event.clientX, y: event.clientY };
    const selection = snapshot(current, event.currentTarget);
    suppressClick.current = current.moved;
    clear();
    if (current.moved) void latest.current.commit(selection);
    else if (current.mode === 'replace') void latest.current.commit({ ...selection, range: null });
  };
  return {
    active: preview !== null,
    rectangle: preview ? { left: Math.min(preview.start.x, preview.end.x), top: Math.min(preview.start.y, preview.end.y),
      width: Math.abs(preview.end.x - preview.start.x), height: Math.abs(preview.end.y - preview.start.y) } : null,
    selected: (index: number, id: string) => preview ? marqueeSelected(preview.baseline.has(id),
      preview.range !== null && index >= preview.range.first && index <= preview.range.last, preview.mode) : latest.current.selected.has(id),
    down, move, up, cancel,
    scroll: () => { const current = gesture.current, element = latest.current.scroller.current; if (current?.moved && element) setPreview(snapshot(current, element)); },
    click: () => { const suppressed = suppressClick.current; suppressClick.current = false; return suppressed; },
  };
}
