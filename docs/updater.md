# 앱 내 업데이트

1.3.0은 최초 updater 지원판입니다. 1.2.1과 이전 버전에서는 지원판 설치 파일을 한 번 수동으로 실행해야 합니다. 이 문서는 구현된 동작을 설명하며 실제 게시·서명·설치 검증 상태는 [검증 기록](validation.md)과 GitHub Releases 자산을 기준으로 합니다.

## 확인과 설치

- 상단 업데이트 버튼에서 즉시 확인할 수 있습니다.
- 앱이 준비된 뒤 5초 후 비동기로 확인합니다. 마지막 시도 시각을 앱 데이터 폴더에 저장해 하루 한 번만 자동 요청합니다. 오프라인 실패도 캐시하며 수동 확인은 캐시를 무시합니다. 체크 기록을 쓸 수 없는 환경에서는 재실행 시 다시 확인할 수 있습니다.
- 상시 네트워크 polling, 인증 토큰, telemetry, 자동 다운로드/강제 설치는 없습니다. 창을 열거나 다운로드/설치 준비 중일 때만 로컬 상태를 잠깐 갱신합니다.
- 새 버전과 변경 내용을 표시합니다. **다운로드**와 **설치 후 다시 시작**은 별도 선택입니다. 다운로드는 취소할 수 있습니다.
- HKCU의 기존 NSIS 설치 경로와 현재 실행 파일이 정확히 일치하고 제거 프로그램이 존재하는 경우만 설치형으로 인정합니다. 데이터/웹뷰 경로를 별도로 지정한 ZIP·개발 실행, 확인 불가 경로는 릴리스 안내만 제공합니다.
- 임시 작업 공간은 먼저 프로젝트로 저장하세요. 설치 직전 프로젝트·즐겨찾기·창 상태 저장을 확인하고, 파일 작업은 기존 창에서 완료 대기 또는 정상 취소합니다. 취소는 이미 완료된 파일 작업의 Undo가 아닙니다.
- 미저장 작업 결과, 저장 ACK 실패/시간 초과, Shell 메뉴나 읽기 worker 종료 불확실 시 설치를 중단합니다. updater가 기존 close/save 경로를 건너뛰지 않습니다.

## 실패와 복구

검사/네트워크/서명 실패는 설치를 시작하지 않습니다. 공개 feed에서 더 높은 정식 semver만 허용하고, 정확한 공개 저장소 EXE 주소 및 서명에 결합된 버전을 검사합니다. 예전 서명 파일을 새 버전으로 속인 feed와 다운그레이드는 거부합니다.

준비 중 파일 작업 창에서 **창 유지**를 선택하면 설치 요청을 보류합니다. 결과 기록을 버리고 업데이트하는 기능은 없습니다. 저장 또는 종료 확인 실패 후 일부 서비스가 종료됐다면 상태를 확인하고 앱을 정상 종료한 뒤 다시 실행하세요.

설치 프로그램은 현재 앱이 자연 종료하기를 최대 약 10초 기다리고, 계속 사용 중이거나 확인이 실패하면 교체를 차단합니다. 다른 앱이나 worker를 강제로 종료하지 않습니다. 설치 오류는 NSIS 창에 남습니다. 앱은 설치 전에 목표 버전만 기록하며 다음 시작 때 실행 버전이 일치하는지 확인합니다. 미확인/중단 설치는 자동 재시도하지 않습니다. 이 기록은 파일 교체 전체의 원자성이나 자동 롤백을 보장하지 않습니다.

설치형의 프로젝트·설정은 기존 앱 데이터 폴더를 유지합니다. ZIP은 새 폴더에 풀어 실행 파일을 갱신하고 기존 data/webview-profile을 보존하세요. 기존 앱이 실행 중이면 직접 파일을 덮어쓰지 마세요.

## 배포 담당자

Tauri 2.12.1 + updater 2.14.1을 고정합니다. HTTPS 공개 GitHub Release의 latest.json을 읽으며 클라이언트에 GitHub 토큰을 넣지 않습니다. updater 개인키는 저장소·배포 ZIP·GitHub Release·CI 로그에 포함하지 않습니다. 개인키 또는 복호화 암호를 잃으면 기존 앱의 업데이트 경로를 유지할 수 없습니다.

Updater 서명은 Microsoft Authenticode 코드 서명과 다릅니다. 현재 Windows 코드 서명과 WebView2 포함 오프라인 설치판은 제공하지 않습니다. 회사 보안 정책이나 OS 경고를 우회하지 마세요.

참고: [Tauri updater](https://v2.tauri.app/plugin/updater/), [고정 updater 구현](https://docs.rs/tauri-plugin-updater/2.14.1/tauri_plugin_updater/), [Windows 코드 서명](https://v2.tauri.app/distribute/sign/windows/).

## 유지보수용 격리 검사

일반 `npm test`/브라우저 검사는 모의 IPC를 사용합니다. 실제 키 검증은 `PROJECTPANE_TEST_UPDATE_PACKAGE`에 서명된 로컬 EXE를 지정한 뒤 Rust의 `actual_plugin_verifies_release_bytes_and_rejects_tampering_and_wrong_version` ignored 검사를 명시적으로 실행합니다. `.sig`는 EXE 바로 옆에 있어야 하며 개인키는 검사 코드가 읽지 않습니다.

`scripts/test-updater-installer-guard.mjs`는 생산용 NSIS 가드를 작은 합성 실행 파일로 검사합니다. `scripts/prepare-updater-handoff.mjs`는 레지스트리와 실제 설치본을 건드리지 않는 합성 NSIS 파일과 설정을 만듭니다. 이 합성 파일을 배포 버전에 묶어 로컬 서명한 뒤, `cargo test --manifest-path src-tauri/Cargo.toml --features desktop --bin projectpane --no-run --message-format=json`의 출력은 `artifacts/updater-review/native-test-executable.jsonl`에 보관합니다. `scripts/test-updater-handoff.ps1`는 해당 테스트 호스트를 합성 폴더에 복사해 실제 플러그인 인계·가드·재시작·합성 설정 보존을 확인합니다. 합성 설치파일은 릴리스에 포함하지 않습니다.

위 검사는 실제 Windows 프로세스와 플러그인 설치 API를 사용하지만 전체 사용자 GUI에서 두 정식 버전 사이를 업데이트한 검사는 아닙니다. 마지막 실제 공개 feed 다운로드 검사는 게시 후 `published_feed_download_matches_the_verified_local_package`를 명시 실행하며 설치는 하지 않습니다. 공개 패키지 내용·고지·중복 파일·빌드 대응은 `scripts/verify-nsis-payload.mjs`로 별도 확인합니다.
