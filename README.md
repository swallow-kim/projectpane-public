# ProjectPane

Windows용 경량 멀티패널·멀티탭 파일 탐색기입니다. 프로젝트별 폴더와 화면 구성을 저장하고 여러 폴더에서 기본 파일 작업을 수행합니다.

현재 소스 버전은 **1.2.1**이며 공개 소스 저장소는 [swallow-kim/projectpane-public](https://github.com/swallow-kim/projectpane-public)입니다. 아직 공개 바이너리 릴리스는 없습니다. 기존 개발 이력·시험 원본·설치 파일은 포함하지 않습니다.

## 기능

- 1·2·4패널, 폴더 탭, 프로젝트 탭, 프로젝트 루트에서 새 탭·빈 패널 열기.
- 프로젝트별 프리셋과 마지막 작업 상태, 프로젝트 공통 폴더 즐겨찾기.
- 활성 파일 목록에서 타이핑하면 파일명 필터로 입력·포커스 이동. 기존 필터는 새 검색어로 교체하며 필터에서 Esc로 비우고 목록에 복귀.
- Ctrl/Shift 다중 선택, 빈 공간 드래그 영역 선택, 조밀한 두 줄 패널 헤더와 키보드 리사이즈.
- Ctrl+Shift+P 빠른 이동, Ctrl+Shift+T 닫은 탭 복원, 탭 닫기 보호. 상시 디스크 색인은 사용하지 않음.
- 일반 복사는 추가 재확인 없이 실행. 충돌 때 건너뛰기·다른 이름 복사, 이동·삭제 등의 확인 유지. 진행·취소·결과는 상단 파일 작업 버튼에서 확인.
- 이름·확장자 분리 편집, 최종 이름 미리보기, 절대·상대경로 텍스트 복사.
- ZIP 압축·새 폴더에 해제, 설치된 압축 앱으로 전달. 기본 앱 열기·속성·탐색기에서 선택 항목 보기와 시험용 Shell 메뉴.

자세한 동작과 단축키는 [사용·복구 안내](docs/user-guide.md), 기능별 변경은 [변경 내역](CHANGELOG.md)을 참고하세요.

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

Windows 산출 경로는 src-tauri/target/release/bundle/nsis/입니다. 빌드는 설치를 실행하지 않습니다. WebView2가 포함된 오프라인 설치판, 코드 서명, 앱 내 업데이트는 제공하지 않습니다. [설치·배포 요구사항](docs/distribution.md)을 확인하세요.

## 문서와 라이선스

- [사용·복구 안내](docs/user-guide.md)
- [아키텍처](docs/architecture.md), [UI 규칙](docs/design-system.md), [후속 단계](docs/post-v1-phases.md)
- [MIT 라이선스](LICENSE), [제3자 고지](THIRD_PARTY_NOTICES.md)
- [의존성 목록](docs/dependency-licenses-1.2.1.md), [원문 고지 모음](docs/third-party-license-texts.md)

ProjectPane 소유 코드·문서는 MIT입니다. 제3자 코드·아이콘·SDK·설치 도구에는 각각의 원래 조건이 적용됩니다.
