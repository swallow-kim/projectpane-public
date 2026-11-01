import { useEffect, useState, useSyncExternalStore } from 'react';
import { desktop, transport } from '../../shared/ipc';
import { ListingController } from './listing';

export function useListing(pane: number, initialPath?: string) {
  const [store] = useState(() => {
    const listeners = new Set<() => void>();
    const listing = new ListingController(pane, transport, () => { for (const listener of listeners) listener(); });
    return {
      listing,
      subscribe(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; },
      snapshot: () => listing.state,
    };
  });
  // IPC/watch callbacks mutate an external store. A batched reducer notification
  // leaves the rendered busy state behind the state read by input handlers.
  useSyncExternalStore(store.subscribe, store.snapshot);
  const { listing } = store;
  useEffect(() => {
    if (desktop && initialPath) listing.navigate(initialPath);
    return () => listing.dispose();
  }, [initialPath, listing]);
  return listing;
}
