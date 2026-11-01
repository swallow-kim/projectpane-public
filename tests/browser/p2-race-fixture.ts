import { expect, type Page } from '@playwright/test';
import type { Entry, Identity } from '../../src/shared/contracts';
import type { ListingView } from '../../src/shared/listing-view';

type AuditCall = { readonly command: string; readonly pane: number; readonly offset?: number | null; readonly entryId?: string };
declare global {
  interface Window {
    auditCalls: AuditCall[];
    auditWaiting: string[];
    auditHold: (key: string) => void;
    auditRelease: (key: string) => void;
  }
}

export async function installRaceListing(page: Page, count = 192) {
  await page.addInitScript(total => {
    const entries: Entry[] = Array.from({ length: total }, (_, index) => ({
      id: `item-${index}`, name: `${String(index).padStart(3, '0')}-${index % 2 ? 'other' : 'keep'}.txt`,
      kind: 'file', modifiedAtMs: index * 1000, sizeBytes: index,
    }));
    const sessions = new Map<number, { revision: number; entries: Entry[] }>();
    const gates = new Map<string, { readonly promise: Promise<void>; readonly release: () => void }>();
    window.auditCalls = [];
    window.auditWaiting = [];
    window.auditHold = key => {
      let release = () => {};
      const promise = new Promise<void>(resolve => { release = resolve; });
      gates.set(key, { promise, release });
    };
    window.auditRelease = key => { gates.get(key)?.release(); gates.delete(key); };
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: {
      readonly identity: Identity; readonly offset?: number | null; readonly entryId?: string; readonly view?: ListingView;
    }) => {
      if (command === 'startup_config') return { folders: ['/fixture/a', '/fixture/b', '/fixture/c', '/fixture/d'], error: null, warnings: [] };
      const pane = args.identity.pane;
      window.auditCalls.push({ command, pane, offset: args.offset, entryId: args.entryId });
      if (command === 'cancel_listing') return;
      if (command === 'start_listing') sessions.set(pane, { revision: 1, entries: entries.slice() });
      const session = sessions.get(pane);
      if (!session) throw new Error('Missing fixture session');
      const offset = command === 'read_listing' ? args.offset ?? 0 : 0;
      const batch = { revision: session.revision, offset, entries: session.entries.slice(offset, offset + 64),
        total: session.entries.length, complete: true, capped: false, snapshotBytes: 32768, capReason: null };
      const key = `${pane}:${command}:${offset}`;
      const gate = gates.get(key);
      if (gate) { window.auditWaiting.push(key); await gate.promise; }
      if (command === 'locate_entry') return { identity: args.identity, revision: session.revision,
        index: session.entries.some(entry => entry.id === args.entryId) ? session.entries.findIndex(entry => entry.id === args.entryId) : null };
      if (command === 'set_listing_view') {
        session.entries = entries.filter(entry => entry.name.includes(args.view?.filter ?? ''));
        session.revision++;
        return { identity: args.identity, batch: { ...batch, revision: session.revision, total: session.entries.length, entries: session.entries.slice(0, 64) } };
      }
      return { identity: args.identity, batch };
    } } });
  }, count);
  await page.goto('/');
  for (const pane of [1, 3]) await expect(page.getByRole('region', { name: `패널 ${pane}`, exact: true })
    .locator('footer [role=status]')).toContainText(`조회 완료 · ${count.toLocaleString()}개`);
}
