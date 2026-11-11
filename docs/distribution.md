# Windows 배포와 업데이트

현재 소스와 공개 배포 버전은 1.5.0입니다. 2026-11-11 게시한 Windows x64 설치 파일, updater 서명, 버전 feed와 SHA256은 [1.5.0 릴리스](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.5.0)에서 제공합니다. [최종 패키지·배포 검증](release-1.5.0-validation.md)에서 정확한 제품 커밋과 해시, 공개 다운로드 검증 결과를 확인하세요. 서명·격리 업데이트 검증이 완료되기 전의 로컬 설치파일은 정식 배포물이 아닙니다.

## 설치형과 ZIP판

설치형은 Windows x64 현재 사용자 NSIS 패키지이며 기본 위치는 `%LOCALAPPDATA%\ProjectPane`입니다. 기존 설치 경로에 업데이트하고 프로젝트·설정·작업 기록을 보존합니다. 실행 중인 앱이나 worker를 강제 종료하지 않습니다. 수동 설치 전에는 파일 작업을 마치고 앱을 정상 종료하세요.

1.2.1 이하에는 updater가 없으므로 첫 지원 버전은 한 번 수동 설치해야 합니다. 1.3.0부터는 상단 **업데이트**에서 확인·다운로드·설치 후 다시 시작을 각각 선택합니다. 시작 후 하루 한 번 확인하며 강제 설치나 상시 네트워크 polling은 없습니다. 자세한 저장·종료 조건과 실패 복구는 [앱 내 업데이트](updater.md)를 참고하세요.

ZIP판을 배포하는 경우 `projectpane.exe`, `projectpane-read-worker.exe`, launcher와 라이선스 고지를 함께 제공합니다. ZIP launcher는 해당 폴더의 `data`와 `webview-profile`을 사용합니다. ZIP판에서는 업데이트 확인과 다운로드 페이지 안내만 제공하며 설치형으로 자동 교체하지 않습니다. 별도 폴더에 새 버전을 풀고, 앱을 종료한 상태에서 자신의 데이터 보존 계획에 따라 전환하세요.

WebView2 Runtime은 패키지에 포함되지 않습니다. 신규 설치 시 Runtime이 없으면 Microsoft 다운로드 연결이 필요합니다. 업데이트 시 Runtime이 없으면 설치를 중단합니다. 회사망 정책이나 OS 보안 경고를 우회하지 마세요. 오프라인 설치판과 Windows Authenticode 코드서명은 이번 배포 범위에 없습니다. updater의 파일 검증 서명은 Windows SmartScreen 신뢰 서명과 다릅니다.

## 로컬 빌드와 배포 준비

README의 도구 체인을 준비한 뒤 `npm ci`, `npm run desktop:build`를 실행합니다. 기본 산출물은 `src-tauri/target/release/bundle/nsis/`입니다. `CARGO_TARGET_DIR`을 사용하면 해당 경로 아래에 생성됩니다. 빌드만으로 설치본을 교체하지 않습니다.

공개 소스에는 개인키가 없으므로 위 서명 빌드는 `TAURI_SIGNING_PRIVATE_KEY`와 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`가 별도로 필요합니다. 개인키나 암호를 명령 기록·저장소에 쓰지 마세요. 서명 없는 로컬 확인용 설치 패키지는 임시 Tauri 설정 파일에 `{"bundle":{"createUpdaterArtifacts":false}}`를 넣고 CLI의 `--config` 파일 옵션으로 병합할 수 있습니다. 이 설정은 공식 공개키를 바꾸지 않으며 생성된 패키지를 공식 updater 배포물로 게시해서는 안 됩니다.

worker는 Cargo의 추가 실행 파일로 패키지에 한 번 포함합니다. 같은 파일을 `externalBin`에도 등록하면 한 설치 경로에 서로 다른 빌드가 중복 포함될 수 있으므로 사용하지 않습니다.

서명 릴리스는 승인된 개인키를 빌드 프로세스에만 전달하고 `bundle.createUpdaterArtifacts`를 활성화해야 합니다. 개인키·암호는 저장소나 릴리스에 포함하지 않습니다. 공개키가 비어 있거나 버전 서명 검증을 끝내지 않은 패키지는 게시하지 않습니다. `latest.json`은 해당 버전의 EXE URL과 그 EXE의 서명만 가리켜야 합니다. 동일한 릴리스 자산이나 태그를 덮어쓰지 않습니다.

## 릴리스 완료 점검

- package/Cargo/Tauri/lockfile 버전, EXE·worker·설치파일의 출처와 SHA256을 맞춥니다.
- updater 서명과 서명 안의 버전을 검증하고 변조·다운그레이드·다운로드 실패 시 설치가 시작되지 않는지 확인합니다.
- 격리 데이터로 저장 ACK, 파일 작업 종료, worker 종료, 설치 인계 및 재시작 복원을 확인합니다. 실제 개인 설치본 시험과 구분합니다.
- [MIT](../LICENSE), [제3자 고지](../THIRD_PARTY_NOTICES.md), [의존성 목록](dependency-licenses-1.5.0.md), [라이선스 원문](third-party-license-texts.md)과 추가 구성요소 고지를 설치 폴더의 `licenses`에 포함합니다.
- 바이너리에 개인 빌드 경로·비밀정보가 없는지 검사하고 PDB·개인 로그를 제외합니다. README·사용자 안내·검증 범위·알려진 제한을 동기화합니다.
- 공개 SHA, 자산 해시, 익명 다운로드, feed와 자산의 대응을 확인합니다. 실패한 검사나 미검증 환경을 통과로 바꾸지 않습니다.

현재 공개 저장소에는 Actions workflow가 없습니다. 로컬 빌드와 기존 인증의 수동 게시를 사용하며 CI 통과를 주장하지 않습니다. 별도 비공개 저장소의 과거 CI 결제 제한은 코드 실패나 이 공개 저장소의 CI 결과가 아닙니다.

## 1.5.0 검증 경계

대용량 복사 구현과 실제 실행 결과는 [검증 기록](large-copy-validation-20261109.md)에 구분합니다. 실제 원격 NAS 시험은 없으며 Explorer 물리 입력·클립보드 왕복은 네이티브 입력/클립보드 접근 제한으로 완료하지 못했습니다. ADS가 있는 파일은 ADS 미지원 대상으로 해당 파일을 만들기 전에 거부합니다. 지원하지 않는 특수 파일·재분석 지점에는 대체 복사를 하지 않습니다. 이러한 제한은 설치 패키지 생성만으로 해소되지 않습니다.
