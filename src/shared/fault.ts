import { z } from 'zod';

export const nativeFaultCodes = ['destination_streams_unsupported', 'unsupported_provider', 'unsafe_reparse_point', 'invalid_request', 'invalid_path', 'invalid_state', 'not_found', 'access_denied', 'io_error', 'resource_limit', 'payload_limit', 'protocol_error', 'worker_unavailable', 'busy', 'destination_exists', 'cancelled', 'timeout', 'stale_request'] as const;
export const nativeFaultSchema = z.strictObject({ code: z.enum(nativeFaultCodes) });
export type FaultCode = typeof nativeFaultCodes[number] | 'ipc_error';

const messages: Readonly<Record<FaultCode, string>> = {
  destination_streams_unsupported: '대상 저장소는 이 항목의 추가 데이터 스트림(ADS)을 보존할 수 없어 복사를 중단했습니다. 해당 항목은 새로 만들지 않았습니다. ADS를 지원하는 대상 저장소를 선택하세요.',
  unsupported_provider: '이 저장소는 안전한 복사에 필요한 파일 식별 또는 생성 기능을 지원하지 않습니다.',
  unsafe_reparse_point: '선택한 항목이나 상위 경로에 심볼릭 링크 또는 재분석 지점이 있어 처리하지 않았습니다. 링크를 따라가지 않습니다. 링크가 없는 실제 원본 경로와 대상 폴더를 직접 선택하세요.',
  destination_exists: '대상에 같은 이름의 항목이 있습니다. 덮어쓰지 않았습니다.',
  invalid_request: '조회 요청을 처리할 수 없습니다.',
  invalid_path: '올바른 절대 경로를 입력하세요.',
  invalid_state: '조회 상태를 확인할 수 없습니다. 다시 시도하세요.',
  not_found: '폴더를 찾을 수 없습니다. 경로를 확인하세요.',
  access_denied: '이 폴더를 읽을 권한이 없습니다.',
  io_error: '폴더를 읽는 중 오류가 발생했습니다.',
  resource_limit: '조회에 사용할 수 있는 자원 한도에 도달했습니다.',
  payload_limit: '조회 결과가 전달 가능한 크기를 초과했습니다.',
  protocol_error: '조회 응답을 확인할 수 없습니다. 다시 시도하세요.',
  worker_unavailable: '폴더 조회 프로세스를 시작할 수 없습니다.',
  busy: '이전 조회를 정리하고 있습니다. 잠시 후 다시 시도하세요.',
  cancelled: '조회를 취소했습니다.',
  timeout: '폴더가 제한 시간 안에 응답하지 않았습니다.',
  stale_request: '이미 종료된 조회의 응답입니다.',
  ipc_error: '앱과 연결하지 못했습니다. 다시 시도하세요.',
};

export function faultMessage(code: FaultCode): string { return messages[code]; }
export function isRetryable(code: FaultCode): boolean {
  return code !== 'invalid_request' && code !== 'invalid_path' && code !== 'cancelled' && code !== 'stale_request';
}

export class IpcFault extends Error {
  constructor(readonly code: FaultCode) {
    super(faultMessage(code));
    this.name = 'IpcFault';
  }
}

export function toFaultCode(error: unknown): FaultCode {
  if (error instanceof IpcFault) return error.code;
  const parsed = nativeFaultSchema.safeParse(error);
  return parsed.success ? parsed.data.code : 'ipc_error';
}
