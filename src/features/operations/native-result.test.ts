import { expect, it } from 'vitest';
import { nativeResultCode, nativeResultReason } from './native-result';

it('explains Win32 disk-full, access and sharing failures while retaining signed HRESULT bits', () => {
  for (const code of [0x80070070, 0x80070027]) {
    expect(nativeResultReason(code | 0)).toContain('여유 공간이 부족');
    expect(nativeResultCode(code | 0)).toBe(`0x${code.toString(16)}`);
  }
  expect(nativeResultReason(0x80070005 | 0)).toContain('접근할 권한이 없습니다');
  for (const code of [0x80070020, 0x80070021]) expect(nativeResultReason(code | 0)).toContain('다른 앱에서 사용 중');
});

it('explains the observed signed native sharing HRESULT without losing its bits', () => {
  expect(nativeResultCode(-2144927705)).toBe('0x80270027');
  expect(nativeResultReason(-2144927705)).toContain('원본 파일을 다른 앱이 사용 중입니다');
});

it('keeps unknown codes visible without inventing a failure reason or cancellation', () => {
  for (const code of [0, 0x270005, -2147483648, -2144927744]) {
    expect(nativeResultReason(code)).toBeNull();
    expect(nativeResultCode(code)).toMatch(/^0x[0-9a-f]{8}$/);
  }
});
