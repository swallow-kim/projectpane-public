# 설치·배포 요구사항

현재 버전은 1.2.1입니다. [공개 소스 저장소](https://github.com/swallow-kim/projectpane-public)에는 설치 파일·ZIP이나 바이너리 릴리스가 없습니다. 이 문서는 향후 바이너리 배포 전 확인할 요구사항입니다.

## 빌드와 설치형

루트 README의 도구 체인을 준비하고 npm ci, npm run desktop:build를 실행합니다. worker·프런트엔드 빌드 뒤 NSIS를 생성하며 설치나 기존 앱 교체를 실행하지 않습니다.

기본 산출 경로는 src-tauri/target/release/bundle/nsis/입니다. 기본 설치 경로는 %LOCALAPPDATA%\\ProjectPane이며 현재 사용자 설치입니다. 실행 중 앱·worker가 있으면 정상 종료를 요구하며 강제 종료하지 않습니다. 기존 설치는 같은 경로에 업데이트/재설치합니다.

WebView2 Runtime은 포함되지 않습니다. 신규 설치에서 런타임이 없으면 Microsoft 다운로드 연결이 필요합니다. 런타임이 없는 업데이트는 오류로 중단합니다. 오프라인 런타임 포함 패키지·코드 서명·자동 업데이트는 제공하지 않습니다. 조직의 보안 절차를 우회하지 마세요.

## ZIP과 데이터 보존

ZIP을 준비한다면 같은 빌드의 projectpane.exe와 projectpane-read-worker.exe, scripts/Start-ProjectPane-Beta.cmd, 고지·사용 안내를 함께 넣어야 합니다. 현재 export는 ZIP 생성이나 설치 시험을 수행하지 않습니다. launcher는 ZIP 위치의 data·webview-profile을 사용하므로 갱신 때 두 디렉터리를 보존해야 합니다.

업데이트 전 정상 종료·저장 완료와 설정 백업을 확인하세요. 새 버전 형식의 수동 다운그레이드나 불확실한 작업의 자동 재실행은 지원하지 않습니다.

## 배포 완료 점검표

- 버전 원본·lockfile·앱·worker가 같은 소스인지 확인하고 해시를 기록합니다.
- [MIT](../LICENSE), [제3자 고지](../THIRD_PARTY_NOTICES.md), [의존성 목록](dependency-licenses-1.2.1.md), [원문 고지](third-party-license-texts.md)를 수령자가 열 수 있도록 실제 배포물에 포함합니다.
- MPL 구성요소의 정확한 소스 위치와 수정분 제공 여부를 확인합니다. 빌드 전용 의존성을 실제 런타임과 구분합니다.
- NSIS 플러그인·압축 모듈, Microsoft WebView2 SDK loader의 실제 조건을 확인합니다. Rust binding의 MIT가 Microsoft 구성요소에 적용되는 것은 아닙니다.
- 개인 빌드 경로·시험 자료가 포함되지 않았는지 확인합니다. 로컬 검증 바이너리는 공개 배포용 산출물이 아닙니다.
- 격리 환경에서 설치·업데이트·종료·설정 보존을 검증합니다. 빌드 통과를 설치 통과로 표시하지 않습니다.
- README·사용 안내·변경 내역·지원 범위·검증 한계를 함께 최신화합니다.
- 목적지·파일 범위 승인을 받고 게시합니다. 기존 비공개 이력·이슈·릴리스·원본 감사 자료를 자동 이관하지 않습니다.

## 자동화

공개 준비 사본에는 GitHub workflows를 포함하지 않습니다. 기존 설치 실행 workflow와 광범위한 진단 자료 업로드를 자동 이전하지 않습니다. 향후 CI는 읽기 권한, 외부 PR secret 차단, 검증된 action 참조와 명시적 산출물 목록으로 별도 구성해야 합니다. 로컬 통과는 공개 저장소 CI 성공이 아닙니다.
