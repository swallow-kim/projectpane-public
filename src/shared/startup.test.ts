import { beforeEach, describe, expect, it, vi } from 'vitest';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { loadStartupConfig } from './startup';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: vi.fn() }));
beforeEach(() => vi.clearAllMocks());

describe('read-only startup options', () => {
  it('keeps browser startup synthetic without making native calls', async () => {
    vi.mocked(isTauri).mockReturnValue(false);
    expect(await loadStartupConfig()).toEqual({ folders: [], error: null, warnings: [] });
    expect(invoke).not.toHaveBeenCalled();
  });
  it('loads the native validated folder order', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockResolvedValue({ folders: ['/first', '/second'], error: null, warnings: [] });
    expect(await loadStartupConfig()).toEqual({ folders: ['/first', '/second'], error: null, warnings: [] });
    expect(invoke).toHaveBeenCalledWith('startup_config');
  });
  it('surfaces startup transport failure without inventing folders', async () => {
    vi.mocked(isTauri).mockReturnValue(true);
    vi.mocked(invoke).mockRejectedValue(new Error('unavailable'));
    expect(await loadStartupConfig()).toEqual({ folders: [], error: 'startup_unavailable', warnings: [] });
  });
});
