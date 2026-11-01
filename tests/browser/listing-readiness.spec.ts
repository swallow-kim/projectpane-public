import { expect, test } from '@playwright/test';
import type { Identity } from '../../src/shared/contracts';

test('a native refresh updates rendered readiness before another input task can run', async ({ page }) => {
  await page.addInitScript(() => {
    let starts = 0;
    Object.assign(window, { isTauri: true, __TAURI_INTERNALS__: { invoke: async (command: string, args: { identity: Identity }) => {
      if (command === 'cancel_listing') return;
      if (command === 'start_listing' && ++starts > 1) await new Promise(resolve => Reflect.set(window, 'releaseReadinessRefresh', resolve));
      return { identity: args.identity, batch: { revision: 1, offset: 0,
        entries: [{ id: 'fixture-entry', name: 'fixture.txt', kind: 'file', modifiedAtMs: 0, sizeBytes: 0 }],
        total: 1, complete: true, capped: false, snapshotBytes: 128, capReason: null } };
    } } });
  });
  await page.route('**/readiness-probe', route => route.fulfill({ contentType: 'text/html', body:
    '<div id="readiness-root"></div><script type="module" src="/tests/browser/listing-readiness-fixture.tsx"></script>' }));
  await page.goto('/readiness-probe');
  const state = page.locator('#listing-readiness');
  await expect(state).toHaveText('complete');
  const observation = await page.evaluate(() => Reflect.get(window, 'beginReadinessRefresh')());
  expect(observation).toEqual({ internal: 'loading', rendered: 'true' });
  await page.evaluate(() => Reflect.get(window, 'releaseReadinessRefresh')());
  await expect(state).toHaveText('complete');
});
