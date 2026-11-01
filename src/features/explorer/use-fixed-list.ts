import { useLayoutEffect, useRef, useState, type RefObject } from 'react';
import { revealRow, ROW_HEIGHT, visibleRange } from './fixed-list';

export function useFixedList(count: number, scroller: RefObject<HTMLDivElement | null>) {
  const [viewport, setViewport] = useState({ top: 0, height: 0 });
  const measured = useRef({ top: 0, height: 0, scrollHeight: 0 });
  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) return;
    let frame = 0;
    const measure = (resized = false) => {
      frame = 0;
      const top = element.scrollTop, height = element.clientHeight;
      // A scheduled scroll frame may run before ResizeObserver. Keep its
      // previous height until the resize callback has applied anchoring.
      measured.current = { top, height: resized ? height : measured.current.height, scrollHeight: element.scrollHeight };
      setViewport(previous => previous.top === top && previous.height === height ? previous : { top, height });
    };
    const schedule = () => { if (!frame) frame = requestAnimationFrame(() => measure()); };
    const observer = new ResizeObserver(() => {
      const previous = measured.current;
      // Late toolbar/status layout can shrink the viewport after End has
      // revealed the last row. Preserve that bottom position, but never
      // override a subsequent user scroll or a changed listing length.
      if (previous.height > element.clientHeight && previous.scrollHeight === element.scrollHeight
        && Math.abs(element.scrollTop - previous.top) <= 1
        && previous.top + previous.height >= previous.scrollHeight - 1) {
        element.scrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
      }
      measure(true);
    });
    observer.observe(element);
    element.addEventListener('scroll', schedule, { passive: true });
    measure(true);
    return () => { observer.disconnect(); element.removeEventListener('scroll', schedule); cancelAnimationFrame(frame); };
  }, [scroller]);
  const range = visibleRange(count, viewport.top, viewport.height);
  const rows = Array.from({ length: range.end - range.first }, (_, offset) => {
    const index = range.first + offset;
    return { index, key: index, start: index * ROW_HEIGHT, size: ROW_HEIGHT };
  });
  return {
    rows, totalSize: count * ROW_HEIGHT,
    scrollToIndex(index: number) {
      const element = scroller.current;
      if (!element || !count) return;
      element.scrollTop = revealRow(Math.max(0, Math.min(count - 1, index)), element.scrollTop, element.clientHeight);
      measured.current = { top: element.scrollTop, height: element.clientHeight, scrollHeight: element.scrollHeight };
      setViewport({ top: element.scrollTop, height: element.clientHeight });
    },
  };
}
