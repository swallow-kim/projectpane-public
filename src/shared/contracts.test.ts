import { describe, expect, it } from 'vitest';
import fixture from '../../tests/fixtures/ipc-contract.json';
import { MAX_ENTRIES, MAX_FRAME, MAX_SNAPSHOT_BYTES, parseIdentity, parseOffset, parsePath, parseReply, startupSchema } from './contracts';
import { IpcFault, toFaultCode } from './fault';

const identity = parseIdentity(fixture.validReply.identity);
const reply = fixture.validReply;

describe('native IPC trust boundary', () => {
  it('accepts the shared wire fixture and preserves epoch zero', () => {
    const parsed = parseReply(reply, identity);
    expect(parsed.batch.entries[0]?.modifiedAtMs).toBe(0);
    expect(parsed.batch.capReason).toBeNull();
  });
  it('rejects shared malformed fields and missing diagnostics', () => {
    for (const mutation of fixture.invalidReplyMutations) {
      expect(() => parseReply({ ...reply, batch: { ...reply.batch, [mutation.field]: mutation.value } }, identity)).toThrow(IpcFault);
    }
    const { snapshotBytes, ...withoutSnapshot } = reply.batch;
    const { capReason, ...withoutReason } = reply.batch;
    expect(snapshotBytes).toBeGreaterThan(0);
    expect(capReason).toBeNull();
    for (const batch of [withoutSnapshot, withoutReason]) expect(() => parseReply({ ...reply, batch }, identity)).toThrow(IpcFault);
  });
  it('rejects wrong identity and unsafe request ranges before dispatch', () => {
    expect(() => parseReply(reply, parseIdentity({ ...identity, generation: 2 }))).toThrow(new IpcFault('stale_request'));
    for (const generation of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1]) expect(() => parseIdentity({ ...identity, generation })).toThrow(IpcFault);
    expect(() => parseIdentity({ ...identity, pane: 4 })).toThrow(IpcFault);
    expect(() => parseIdentity({ ...identity, requestId: 'path/private' })).toThrow(IpcFault);
    for (const offset of [-1, 0.5, MAX_ENTRIES + 1]) expect(() => parseOffset(offset)).toThrow(IpcFault);
  });
  it('bounds UTF8 paths and rejects NUL without interpreting OS paths in the UI', () => {
    expect(parsePath('C:\\fixture')).toBe('C:\\fixture');
    expect(parsePath('/fixture')).toBe('/fixture');
    for (const path of ['', 'C:\\bad\0name', `C:\\${'한'.repeat(11_000)}`]) expect(() => parsePath(path)).toThrow(new IpcFault('invalid_path'));
  });
  it('rejects excess page/frame allocation, duplicate rows and invalid timestamps', () => {
    const entries = Array.from({ length: 65 }, (_, index) => ({ ...reply.batch.entries[0], id: `entry-${index}` }));
    expect(() => parseReply({ identity, batch: { ...reply.batch, total: 65, entries } }, identity)).toThrow(IpcFault);
    expect(() => parseReply({ identity, batch: { ...reply.batch, total: 2, entries: [reply.batch.entries[0], reply.batch.entries[0]] } }, identity)).toThrow(IpcFault);
    for (const modifiedAtMs of [NaN, 8_640_000_000_000_001, '2026-10-04']) {
      expect(() => parseReply({ identity, batch: { ...reply.batch, entries: [{ ...reply.batch.entries[0], modifiedAtMs }] } }, identity)).toThrow(IpcFault);
    }
    const large = { identity, batch: { ...reply.batch, total: 64, entries: entries.slice(0, 64).map(entry => ({ ...entry, name: 'a'.repeat(5000) })) } };
    expect(JSON.stringify(large).length).toBeGreaterThan(MAX_FRAME);
    expect(() => parseReply(large, identity)).toThrow(IpcFault);
  });
  it('accepts explicit capped output without raising limits', () => {
    const batch = { ...reply.batch, total: MAX_ENTRIES, complete: true, capped: true, capReason: 'entry_limit', snapshotBytes: MAX_SNAPSHOT_BYTES };
    expect(parseReply({ identity, batch }, identity).batch.capped).toBe(true);
  });
  it('keeps untrusted rejection text and unknown fault codes out of user messages', () => {
    expect(toFaultCode({ code: 'not_found' })).toBe('not_found');
    for (const error of ['private-input', { code: 'private-input' }, { code: 'not_found', path: 'private-input' }]) {
      const fault = new IpcFault(toFaultCode(error));
      expect(fault.code).toBe('ipc_error');
      expect(fault.message).not.toContain('private-input');
    }
  });
  it('accepts only atomic bounded startup configuration and known warnings', () => {
    expect(startupSchema.safeParse({ folders: ['/fixture'], error: null, warnings: ['state_invalid'] }).success).toBe(true);
    expect(startupSchema.safeParse({ folders: ['/fixture'], error: 'invalid_startup_args', warnings: [] }).success).toBe(false);
    expect(startupSchema.safeParse({ folders: [], error: null, warnings: ['private-input'] }).success).toBe(false);
  });
});
