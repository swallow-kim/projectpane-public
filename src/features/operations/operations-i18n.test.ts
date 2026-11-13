import { afterEach, describe, expect, it } from 'vitest';
import { setLanguage } from '../../shared/i18n';
import { fileActionText, operationStatusText } from '../../shared/operations';
import { operationError } from './operation-client';
import { copyFallbackNotice } from './cut-fallback';
import { nativeResultCode, nativeResultReason } from './native-result';

afterEach(() => setLanguage('ko'));
describe('file operation localization', () => {
  it('evaluates action and status maps in the current language without reimporting', () => {
    setLanguage('ko');
    expect(fileActionText.rename).toBe('이름 변경');
    expect(operationStatusText.succeeded).toBe('완료');
    setLanguage('en');
    expect(fileActionText.rename).toBe('Rename');
    expect(operationStatusText.succeeded).toBe('Completed');
    expect(Object.values(fileActionText).every(value => !/[가-힣]/.test(value))).toBe(true);
    expect(Object.values(operationStatusText).every(value => !/[가-힣]/.test(value))).toBe(true);
    setLanguage('ko');
    expect(fileActionText.recycle).toBe('휴지통');
  });
  it.each(['destination_streams_unsupported', 'unsupported_provider', 'unsafe_reparse_point', 'destination_exists', 'busy', 'stale_request', 'invalid_request', 'access_denied', 'not_found', 'worker_unavailable', 'invalid_state', 'payload_limit', 'resource_limit', 'io_error'])('localizes error %s', code => {
    setLanguage('en');
    const message = operationError({ code });
    expect(message.length).toBeGreaterThan(10);
    expect(message).not.toMatch(/[가-힣]/);
    setLanguage('ko');
    expect(operationError({ code })).toMatch(/[가-힣]/);
  });
  it.each(['copy_skip_existing', 'copy_as', 'recycle', 'compress', 'extract', 'move'] as const)('localizes action-specific guidance for %s', kind => {
    setLanguage('en');
    expect(operationError({ code: 'invalid_request' }, kind)).not.toMatch(/[가-힣]/);
  });
  it.each(['payload_limit', 'resource_limit', 'access_denied', 'unsupported_provider', 'invalid_request'])('keeps fallback safety guidance in English for %s', code => {
    setLanguage('en');
    expect(copyFallbackNotice({ code })).not.toMatch(/[가-힣]/);
    expect(copyFallbackNotice({ code })).toContain('The originals will remain');
    setLanguage('ko');
    expect(copyFallbackNotice({ code })).toContain('원본은 그대로 남습니다');
  });
  it.each([0x80070070, 0x80070027, 0x80070005, 0x80070020, 0x80070021, 0x80270027, 0x80270028, 0x80270021, 0x80270022, 0x80270023, 0x80270024, 0x80270032, 0x80270033, 0x8027000d, 0x8027003f])('localizes HRESULT %s without changing the diagnostic code', code => {
    const before = nativeResultCode(code);
    setLanguage('en');
    expect(nativeResultReason(code)).not.toMatch(/[가-힣]/);
    expect(nativeResultReason(code)).not.toBeNull();
    expect(nativeResultCode(code)).toBe(before);
    setLanguage('ko');
    expect(nativeResultReason(code)).toMatch(/[가-힣]/);
  });
});
