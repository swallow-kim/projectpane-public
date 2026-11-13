# ProjectPane

**English** | [한국어](README.ko.md)

A lightweight multi-pane, multi-tab file explorer for Windows. Save folders and layouts by project, and work with files across multiple folders.

The current version is **1.5.2**. Download the Windows x64 installer and SHA256 checksums from the [1.5.2 release](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.5.2), published on November 13, 2026. The signed update package, final installer payload, public downloads and update feed have been verified. This [public source repository](https://github.com/swallow-kim/projectpane-public) does not include the earlier private development history or original test artifacts.

## Features

- One, two or four panes, folder tabs and project tabs. New tabs and empty panes open at the project root.
- Per-project presets and saved working state, plus shared folder favorites across projects.
- Type in the active file list to start a new filename filter. Esc clears the filter and returns focus to the list.
- Ctrl/Shift multi-selection, drag selection on empty space, compact two-row pane headers and keyboard resizing.
- Ctrl+Shift+P quick navigation, Ctrl+Shift+T closed-tab restoration and tab-close protection. No always-on disk indexing.
- Ordinary copies run without an extra confirmation. Conflicts offer skip or copy under another name; moves and deletions retain confirmation. The top-bar copy button shows copy progress, cancellation and results; other operations use the file-operations button.
- Separate filename and extension editing, final-name previews, and copying absolute or project-relative paths as text.
- ZIP creation and extraction into a new folder, and handoff to installed archive apps. Open with the default app, view properties, reveal selections in Explorer, or use the Windows Shell menu.
- An update button and a once-a-day check after startup. Installed copies can download, verify, install and restart with user approval; ZIP copies provide download guidance only.
- Korean and English interface options, switchable immediately from the header with a saved preference.

See the [user and recovery guide](docs/user-guide.md) for behavior and shortcuts, and the [changelog](CHANGELOG.md) for version history. Most detailed guides are currently in Korean; the English README summarizes the main behavior and limitations.

## What's new in 1.5.2: Korean and English

Choose **한국어 / English** in the header. The interface changes immediately and remembers the choice on the next launch. Korean remains the default. Menus, dialogs, statuses, errors and accessibility labels are translated; your filenames, project names and paths are not changed. If the preference cannot be saved, the app displays a session-only warning.

Windows Shell menus, external application dialogs and server-provided release notes keep their own language. Dates use PC local time in YYYY-MM-DD HH:mm format.

See the [1.5.2 package and release validation](docs/release-1.5.2-validation.md).

## 1.5.1: file actions and column widths

Folders can be selected for deletion through the toolbar and context menu. When a move cannot be prepared, the app explains the limitation and offers copying instead. Resize the Name, Type, Size and Modified columns by dragging their boundaries or using the left/right arrow keys. Widths are saved per pane; double-click a boundary to restore its default.

See the [1.5.1 package and release validation](docs/release-1.5.1-validation.md).

## 1.5.0: editable project roots

Use **Edit project** in project management to change the name and root path. New tabs and empty panes start at the root. Relative paths in the address bar and copied relative paths are based on the project root, not the current folder. Existing tabs and presets keep their actual folder locations; editing a root does not move files. WSL path case handling and recovery from interrupted settings changes were also improved.

See the [1.5.0 package and release validation](docs/release-1.5.0-validation.md).

## 1.4.0: larger copies and more responsive lists

Copy selections and results are recorded incrementally on disk, and folders are traversed progressively. This copy path no longer uses the previous 128/1,024-item selection or 2,048-child-result limits; path, individual request-size and file-type checks remain. Filename filtering, ASCII natural sorting and multi-selection processing were also improved. See the [large-copy validation record](docs/large-copy-validation-20261109.md) for measurements and native-app test coverage.

Copying a file with alternate data streams (ADS) to a destination that cannot preserve them is rejected before creating that destination file. There is no fallback that silently discards streams. Unsupported links, reparse points and special files stop safely. Completed items remain after a failure or cancellation, so check both source and destination. Real remote NAS testing was not completed. Physical Explorer input and system-clipboard round-trip tests were also blocked by native input/clipboard access restrictions.

## Installation and updates

Version 1.2.1 has no updater, so upgrading from it requires a one-time manual installation of an updater-enabled version. Subsequent per-user NSIS installations can use the app's update button. The app downloads and verifies the signature first. After you separately choose to install, it checks that file operations have ended, projects have been saved and the worker has stopped. Installation does not start if saving or shutdown is uncertain. Save temporary workspaces as projects.

For ZIP copies, extract the new version into a separate folder and preserve `data` and `webview-profile` manually. Automatic installation is not offered when the app cannot confirm that it is running from an installed location. See [update behavior and failure recovery](docs/updater.md).

## Support and limitations

The main development and validation environment is Windows 10 x64. Windows ↔ WSL copying and rename-based moves within the same WSL distribution and volume are supported. Cross-drive folder moves are limited to ordinary folders on local fixed NTFS volumes at both ends. Link, conflict, metadata and operation-limit checks remain in place.

External cut/MOVE operations, Windows ↔ WSL moves, moves between WSL distributions or general network locations, folder merging, overwriting, permanent deletion and general Undo are not supported. Cancellation does not roll back completed work. Operations with an uncertain outcome are not automatically retried. SSH/SFTP support is not included.

Windows 11, physical Korean IME input, Snap, multi-monitor DPI, physical Explorer drag-and-drop and real system-clipboard round trips have not completed validation. An earlier eight-hour functional test passed, but exceeding the 250 MiB memory target during repeated large-folder browsing remains a known issue. See the [validation scope](docs/validation.md).

## Build from source

Requires Node 24, the repository-pinned Rust toolchain, Windows MSVC C++ Build Tools and the Windows SDK. WebView2 Runtime is required to run the app.

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

`npm run dev` starts the browser development UI with synthetic listings. `npm run desktop:dev` starts the desktop app against the real filesystem. Use dedicated synthetic files for tests. Installation, uninstallation and native-input tests are not included in the checks above.

Windows installers are written to `src-tauri/target/release/bundle/nsis/`. Building does not install the app. Offline installers bundled with WebView2 and Windows Authenticode code signing are not provided. Cryptographic updater signatures are separate from Windows code signing. See [installation and distribution requirements](docs/distribution.md).

Creating a signed installer with `npm run desktop:build` requires a separately stored updater signing key; no private key is included in the public source. For unsigned local test packages, see the [local-build configuration](docs/distribution.md#로컬-빌드와-배포-준비).

## Documentation and license

- [1.4.0 final package and release validation](docs/release-1.4.0-validation.md)
- [User and recovery guide](docs/user-guide.md)
- [Architecture](docs/architecture.md), [UI rules](docs/design-system.md), [follow-up phases](docs/post-v1-phases.md)
- [MIT license](LICENSE), [third-party notices](THIRD_PARTY_NOTICES.md)
- [Dependency inventory](docs/dependency-licenses-1.5.2.md), [full third-party notices](docs/third-party-license-texts.md)

ProjectPane-owned code and documentation are MIT-licensed. Third-party code, icons, SDKs and installer tools retain their respective licenses.
