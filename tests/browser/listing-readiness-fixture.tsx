import { createRoot } from 'react-dom/client';
import { useListing } from '../../src/features/explorer/use-listing';

function ListingReadinessProbe() {
  const listing = useListing(0, '/fixture');
  Reflect.set(window, 'beginReadinessRefresh', async () => {
    listing.refresh('/fixture', 'pane-0-real');
    await Promise.resolve();
    return { internal: listing.state.status, rendered: document.querySelector('#listing-readiness')?.getAttribute('aria-busy') };
  });
  return <div id="listing-readiness" aria-busy={listing.state.status === 'loading'}>{listing.state.status}</div>;
}

createRoot(document.getElementById('readiness-root')!).render(<ListingReadinessProbe />);
