# ProjectPane

Windows용 경량 멀티패널·멀티탭 파일 탐색기입니다. 프로젝트별 폴더와 화면 구성을 저장하고 여러 폴더에서 기본 파일 작업을 수행합니다.

현재 소스 버전은 **1.5.2**이며 공개 소스 저장소는 [swallow-kim/projectpane-public](https://github.com/swallow-kim/projectpane-public)입니다. Windows x64 설치 파일과 SHA256은 2026-11-13 게시한 [1.5.2 릴리스](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.5.2)에서 제공합니다. 서명·최종 패키지·공개 다운로드와 업데이트 feed를 검증했습니다. 기존 비공개 개발 이력·시험 원본은 포함하지 않습니다.

## 기능

- 1·2·4패널, 폴더 탭, 프로젝트 탭, 프로젝트 루트에서 새 탭·빈 패널 열기.
- 프로젝트별 프리셋과 마지막 작업 상태, 프로젝트 공통 폴더 즐겨찾기.
- 활성 파일 목록에서 타이핑하면 파일명 필터로 입력·포커스 이동. 기존 필터는 새 검색어로 교체하며 필터에서 Esc로 비우고 목록에 복귀.
- Ctrl/Shift 다중 선택, 빈 공간 드래그 영역 선택, 조밀한 두 줄 패널 헤더와 키보드 리사이즈.
- Ctrl+Shift+P 빠른 이동, Ctrl+Shift+T 닫은 탭 복원, 탭 닫기 보호. 상시 디스크 색인은 사용하지 않음.
- 일반 복사는 추가 재확인 없이 실행. 충돌 때 건너뛰기·다른 이름 복사, 이동·삭제 등의 확인 유지. 복사 진행·취소·결과는 상단 복사 작업 버튼에서 확인하고 다른 작업은 파일 작업 버튼에서 확인.
- 이름·확장자 분리 편집, 최종 이름 미리보기, 절대·상대경로 텍스트 복사.
- ZIP 압축·새 폴더에 해제, 설치된 압축 앱으로 전달. 기본 앱 열기·속성·탐색기에서 선택 항목 보기와 Windows Shell 메뉴.
- 업데이트 버튼과 시작 후 하루 한 번 확인. 설치형은 사용자 선택으로 다운로드·서명 검증·설치·재시작하며, ZIP은 다운로드 안내만 제공합니다.

자세한 동작과 단축키는 [사용·복구 안내](docs/user-guide.md), 기능별 변경은 [변경 내역](CHANGELOG.md)을 참고하세요.

## 1.5.2 파일 작업과 열 너비 개선

[1.5.2 패키지·배포 검증](docs/release-1.5.2-validation.md)을 참고하세요.

폴더 삭제 버튼·메뉴를 활성화하고 이동 준비 제한에는 복사 전환 안내를 제공합니다. 목록의 네 열 경계를 드래그하거나 좌우 화살표로 조절할 수 있습니다. 설정은 패널별로 저장하며 두 번 클릭하면 기본 너비로 돌아갑니다.

## 1.5.0 프로젝트 루트 편집

프로젝트 관리의 **프로젝트 편집**에서 이름과 루트 경로를 수정합니다. 새 탭과 빈 패널은 루트에서 열리고 주소창 상대경로와 상대경로 복사도 프로젝트 루트 기준입니다. 기존 탭·프리셋의 실제 폴더 위치는 유지하며 파일을 이동하지 않습니다. WSL 경로 대소문자와 중단된 설정 변경의 복구를 보강했습니다.

[1.5.0 패키지·배포 검증](docs/release-1.5.0-validation.md)을 참고하세요.

## 1.4.0 복사와 목록 응답 개선

복사 선택과 결과를 디스크에 나누어 기록하고 폴더를 순차 탐색합니다. 이 복사 경로에는 이전 128/1,024개 선택·2,048개 하위 결과 한도를 적용하지 않으며, 경로·개별 요청 크기·파일 형식 검사는 유지합니다. 목록 필터 입력과 ASCII 이름 정렬·다중 선택 해석을 개선했습니다. 측정값과 실제 앱 검사 범위는 [대용량 복사 검증 기록](docs/large-copy-validation-20261109.md)을 참고하세요.

ADS가 있는 원본을 ADS 미지원 대상으로 복사하면 해당 대상 파일을 만들기 전에 거부합니다. 스트림을 버리는 대체 복사는 하지 않습니다. 링크·재분석 지점·특수 파일 등 지원하지 않는 항목은 안전하게 중단합니다. 이미 완료된 항목은 유지되므로 실패·취소 후 원본과 대상을 확인하세요. 실제 원격 NAS는 검증하지 않았으며 Explorer 물리 입력·클립보드 왕복 시험은 네이티브 입력/클립보드 접근 제한으로 완료하지 못했습니다.

## 설치와 업데이트

1.2.1에는 updater가 없으므로 최초 업데이트 지원판은 한 번 수동 설치해야 합니다. 이후 현재 사용자 NSIS 설치형은 앱의 업데이트 버튼을 사용합니다. 먼저 다운로드와 서명을 확인하고, 별도의 설치 선택 후 파일 작업 종료·프로젝트 저장·worker 종료를 확인합니다. 저장이나 종료가 불확실하면 설치를 시작하지 않습니다. 임시 작업 공간은 프로젝트로 저장하세요.

ZIP은 별도 폴더에 새 버전을 풀고 data와 webview-profile을 보존하는 수동 방식입니다. 앱이 설치형 경로임을 확인하지 못하면 자동 설치를 제공하지 않습니다. [업데이트 동작·실패 복구](docs/updater.md)를 참고하세요.

## 지원 범위

주요 개발·검증 환경은 Windows 10 x64입니다. Windows↔WSL 복사와 같은 WSL 배포판·볼륨 안 rename 이동을 지원합니다. 다른 드라이브의 폴더 이동은 양쪽 모두 로컬 고정 NTFS인 일반 폴더로 제한합니다. 링크·충돌·메타데이터와 작업 한도 검사는 유지합니다.

외부 잘라내기·MOVE, Windows↔WSL 이동, 다른 WSL 배포판·일반 네트워크 간 이동, 폴더 병합·덮어쓰기·영구 삭제·일반 Undo는 지원하지 않습니다. 취소는 완료된 작업을 되돌리는 기능이 아닙니다. 불명 작업은 자동 재실행하지 않습니다.

Windows 11, 실제 한국어 IME·Snap·다중 모니터 DPI, Explorer 물리 드래그·실제 클립보드 왕복은 검증을 완료하지 않았습니다. 이전 버전의 8시간 기능 검사는 통과했으나 대용량 반복 탐색의 250 MiB 메모리 목표 초과는 알려진 이슈로 남아 있습니다. [검증 범위](docs/validation.md)를 확인하세요.

## 빌드

Node 24, 저장소에 고정된 Rust 도구 체인, Windows MSVC C++ Build Tools와 Windows SDK가 필요합니다. 실행에는 WebView2 Runtime이 필요합니다.

```sh
npm ci
npm run typecheck
npm test
npm run build
npx playwright install chromium
npm run test:browser
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo clippy --locked --manifest-path src-tauri/Cargo.toml --features desktop --all-targets -- -D warnings
cargo test --locked --manifest-path src-tauri/Cargo.toml --features desktop --lib
npm run desktop:build
```

npm run dev는 합성 목록의 브라우저 개발 화면, npm run desktop:dev는 실제 파일 시스템을 사용하는 데스크톱 앱입니다. 시험에는 전용 합성 파일만 사용하세요. 설치·제거·네이티브 입력 시험은 위 자동검사에 포함되지 않습니다.

Windows 산출 경로는 src-tauri/target/release/bundle/nsis/입니다. 빌드는 설치를 실행하지 않습니다. WebView2가 포함된 오프라인 설치판과 Windows Authenticode 코드 서명은 제공하지 않습니다. Updater의 암호학적 서명은 Windows 코드 서명과 별개입니다. [설치·배포 요구사항](docs/distribution.md)을 확인하세요.

서명 설치 패키지를 만드는 `npm run desktop:build`에는 별도로 보관한 updater 서명키가 필요합니다. 공개 소스에는 개인키가 없습니다. 서명 없는 로컬 확인용 패키지의 설정 방법은 [배포 문서](docs/distribution.md#로컬-빌드와-배포-준비)를 참고하세요.

## 문서와 라이선스

- [1.4.0 최종 패키지·배포 검증](docs/release-1.4.0-validation.md)
- [사용·복구 안내](docs/user-guide.md)
- [아키텍처](docs/architecture.md), [UI 규칙](docs/design-system.md), [후속 단계](docs/post-v1-phases.md)
- [MIT 라이선스](LICENSE), [제3자 고지](THIRD_PARTY_NOTICES.md)
- [의존성 목록](docs/dependency-licenses-1.5.0.md), [원문 고지 모음](docs/third-party-license-texts.md)

ProjectPane 소유 코드·문서는 MIT입니다. 제3자 코드·아이콘·SDK·설치 도구에는 각각의 원래 조건이 적용됩니다.
