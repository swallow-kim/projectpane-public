import { useLayoutEffect, useRef, type RefObject } from 'react';
import { toFaultCode } from '../../shared/fault';
import { ROW_HEIGHT } from './fixed-list';
import type { ListingController } from './listing';
import type { ListPosition, PositionUpdate } from './navigation';
import { syntheticRow } from './synthetic';

type PositionOptions = {
  readonly visible: boolean;
  readonly listing: ListingController;
  readonly synthetic: boolean;
  readonly position: ListPosition;
  readonly onPosition: (update: PositionUpdate) => void;
  readonly scroller: RefObject<HTMLDivElement | null>;
};
export function useListPosition(options: PositionOptions) {
  const latest = useRef(options);
  latest.current = options;
  const restoring = useRef(true);
  const revision = options.synthetic ? 0 : options.listing.state.revision;
  const requestId = options.synthetic ? undefined : options.listing.requestId;
  const ready = options.visible && (options.synthetic || options.listing.state.status === 'complete' || options.listing.state.status === 'capped');
  useLayoutEffect(() => {
    restoring.current = true;
    if (!ready) return;
    let active = true;
    const { position, listing, synthetic, scroller } = latest.current;
    void (async () => {
      try {
        const focus = !synthetic && position.focusedId ? await listing.locate(position.focusedId) : position.focusedIndex;
        const scroll = !synthetic && position.scrollId ? await listing.locate(position.scrollId) : position.scrollIndex;
        if (!active || !restoring.current) return;
        const element = scroller.current;
        // A refresh can remove the anchor. Keep its prior offset and let the
        // browser clamp to the shorter list instead of jumping back to row zero.
        if (element) element.scrollTop = Math.max(0, scroll ?? position.scrollIndex) * ROW_HEIGHT + position.scrollTop % ROW_HEIGHT;
        const top = element?.scrollTop ?? 0;
        const scrollIndex = Math.floor(top / ROW_HEIGHT);
        const selected = new Set(position.selected);
        if (focus === null && position.focusedId) selected.delete(position.focusedId);
        latest.current.onPosition(current => ({ ...current, selected, focusedId: focus === null ? null : position.focusedId,
          focusedIndex: focus ?? 0, scrollIndex, scrollId: scroll === null ? listing.cache.row(scrollIndex)?.id ?? null : position.scrollId,
          scrollTop: top }));
      } catch (error) {
        const code = toFaultCode(error);
        if (code !== 'stale_request' && code !== 'cancelled') latest.current.onPosition(current => ({ ...current, scrollId: null }));
      } finally { if (active) restoring.current = false; }
    })();
    return () => { active = false; };
  }, [revision, ready, requestId]);

  const changed = () => {
    if (!latest.current.visible || restoring.current || latest.current.listing.state.viewPending) return;
    const { listing, synthetic, scroller, onPosition } = latest.current;
    if (!synthetic && listing.state.status !== 'complete' && listing.state.status !== 'capped') return;
    const top = scroller.current?.scrollTop ?? 0, index = Math.floor(top / ROW_HEIGHT);
    const entry = synthetic ? syntheticRow(index) : listing.cache.row(index);
    const scrollId = entry?.id ?? null;
    onPosition(current => top === current.scrollTop && scrollId === current.scrollId ? current : { ...current, scrollTop: top, scrollIndex: index, scrollId });
  };
  const interrupt = () => { restoring.current = false; };
  return { changed, interrupt };
}
