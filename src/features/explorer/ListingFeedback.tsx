import { faultMessage, isRetryable } from '../../shared/fault';
import type { ListingState } from './listing';

export function listingStatus(state: ListingState): string {
  if (state.refreshing) return '새로 고치는 중…';
  if (state.viewPending) return '정렬·필터 적용 중…';
  switch (state.status) {
    case 'idle': return '폴더 경로를 입력하세요';
    case 'loading': return state.total > 0 ? '읽는 중… · 일부 목록 표시' : '읽는 중…';
    case 'complete': return state.total === 0 ? '빈 폴더' : '조회 완료';
    case 'capped': return '목록 상한 도달 · 일부만 표시';
    case 'error': return faultMessage(state.error);
  }
}

export function ListingFeedback({ state, retry }: {
  readonly state: ListingState;
  readonly retry: () => void;
}) {
  if (state.refreshing) return null;
  if (state.status === 'complete' && state.total > 0 && !state.viewPending) return null;
  return <div className="listing-feedback" role={state.status === 'error' ? 'alert' : undefined}>
    <span>{listingStatus(state)}</span>
    {state.status === 'error' && isRetryable(state.error) &&
      <button type="button" onClick={retry}>다시 시도</button>}
  </div>;
}
