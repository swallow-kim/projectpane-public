import { t } from '../../shared/i18n';
import { toFaultCode } from '../../shared/fault';

// Only pre-execution capability/limit failures permit changing the requested
// operation. Stale selections, broken IPC, cancellation and uncertain results
// must never be retried as copies.
export function canFallbackToCopy(error: unknown): boolean {
  return ['invalid_request', 'unsupported_provider', 'resource_limit', 'payload_limit', 'access_denied'].includes(toFaultCode(error));
}
export const CUT_COPY_NOTICE = '이 조건에서는 안전한 이동을 지원하지 않아 복사로 전환했습니다. 원본은 그대로 남습니다. 대상 폴더를 확인한 뒤 복사를 누르세요.';

export function copyFallbackNotice(error: unknown): string {
  const code = toFaultCode(error);
  const reason = code === 'payload_limit' || code === 'resource_limit'
    ? t("이동 작업의 항목 수·경로 길이·처리 한도를 넘었습니다. ", "The move exceeds an item-count, path-length, or processing limit. ")
    : code === 'access_denied' ? t("이동에 필요한 권한을 얻지 못했습니다. ", "The permissions required to move these items were not granted. ")
    : code === 'unsupported_provider' ? t("이 저장소에서는 안전한 이동을 지원하지 않습니다. ", "Safe moves are not supported on this storage. ")
    : t("선택한 항목과 대상 경로의 조합으로는 이동을 준비할 수 없습니다. ", "A move cannot be prepared for these items and this destination. ");
  return reason + t(CUT_COPY_NOTICE, 'A safe move is not supported under these conditions, so this operation was changed to a copy. The originals will remain. Check the destination folder, then click Copy.');
}
