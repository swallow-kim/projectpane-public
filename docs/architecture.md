# 아키텍처

ProjectPane 1.2.1은 React·TypeScript 프런트엔드와 Tauri·Rust 데스크톱 호스트, 별도 읽기 worker로 구성됩니다.

## 경계

- src/app: 창 레이아웃, 프로젝트·패널 구성과 전역 UI.
- src/features/explorer: 탭·탐색·선택·필터·가상 목록·페이지 캐시·키보드 입력.
- src/features/projects: 프로젝트 저장·복원·프리셋과 종료 흐름.
- src/features/operations: 파일 작업 준비·실행·진행·결과 및 드롭 처리.
- src/shared: IPC 계약과 입력·응답 검증.
- src-tauri/src/domain: 경로·식별자·오류·목록·파일 작업 계약.
- src-tauri/src/services: 목록 broker, 프로젝트·파일 작업 서비스와 실행 수명.
- src-tauri/src/platform: worker 통신, 파일 시스템·Windows Shell/COM, 감시.
- src-tauri/src/persistence: 설정·프로젝트·작업 기록·복구 저장.
- src-tauri/src/desktop: Tauri 명령과 서비스 조립·창 수명.
- src-tauri/src/bin/read-worker.rs: 별도 읽기 프로세스.

## 목록과 반응성

폴더 열거는 worker에서 수행하고 UI는 필요한 표시 창만 요청합니다. 스냅샷 identity와 view revision으로 오래된 정렬·필터·탐색 응답을 구분합니다. 페이지 캐시와 가상 목록의 표시 행을 제한하며, 전체 목록을 DOM에 만들지 않습니다. 폴더 크기를 재귀 계산하거나 디스크를 상시 색인하지 않습니다.

감시 이벤트와 명시적 새로고침은 기존 목록 수명 계약을 따릅니다. 데이터 준비 중에는 오래된 선택에 대한 파일 작업을 막습니다. worker와 호스트는 같은 빌드로 배포해야 합니다.

## 파일 작업과 저장

파일 작업은 목록 읽기와 분리됩니다. 입력 경로·원본 식별·충돌·링크·허용 작업 범위를 내부에서 검증합니다. 일반 비충돌 복사는 중복 확인 없이 실행하며, 삭제·이동 등의 확인은 유지합니다. 결과는 완료·실패·취소·부분 성공·불명 상태를 구분하고 불명 작업을 자동 재실행하지 않습니다.

프로젝트 프리셋과 마지막 세션은 별개입니다. 저장 응답 확인과 파일 작업 종료 상태를 확인한 뒤 정상 종료합니다. 설치 중 실행 중인 앱을 강제 종료하지 않습니다. 외부 MOVE 실험 코드는 시험 전용이며 제품에서 비활성입니다.

## 보안과 검증

Tauri capability는 로컬 main 창에 명시적 앱 명령만 허용합니다. CSP는 로컬 스크립트와 IPC로 제한됩니다. 전체 파일시스템·shell·opener 기본 권한을 부여하지 않습니다. Shell 메뉴에서 위임한 외부 명령은 앱의 작업 저널·복구와 구분됩니다.

[검증 범위](validation.md), [사용·복구 안내](user-guide.md), [외부 MOVE 제한](external-move-boundaries.md)을 참고하세요. 모든 파일 시스템 호출이 비동기이거나 모든 Shell 확장이 호환된다는 보장은 하지 않습니다.
