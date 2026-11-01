import { useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent } from 'react';
import { faultMessage, toFaultCode } from '../../shared/fault';
import type { ListingController } from './listing';
import type { ListPosition, PositionUpdate } from './navigation';
import { syntheticRow } from './synthetic';
import type { MarqueeSelection } from './marquee';

type SelectionOptions = {
  readonly synthetic: boolean;
  readonly listing: ListingController;
  readonly position: ListPosition;
  readonly onPosition: (update: PositionUpdate) => void;
  readonly reveal: (index: number) => void;
  readonly count: number;
  readonly onPending?: (pending: boolean) => void;
};
type Modifiers = Pick<KeyboardEvent | MouseEvent, 'ctrlKey' | 'shiftKey' | 'metaKey'>;

export function useSelection(options: SelectionOptions) {
  const latest = useRef(options);
  latest.current = options;
  const sequence = useRef(0);
  const requestedIndex = useRef<number | null>(null);
  const abort = useRef<AbortController | null>(null);
  const [pending, setPendingState] = useState(false);
  // Notify the parent in the initiating event/async completion, not in a
  // render effect that dispatches another update for each keyboard selection.
  const setPending = (value: boolean) => {
    setPendingState(value);
    latest.current.onPending?.(value);
  };
  const [error, setError] = useState<string | null>(null);
  useEffect(() => () => { sequence.current++; abort.current?.abort(); latest.current.onPending?.(false); }, []);
  const revision = options.synthetic ? 0 : options.listing.state.revision;
  useLayoutEffect(() => { requestedIndex.current = null; }, [revision]);
  useLayoutEffect(() => {
    if (requestedIndex.current === options.position.focusedIndex) requestedIndex.current = null;
  }, [options.position.focusedIndex, options.position.focusedId]);
  const focusIndex = () => requestedIndex.current ?? latest.current.position.focusedIndex;

  const range = async (first: number, last: number, signal: AbortSignal): Promise<ReadonlySet<string>> => {
    const { synthetic, listing } = latest.current;
    if (!synthetic) return listing.selectRange(first, last, signal);
    const start = Math.min(first, last), end = Math.max(first, last);
    return new Set(Array.from({ length: end - start + 1 }, (_, index) => syntheticRow(start + index).id));
  };
  const select = async (index: number, modifiers: Modifiers, focusOnly = false) => {
    const { ctrlKey, metaKey, shiftKey } = modifiers;
    const token = ++sequence.current;
    abort.current?.abort();
    const controller = new AbortController();
    abort.current = controller;
    const { synthetic, listing, position, count, reveal } = latest.current;
    if (!count) { requestedIndex.current = null; setPending(false); return; }
    const target = Math.max(0, Math.min(count - 1, index));
    requestedIndex.current = target;
    reveal(target);
    setError(null);
    setPending(true);
    try {
      // Cached clicks must commit their intent before the next input. Awaiting
      // an already cached row lets the next sequence discard a Ctrl toggle.
      const cached = synthetic ? syntheticRow(target) : listing.cachedEntryAt(target);
      const entry = cached ?? await listing.entryAt(target, true);
      if (!entry || token !== sequence.current) return;
      let rangeValues: ReadonlySet<string> | null = null;
      if (!focusOnly && shiftKey) {
        const anchor = position.anchorId;
        const anchorIndex = anchor ? synthetic ? Number(anchor.replace('synthetic-', '')) : await listing.locate(anchor) : null;
        rangeValues = await range(anchorIndex ?? position.focusedIndex, target, controller.signal);
      }
      if (token !== sequence.current) return;
      latest.current.onPosition(current => {
        // React may batch several inputs. Reduce each toggle against the latest
        // queued selection instead of the render captured before those inputs.
        let selected = current.selected;
        if (!focusOnly) {
          if (rangeValues) selected = ctrlKey || metaKey ? new Set([...selected, ...rangeValues]) : rangeValues;
          else if (ctrlKey || metaKey) {
            const values = new Set(selected);
            if (values.has(entry.id)) values.delete(entry.id); else values.add(entry.id);
            selected = values;
          } else selected = new Set([entry.id]);
        }
        return { ...current, selected, focusedIndex: target, focusedId: entry.id,
          anchorId: shiftKey ? position.anchorId ?? position.focusedId ?? entry.id : entry.id };
      });
    } catch (cause) {
      const code = toFaultCode(cause);
      if (token === sequence.current) requestedIndex.current = null;
      if (token === sequence.current && code !== 'stale_request' && code !== 'cancelled') setError(faultMessage(code));
    } finally { if (token === sequence.current) setPending(false); }
  };
  const all = async () => {
    requestedIndex.current = null;
    const token = ++sequence.current;
    abort.current?.abort();
    const controller = new AbortController();
    abort.current = controller;
    setError(null); setPending(true);
    try {
      const selected = latest.current.count ? await range(0, latest.current.count - 1, controller.signal) : new Set<string>();
      if (token === sequence.current) latest.current.onPosition(current => ({ ...current, selected }));
    } catch (cause) {
      const code = toFaultCode(cause);
      if (token === sequence.current && code !== 'stale_request' && code !== 'cancelled') setError(faultMessage(code));
    } finally { if (token === sequence.current) setPending(false); }
  };
  const cancel = () => { sequence.current++; requestedIndex.current = null; abort.current?.abort(); setPending(false); };
  const marquee = async ({ range: bounds, baseline, mode }: MarqueeSelection) => {
    const token = ++sequence.current;
    abort.current?.abort();
    const controller = new AbortController(); abort.current = controller;
    const startRevision = latest.current.listing.state.revision;
    requestedIndex.current = null; setError(null); setPending(true);
    try {
      const values = bounds ? await range(bounds.first, bounds.last, controller.signal) : new Set<string>();
      if (token !== sequence.current || (!latest.current.synthetic && latest.current.listing.state.revision !== startRevision)) return;
      const selected = mode === 'replace' ? new Set(values) : new Set(baseline);
      let focusedId: string | null = null;
      for (const id of values) {
        focusedId = id;
        if (mode === 'toggle' && selected.has(id)) selected.delete(id);
        else selected.add(id);
      }
      latest.current.onPosition(current => ({ ...current, selected,
        focusedId: focusedId ?? current.focusedId, focusedIndex: bounds?.last ?? current.focusedIndex,
        anchorId: focusedId ?? current.anchorId }));
    } catch (cause) {
      const code = toFaultCode(cause);
      if (token === sequence.current && code !== 'stale_request' && code !== 'cancelled') setError(faultMessage(code));
    } finally { if (token === sequence.current) setPending(false); }
  };
  return { select, all, marquee, cancel, pending, error, focusIndex };
}
