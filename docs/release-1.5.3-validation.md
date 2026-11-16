# ProjectPane 1.5.3 validation

Product build commit: `021d1f8bd545308795fa8a94b595c2ae8e4a7411`. Built from clean committed source on 2026-11-16. Later documentation or test-only changes do not alter the package.

## Features and compatibility

Project tabs have root-type icons, text badges, path tooltips and accessible descriptions. UNC and WSL classification does not contact servers; Windows drive letters use GetDriveTypeW, including DRIVE_REMOTE for mapped network drives. Unknown or unavailable drive types are not labeled local. Root classification is display information, not a permission or connectivity check.

Tab order is saved in the existing manifest array without schema migration. Native Windows uses pointer capture with a movement threshold, preserving the existing native file-drop integration. Browser drag uses an internal MIME type. Ctrl+Shift+Left/Right provides keyboard ordering. Escape, pointer cancellation and release outside the strip cancel. Reordering preserves the active project and does not move files.

The backend validates complete unique ID permutations and compares the expected catalog order under the existing transaction lock. Stale updates, invalid IDs and partial permutations are rejected. An uncertain acknowledgement is recovered by reading the catalog, not by repeating the write. Shutdown waits for pending order writes and refuses an unresolved save.

## Exact package

- Installer SHA256: `8f27c93cd85cc9aac891ae94fd93d987cc794940dd20a146c632898b809d7dfb`.
- Extracted app SHA256: `f4c4fe8b5b604eea47171a07dbce0d9a78b19467c07acd44bbbd9cf9c43afad0`.
- Worker SHA256: `a2de24a9b4dc145ea4009d509fc61f728464adf9a330722994394cdd77594270`.

The extracted NSIS payload matched compiled binaries, allowing only Tauri's exact bundle marker. All 14 notice resources and relative links passed; no duplicate destinations or local build paths were found. The actual updater accepted the final signed package and rejected tampering, wrong versions and missing signatures. Updater signatures are separate from Windows Authenticode signing.

## Automated checks

- Vitest: 571 passed. Node/security: 23 passed.
- Rust library: 383 passed, 21 ignored. Desktop binary: 28 passed, 4 ignored. Explicit signature tests are separate.
- Typecheck, production build, Rust formatting, strict Clippy and new-commit secret scanning passed. The existing Vite bundle-size warning remains.
- Browser coverage includes root edits, mapped-drive response and failure handling, stale responses, both languages, insertion positions, pointer and keyboard ordering, external drops, cancellation, persistence, failed acknowledgements and read-only state. Final suite results are recorded below after the completed run.
- A new concurrent-order Rust test initially had a closure-lifetime compile error; it was corrected before final checks. The initial browser full run passed 306/307; the new pointer-cancellation test reused screen coordinates while the mock watcher failure inserted a status banner above the tabs. Event traces confirmed the pointer hit the banner rather than a tab. The test now awaits that known fixture status before capturing coordinates, without changing the app. Waiting for badge classification alone did not fix this timing issue.

## Exact packaged native acceptance

The extracted final app ran with owned synthetic folders and an isolated persistent WebView2 profile (157.0.4341.46):

1. Actual system-drive API classification, UNC/WSL/unknown native responses and visible badges.
2. Pointer drag changed order through actual Rust IPC without changing the active project.
3. Repeated keyboard reordering preserved project identity.
4. Duplicate-ID requests were rejected with the catalog unchanged.
5. Normal close/restart retained order, active project and language preference.

Both language screenshots were inspected. Normal closes returned saved-state acknowledgements, left no owned processes, and recorded no runtime errors. Source sentinel contents were unchanged. User installation and system clipboard were untouched. The fixture displayed the existing session_missing recovery notice after restart: newly created projects had no changed workspace session yet, so loading correctly fell back to their presets. This behavior was confirmed in the previous source version too. Catalog order and selected project were separately asserted; reordering did not alter session files.

## Boundaries

The build PC has no mapped network drive. DRIVE_REMOTE has mocked API coverage; actual GetDriveTypeW was exercised on the system drive. No remote share or drive mapping was created for testing. Windows 11, physical Explorer input and the existing file-operation limitations are unchanged. This is exact-package native acceptance, not an installed-app upgrade test. No CI workflow result is claimed; validation ran on the Windows build machine.
