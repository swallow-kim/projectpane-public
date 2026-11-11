# ProjectPane 1.5.0 validation

Product commit: `30c2299241519c05de0efd399226953e368564d2`.

## Exact package

- Installer SHA256: `108836f1d9418a7dd45417178c07e8a34313aa943e40cd209e9de29c6b4a9f87`.
- Packaged app SHA256: `39cc0c32e66889816b2964f769ae79e9cf95103ca7a8d74027defbb16416ee28`.
- Read-worker SHA256: `20af9986e159c9c3d5de9551647940d02eeedd231ea7e8cf46cbe77bb874db90`.

The build began with the exact staged source subsequently committed above. The clean committed tracked-file manifest was captured during compilation; source contents did not change during the build. The source commit occurred after build startup because the first commit attempt lacked a local author identity. The previous release's author identity was reused for this commit without changing global Git settings.

The extracted NSIS payload matched the compiled app and worker, allowing only Tauri's exact bundle-marker change. All 14 notice resources and installed links passed, with no duplicate destinations or local build paths. Actual updater signature verification accepted the package and rejected altered bytes, a wrong version and a missing signature. Updater signing is separate from Windows Authenticode signing.

## Regression checks

- Vitest 445/445 and Node/security 22/22 passed on 1.5.0.
- Browser regression 269/269 passed on 1.5.0.
- Rust core tests: 369 passed, 20 ignored; desktop binary tests: 28 passed, 4 ignored. Ignored signed-package and public-feed tests are separate explicit checks, not included in those counts.
- Typecheck, production frontend build, strict Clippy, formatting and diff checks passed.
- Root persistence tests cover existing projects with no session, Windows/UNC/WSL roots, case-only WSL differences, stale revisions, invalid roots, preservation of existing locations and seven interrupted-write recovery boundaries.

## Exact packaged native acceptance

The extracted 1.5.0 app ran with isolated owned settings and WebView2 profiles on Windows, WebView2 157.0.4341.42. Six assertion groups passed:

1. Create and edit project root using the UI while preserving the old open folder.
2. Resolve a relative address from the edited root rather than the current folder.
3. Open a new tab at the edited root.
4. Open a newly shown empty panel at the edited root.
5. Close normally, restart and restore the edited project and both panels.
6. Preserve source folder sentinel contents.

Both normal closes acknowledged saved state, exited with no owned processes remaining and reported no runtime errors. No user installation or system clipboard was modified. This was an extracted-package test, not an in-place installer upgrade. Clipboard relative-path routing is covered by modeled IPC tests; live clipboard use and every remote NAS/WSL environment were not manually certified.

## Publication

[ProjectPane 1.5.0](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.5.0) was published as latest on 2026-11-11 at 07:42:44 UTC. The annotated tag resolves to the product commit above. All four draft assets were downloaded and matched locally before publication. After publication the installer, signature, latest.json and SHA256SUMS.txt were downloaded anonymously and matched byte-for-byte; the public latest feed also matched.

The repository has no Actions workflows. These are local/native/package/public-download checks, not CI claims. Later documentation-only commits do not change the release tag or binaries.

The actual updater public-feed download test passed exactly 1/1 and matched the verified installer without installing it. Immediately after publication its application/json request briefly saw the previous release through GitHub cache; verification was repeated after propagation and passed on the unchanged endpoint and client.
