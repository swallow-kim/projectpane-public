# ProjectPane 1.5.2 validation

Product commit and tag target: `77d3c11764bd59c61b7e1ac19c15463203159472` (`v1.5.2`). Built from clean committed source on 2026-11-13. Subsequent documentation-only commits do not change the package.

## Feature and compatibility

Korean/English display language selection is available in the header. Existing users retain Korean by default. The preference is local to the app WebView profile; switching does not reload the app or change project/file data. A blocked preference store produces a visible session-only warning. Menus, dialogs, errors, statuses and accessibility labels react to the selected language, including errors already displayed.

## Exact package

- Installer SHA256: `7511dc2daddfdf7f669e3cfe1ce6b7c5bf183b511f28a389cc5503bb42e2010c`.
- Packaged app SHA256: `d641c891b57e26c4f66df668a0cc414e1fdcdf719095b4c8d22fd4b75defd594`.
- Read-worker SHA256: `96de8c3ddd486dc71a4ebd7b6875030128d45b54e1236baf71edbe1385486768`.

The extracted NSIS payload matched the compiled binaries, allowing only Tauri’s exact bundle marker. All 14 notice resources and installed relative links passed. No duplicate destinations or unremapped local build paths were found. The actual updater accepted the signed package and rejected tampering, wrong version and missing signature. Updater signing is separate from Windows Authenticode signing.

## Regression checks

- Vitest: 515/515 passed. Node/security: 22/22 passed.
- Browser regression: 292/292 passed, including 13 language-focused checks.
- Rust library: 372 passed, 21 ignored. Desktop binary: 28 passed, 4 ignored. Explicit signature/package checks are separate.
- Typecheck, production build, Rust formatting, strict Clippy and new-commit secret scan passed. The existing Vite bundle-size warning remains.
- Language browser checks cover immediate switching and reload, invalid preferences, blocked storage, project creation/editing, Unicode input preservation, live errors, favorites, updates, open palette search, copy fallback, recycle cancellation and compact layout.
- Narrow-window regression exposed a header overflow; wrapping and bounded child layout fixed it. The final full suite passed. A packaged license link mismatch was fixed and rebuilt before publication. Superseded builds were not published.

## Exact packaged native acceptance

The extracted final package ran with isolated owned settings and a persistent test WebView2 profile (157.0.4341.46). Six assertion groups passed:

1. Korean default, immediate English UI, and unchanged Korean file names.
2. English folder-recycle preview and cancellation without modifying the source.
3. English project creation preserving Unicode project name and root.
4. Normal close/restart preserving English and project state.
5. Existing native folder-not-found errors changing language without another operation.
6. Clean normal closes with saved-state acknowledgements, no owned processes remaining, no runtime errors and unchanged source sentinel contents.

Both language screenshots were inspected. The initial test helper used a fresh WebView profile on every launch, which cannot test profile-local preference persistence. The final helper reuses an isolated owned profile across normal restarts. A duplicate test-library import in that helper was also corrected. Neither was a product persistence failure. The user installation and system clipboard were untouched.

## Publication

All four draft assets were downloaded and hash-matched before publication. Anonymous downloads of installer, signature, latest.json and SHA256SUMS.txt matched staged bytes. The latest-release feed matches 1.5.2. The actual updater independently downloaded the published package and matched the verified local installer (1 passed), without installing it. No CI workflow result is claimed; checks ran on the Windows build machine.

[Release and downloads](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.5.2).

## Boundaries

Windows shell menus, external application dialogs and server-provided release-note text retain their own language. Dates remain YYYY-MM-DD HH:mm in PC local time. No new storage providers or SFTP support is introduced. Existing recycle and move safety boundaries are unchanged. This was not a live installed-app upgrade; native acceptance used the exact extracted final package.
