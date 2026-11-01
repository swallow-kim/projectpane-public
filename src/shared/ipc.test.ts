import { beforeEach, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import fixture from '../../tests/fixtures/ipc-contract.json';
import { parseIdentity } from './contracts';
import { IpcFault } from './fault';
import { transport } from './ipc';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn(), isTauri: () => true }));
beforeEach(() => vi.resetAllMocks());
const identity = parseIdentity(fixture.validReply.identity);

it('validates responses through the real adapter and preserves structured errors', async () => {
  vi.mocked(invoke).mockResolvedValueOnce(fixture.validReply);
  expect((await transport.start(identity, '/fixture')).batch.total).toBe(1);
  vi.mocked(invoke).mockRejectedValueOnce({ code: 'timeout' });
  await expect(transport.read(identity, null)).rejects.toEqual(new IpcFault('timeout'));
  vi.mocked(invoke).mockResolvedValueOnce({ ...fixture.validReply, batch: { ...fixture.validReply.batch, total: -1 } });
  await expect(transport.read(identity, 0)).rejects.toEqual(new IpcFault('protocol_error'));
});

it('never sends invalid requests and checks cancellation acknowledgement', async () => {
  await expect(transport.start(identity, '')).rejects.toEqual(new IpcFault('invalid_path'));
  await expect(transport.read(identity, -1)).rejects.toEqual(new IpcFault('invalid_request'));
  expect(invoke).not.toHaveBeenCalled();
  vi.mocked(invoke).mockResolvedValueOnce(null);
  await expect(transport.cancel(identity)).resolves.toBeUndefined();
  vi.mocked(invoke).mockResolvedValueOnce(true);
  await expect(transport.cancel(identity)).rejects.toEqual(new IpcFault('protocol_error'));
});
