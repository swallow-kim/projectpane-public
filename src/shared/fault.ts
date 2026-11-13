import { t } from './i18n';
import { z } from 'zod';

export const nativeFaultCodes = ['destination_streams_unsupported', 'unsupported_provider', 'unsafe_reparse_point', 'invalid_request', 'invalid_path', 'invalid_state', 'not_found', 'access_denied', 'io_error', 'resource_limit', 'payload_limit', 'protocol_error', 'worker_unavailable', 'busy', 'destination_exists', 'cancelled', 'timeout', 'stale_request'] as const;
export const nativeFaultSchema = z.strictObject({ code: z.enum(nativeFaultCodes) });
export type FaultCode = typeof nativeFaultCodes[number] | 'ipc_error';

const messages = (): Readonly<Record<FaultCode, string>> => ({
  destination_streams_unsupported: t("대상 저장소는 이 항목의 추가 데이터 스트림(ADS)을 보존할 수 없어 복사를 중단했습니다. 해당 항목은 새로 만들지 않았습니다. ADS를 지원하는 대상 저장소를 선택하세요.", "Copy stopped because the destination cannot preserve this item’s alternate data streams (ADS). The item was not created. Choose a destination that supports ADS."),
  unsupported_provider: t("이 저장소는 안전한 복사에 필요한 파일 식별 또는 생성 기능을 지원하지 않습니다.", "This storage does not support the file identification or creation features required for safe copying."),
  unsafe_reparse_point: t("선택한 항목이나 상위 경로에 심볼릭 링크 또는 재분석 지점이 있어 처리하지 않았습니다. 링크를 따라가지 않습니다. 링크가 없는 실제 원본 경로와 대상 폴더를 직접 선택하세요.", "The item or a parent path contains a symbolic link or reparse point. Nothing was processed and links are not followed. Select the actual source and destination paths without links."),
  destination_exists: t("대상에 같은 이름의 항목이 있습니다. 덮어쓰지 않았습니다.", "An item with the same name exists at the destination. Nothing was overwritten."),
  invalid_request: t("조회 요청을 처리할 수 없습니다.", "The listing request could not be processed."),
  invalid_path: t("올바른 절대 경로를 입력하세요.", "Enter a valid absolute path."),
  invalid_state: t("조회 상태를 확인할 수 없습니다. 다시 시도하세요.", "The listing state could not be verified. Try again."),
  not_found: t("폴더를 찾을 수 없습니다. 경로를 확인하세요.", "The folder was not found. Check the path."),
  access_denied: t("이 폴더를 읽을 권한이 없습니다.", "You do not have permission to read this folder."),
  io_error: t("폴더를 읽는 중 오류가 발생했습니다.", "An error occurred while reading the folder."),
  resource_limit: t("조회에 사용할 수 있는 자원 한도에 도달했습니다.", "The resource limit for this listing was reached."),
  payload_limit: t("조회 결과가 전달 가능한 크기를 초과했습니다.", "The listing exceeds the maximum response size."),
  protocol_error: t("조회 응답을 확인할 수 없습니다. 다시 시도하세요.", "The listing response could not be verified. Try again."),
  worker_unavailable: t("폴더 조회 프로세스를 시작할 수 없습니다.", "The folder listing process could not be started."),
  busy: t("이전 조회를 정리하고 있습니다. 잠시 후 다시 시도하세요.", "The previous listing is being cleaned up. Try again shortly."),
  cancelled: t("조회를 취소했습니다.", "The listing was cancelled."),
  timeout: t("폴더가 제한 시간 안에 응답하지 않았습니다.", "The folder did not respond within the time limit."),
  stale_request: t("이미 종료된 조회의 응답입니다.", "This response belongs to a listing that has already ended."),
  ipc_error: t("앱과 연결하지 못했습니다. 다시 시도하세요.", "Could not connect to the app. Try again."),
});

export function faultMessage(code: FaultCode): string { return messages()[code]; }
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
