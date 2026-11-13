# ProjectPane 1.5.2 validation

This patch adds Korean/English display language selection. Existing users retain Korean by default. The preference is local to the app WebView profile; switching does not reload the app or change project/file data. A blocked preference store produces a visible session-only warning.

## Validation scope

Frontend tests cover preference parsing, persistence failure, interpolation, translated errors, live notices, project/palette state and preservation of user names and paths. Browser checks cover both languages, restart/reload, file and project dialogs, error paths and compact layout. Exact package and publication results will be recorded after final checks.

## Boundaries

Windows shell menus, external application dialogs and server-provided release-note text retain their own language. Dates remain YYYY-MM-DD HH:mm in PC local time. No new storage providers or SFTP support is introduced. The existing recycle and move safety boundaries are unchanged. The user installation is not modified by validation.
