# ProjectPane 1.4.0 validation

Product commit: `f7c0deb5ea7a039ad7b8505348dd809db792d141`.

## Exact package

- Installer SHA256: `64334bee901483ed6c32d990973896ba8d15fd3af2cbfe1471ec62535ba594ad`.
- Packaged application SHA256: `e958aeb8ce38e26b7e9701a7b14d9470a0ec45a0570c0f3dfdc4b74dde981546`.
- Read-worker SHA256: `64bec3c1a60cce41b4197a7cdd748c834856e03985a1fc3707e74d5dd34620c3`.
- Frozen tracked-input digest: `c10090a03f9b0a12a49ff8ba5b4037c1c2b456339723a48c733cc51341828460` (572 files).

The NSIS payload matched the build outputs, allowing only Tauri's documented bundle-marker change. All 14 notice resources matched, with no duplicate destinations or unremapped local build paths. The updater signature test accepted the package and rejected tampered bytes, wrong version, missing signature and downgrade. Updater signing is separate from Windows Authenticode signing.

## Regression and native checks

- Rust: 441 passed (362 core, 28 desktop, 51 integration); 20 ignored and 7 name-filtered in the aggregate. Two filtered pure WSL tests passed separately; the five old native fixtures requiring hardcoded example-user paths were not run.
- Additional native tests passed for ADS-to-WSL failure preservation and actual Windows Shell IDataObject round-trips of 129/1025 Unicode files on local, loopback SMB and WSL paths, without changing the system clipboard.
- Vitest 424/424, Node/security 22/22, browser 267/267; typecheck, production build, formatting and strict Clippy passed.
- Installer guard 3/3 and maintenance 8/8 synthetic cases passed. Helpers exited and the uniquely owned registry fixture was removed. No actual installation or uninstallation was performed.

The exact packaged application ran on Windows 10 Pro 19045 with WebView2 157.0.4341.42 from 2026-11-09 06:56:06 to 07:02:03 UTC. Its native acceptance passed 17 assertion groups:

- 2051 files plus an empty directory, with all file hashes verified and 2055 paged processing records.
- 1025 top-level entries obtained through real native listing and bounded selection activation.
- Local NTFS and existing localhost/127.0.0.1 SMB aliases; ordinary Unicode files both Windows→WSL and WSL→Windows, plus copy-as.
- Zone.Identifier, Unicode-named, empty and multi-buffer ADS where supported; source preservation and no target-file creation when WSL cannot preserve ADS.
- Skip-existing integrity, >500-character source/destination paths, cancellation, and real app result display.
- Normal close during a 128-file copy: completed all files, stored the terminal report and saved acknowledgment, exited 0 in 14.333s, no remaining owned processes or runtime errors.
- Restart reopened 17 durable jobs and 2055 result records; second normal close exited 0 with saved acknowledgment and no remaining processes/errors.

The 2051-file check including hash/result verification took 194.890s. The maximum of 3258 status polls was 11.967ms. This does not establish UI frame latency or universal copy throughput. The controlled same-storage journal benchmark, documented separately, observed 14.2% lower mean elapsed time and 50% fewer journal barriers.

## Boundaries

Physical Explorer drag/drop and live Ctrl+C/V were not verified: native window capture/input and clipboard access were unavailable before a transfer could be sent. Real remote NAS behavior, power-loss recovery and actual installed-version upgrade are not certified. A symlink-creation fixture was blocked by the existing Windows privilege error 1314; no security settings were changed. Existing junction rejection tests passed.

ADS-bearing sources are explicitly refused when the destination cannot preserve streams. Reparse/symlink and other unsupported special files remain fail-closed; there is no silent content-only or link-following fallback. Existing targets are not overwritten. Partial/completed artifacts remain after cancellation/failure, and uncertain mutations are not automatically replayed. Per-request byte/depth/stream budgets and existing 100k-entry/32MiB listing budgets remain; unrelated move/archive limits are unchanged.

## Public release verification

[ProjectPane 1.4.0](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.4.0) was published as the latest release on 2026-11-09 at 07:04:39 UTC. The annotated v1.4.0 tag resolves to the product commit above. A later documentation-only commit does not change that tag or the released binaries.

All four assets (installer, updater signature, latest.json and SHA256SUMS.txt) were downloaded and matched byte-for-byte before publication through authenticated draft access and again anonymously after publication. The public latest feed identifies 1.4.0 and matches the verified local feed. The actual updater published-feed download test passed exactly 1/1 and compared its downloaded package with the verified installer without installing it.

The public repository has no Actions workflows; these are recorded local, native and public-download verification results, not a CI pass.
