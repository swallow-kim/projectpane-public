# ProjectPane 1.5.1 validation

Product commit and tag target: `bb157e28ff831ad0ab75354a380bdb5d07a3e09d` (`v1.5.1`). Built from clean committed source on 2026-11-13. Subsequent documentation-only commits do not change this package.

## Exact package

- Installer SHA256: `5e70d2ec7694d76684a9bddf67af06fb00a56383161bb19b8e8d8653ccae332b`.
- Packaged app SHA256: `19514931a239c02acab1e16a0f253f1dc1529202db0095f7fcc490a0fcf24f2a`.
- Read-worker SHA256: `a90c6dd7ab880b6452bee788a8541b3fdb09499fa0e2adbb3d2e5aca41163377`.

Extracted NSIS payload matched compiled binaries allowing only Tauri's exact bundle marker. All 14 notice resources passed, with no duplicate destinations or local build paths. Actual updater accepted the signature and rejected tampering, wrong version and missing signature. Updater signing is separate from Windows Authenticode signing.

## Regression checks

- Vitest 461/461; Node/security 22/22; browser regression 279/279 passed.
- Rust library: 372 passed, 21 ignored; desktop binary: 28 passed, 4 ignored. Explicit package/feed tests are separate from these counts.
- Typecheck, frontend production build, strict Clippy, formatting and secret scan passed. Vite reports its existing bundle-size warning.
- Folder recycle focused tests: 10 passed. Separate Windows native tests recycled an owned folder and a mixed file/folder batch successfully.

## Exact packaged native acceptance

The extracted package ran with isolated owned settings and WebView2 157.0.4341.46. Nine assertion groups passed:

1. Folder recycle enabled in toolbar and context menu; previews cancelled without mutation; plain Windows menu label checked.
2. Column resizing stored changed widths in native WebView.
3. Project creation and root editing preserved existing open folders.
4. Relative address resolved from edited project root rather than current folder.
5. New tab opened at edited root.
6. Newly shown empty panel opened at edited root.
7. Normal close/restart restored edited root and both panels.
8. A 1,025-item move preparation limit visibly offered copy; explicit copy created all 1,025 files and preserved every original.
9. Source folders and sentinel contents remained unchanged.

Both normal closes acknowledged saved state, left no owned processes and reported no runtime errors. User installation and system clipboard were untouched. Initial test attempts exposed harness readiness/context-menu timing and insufficient copy-wait timeout; these were corrected before the complete passing run. Disk exhaustion interrupted initial desktop compilation and signed build; moving regenerable build cache to another drive allowed clean successful retries. No failed build was published.

## Publication

The four draft assets were downloaded and matched before publication. After publication, anonymous downloads of installer, signature, latest.json and SHA256SUMS.txt matched staged bytes. The latest-release feed matched 1.5.1. The actual updater independently downloaded the public feed package and matched the verified local installer (1 passed), without installing it.

[Release and downloads](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.5.1).

## Boundaries

Folder deletion remains recycle-only on supported local fixed drives, with confirmation and identity checks; unsupported providers never fall back to permanent deletion. Other applications must not modify sources during a prepared operation. Cut-to-copy fallback is restricted to safe preparation failures, never uncertain or partly executed moves. External Windows cut clipboard remains explicitly unsupported rather than silently ignored. SFTP is unchanged. Every remote NAS/WSL environment and a live installed-app upgrade were not certified; native acceptance used the exact extracted package. No CI workflow result is claimed.
