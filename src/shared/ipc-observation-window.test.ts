import { afterEach, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
afterEach(() => vi.unstubAllGlobals());

it('exposes a frozen, non-enumerable control surface when normal IPC loads', async () => {
  // Given a browser global without observation installed.
  const browser = {};
  vi.stubGlobal('window', browser);
  // When the normal UI transport loads.
  await import('./ipc');
  // Then the harness can control observation without replacing the bridge.
  const descriptor = Object.getOwnPropertyDescriptor(browser, '__PROJECTPANE_IPC_OBSERVATION__');
  expect(descriptor).toMatchObject({ enumerable: false, writable: false, configurable: false });
  expect(Object.isFrozen(descriptor?.value)).toBe(true);
  expect(Object.keys(descriptor?.value ?? {})).toEqual(['enable', 'disable', 'snapshot']);
  expect(window.__PROJECTPANE_IPC_OBSERVATION__?.snapshot()).toMatchObject({ enabled: false, events: [], dropped: 0 });
});
