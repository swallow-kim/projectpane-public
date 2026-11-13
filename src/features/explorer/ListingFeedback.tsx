import { t, useLanguage } from '../../shared/i18n';
import { faultMessage, isRetryable } from '../../shared/fault';
import type { ListingState } from './listing';

export function listingStatus(state: ListingState): string {
  if (state.refreshing) return t("새로 고치는 중…", "Refreshing…");
  if (state.viewPending) return t("정렬·필터 적용 중…", "Applying sort and filter…");
  switch (state.status) {
    case 'idle': return t("폴더 경로를 입력하세요", "Enter a folder path");
    case 'loading': return state.total > 0 ? t("읽는 중… · 일부 목록 표시", "Reading… · Showing partial results") : t("읽는 중…", "Reading…");
    case 'complete': return state.total === 0 ? t("빈 폴더", "Empty folder") : t("조회 완료", "Loaded");
    case 'capped': return t("목록 상한 도달 · 일부만 표시", "Listing limit reached · Showing partial results");
    case 'error': return faultMessage(state.error);
  }
}

export function ListingFeedback({ state, retry }: {
  readonly state: ListingState;
  readonly retry: () => void;
}) {
  useLanguage();
  if (state.refreshing) return null;
  if (state.status === 'complete' && state.total > 0 && !state.viewPending) return null;
  return <div className="listing-feedback" role={state.status === 'error' ? 'alert' : undefined}>
    <span>{listingStatus(state)}</span>
    {state.status === 'error' && isRetryable(state.error) &&
      <button type="button" onClick={retry}>{t("다시 시도", "Retry")}</button>}
  </div>;
}
