import { t } from '../../shared/i18n';

/** Stored errors stay language-neutral so changing language does not alter controller state. */
export function projectErrorText(error: string | null | undefined): string | null | undefined {
  switch (error) {
    case '세션을 저장하지 못했습니다. 다시 시도하세요.':
      return t(error, 'Could not save the session. Try again.');
    case '다른 저장과 충돌했습니다. 현재 작업을 유지했습니다. 다시 불러오기 전에 변경을 확인하세요.':
      return t(error, 'A conflicting save was detected. Your current work is preserved. Review your changes before reloading.');
    case '프로젝트 작업에 실패했습니다. 현재 작업을 유지했습니다. 다시 시도하세요.':
      return t(error, 'The project action failed. Your current work is preserved. Try again.');
    case '현재 작업 상태를 저장할 수 없습니다. 경로와 탭 수를 확인하세요.':
      return t(error, 'Could not save the current workspace. Check the paths and number of tabs.');
    case '종료 전 저장을 완료하지 못했습니다. 창을 유지합니다. 저장 재시도 후 다시 닫으세요. 파일 작업은 앱을 다시 실행한 뒤 사용할 수 있습니다.':
      return t(error, 'Could not finish saving before closing. The window will stay open. Retry saving, then close again. File operations will be available after restarting the app.');
    case '종료 전 저장 연결을 준비하지 못했습니다.':
      return t(error, 'Could not prepare saving on exit.');
    default: return error;
  }
}
