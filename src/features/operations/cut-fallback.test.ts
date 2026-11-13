import { describe, expect, it } from 'vitest';
import { canFallbackToCopy } from './cut-fallback';
describe('pre-execution cut fallback', () => {
  it.each(['invalid_request', 'unsupported_provider', 'resource_limit', 'payload_limit', 'access_denied'])('allows %s before dispatch', code => {
    expect(canFallbackToCopy({ code })).toBe(true);
  });
  it.each(['stale_request', 'io_error', 'ipc_error', 'protocol_error', 'timeout', 'cancelled', 'not_found', 'busy', 'unsafe_reparse_point'])('does not reinterpret %s as a copy', code => {
    expect(canFallbackToCopy({ code })).toBe(false);
  });
});
