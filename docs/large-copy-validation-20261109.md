# Large-copy and responsiveness validation (2026-11-09)

## Scope and provenance

Local implementation on `codex/reliable-large-transfers-20261109`, based on public ProjectPane 1.3.0 commit `d809ee8e61a4c3c43d08574e1d28c7e00cf43690`. The older private checkout and its unrelated changes were not modified. The initial validation below was performed before release preparation, without committing, publishing, installing, or changing the interactive clipboard. Later 1.4.0 release work is recorded separately below.

Native verification uses an uninstalled release executable and matching read worker on Windows 10 Pro 19045. Fixtures, application data, and WebView profiles are isolated. C/D drives, existing loopback SMB shares, and WSL Ubuntu are exercised without creating shares or changing security settings.

## Implementation

- Ordinary copy, copy-as, skip-existing, native paste/drop, and copy export use a copy-only capture/job API. Captures are disk-backed. IPC batches are bounded by count and bytes; long resolved paths are split adaptively. There is no 128/1024 total-copy-root limit or 2048 descendant-result limit in this path.
- Directory traversal is incremental. A bounded buffer and live native handles replace whole-tree plans/results. Results are durable, byte-bounded pages, with progress and cancellation.
- New destinations and named streams use create-new semantics. Existing targets are never overwritten. Source/destination identities and pinned namespaces are checked. Cancellation keeps completed and partial artifacts and never deletes the source or retries a potentially accepted job automatically.
- NTFS DATA alternate streams, including downloaded files' Zone.Identifier, are preserved when the destination supports named streams. Source stream handles deny writes/deletes while copied. Target streams use FILE_CREATE, not replace/open-if. Exact source and destination stream inventories are checked. Basic timestamps and ordinary attributes are preserved.
- Headers are atomically published. Malformed prior history is isolated rather than disabling new copies. Torn result pages preserve their valid prefix and identify unconfirmed destinations. Restart does not replay interrupted mutations. Application close waits for active copies, with cancel/continue support.
- Counts labelled processing records are journal outcomes, not distinct filesystem objects. Directories have separate creation and final-metadata records. Selected-root totals remain item counts.
- UI filter input is immediate with 120 ms coalescing and IME/stale-response guards. ASCII natural sorting avoids repeated lowercase allocation, with the original Unicode fallback. Bounded batch activation scans a snapshot once instead of once per selected entry.

## Completed frontend and listing checks

- Browser regression suite: 267/267 passed, one worker.
- Vitest: 420/420 passed across 38 files.
- Node/security scripts: 22/22 passed, including the explicit local-main command/permission list.
- Worker focused tests: 20 passed, 2 benchmark tests ignored in the ordinary run.
- Core/view integration: core_contracts 7, view_contracts 2, views 4 passed, including actual broker batch IPC, ordering, count bounds, and stale generations.

Commands include `npm test`, `npm run test:browser -- --workers=1`, and `cargo test --manifest-path src-tauri/Cargo.toml --test core_contracts --test view_contracts --test views`.

## Measured listing changes

Release-mode microbenchmarks use 100,000 synthetic names, three samples per case, and assert equivalent output:

| Operation | Previous median | New median |
| --- | ---: | ---: |
| Short ASCII natural sort | 443.20 ms | 126.09 ms |
| Long common-prefix ASCII sort | 1817.75 ms | 817.60 ms |
| Short ASCII name filter | 9.30 ms | 4.90 ms |
| Resolve last 128 selections in 100k snapshot, same native metadata checks | 189.04 ms | 8.74 ms |

Long-name filtering keeps its previous algorithm after an experimental alternative regressed. These numbers exclude directory enumeration, IPC round trips, UI rendering, and copy throughput. No end-to-end speed multiplier or memory-regression claim is made.

Reproduce with release ignored tests `benchmark_ascii_listing` and `benchmark_activation_batch`. Detailed measurements are retained in `artifacts/listing-responsiveness-results-20261109.md`.

## Native validation

Final desktop/all-target Rust regression passed: 432 tests (353 core, 28 desktop, 51 integration), 18 explicitly ignored, and 7 old hardcoded-WSL fixtures filtered. Custom slow-progress and worker-lifetime harnesses also passed. Strict desktop/all-target Clippy with `-D warnings`, `cargo fmt --check`, and `git diff --check` passed. Logs are in `test-results/copy-backend-final-tests.log` and `test-results/copy-backend-final-clippy.log`. The following executor checks also passed:

- Actual native service copied 1025 roots and reopened its durable report.
- Native executor copied 2051 descendants and preserved ordinary metadata.
- Controlled provider fixtures passed C to D, WSL to D, D to WSL, D to loopback SMB, and loopback SMB to C.
- WSL to WSL copied distinct Foo.txt and foo.txt with exact names and different verified SHA256 contents.
- Focused ADS tests passed for Zone.Identifier, Unicode/multi-buffer/empty streams, directory ADS, collision refusal, source locking, and stream-set changes. The final aggregate includes the added long-path, batching, and recovery checks.

The first pre-ADS application run used its durable journal on D. Its 180-second test deadline and 40-second close-observer deadline expired. The copy itself eventually succeeded after about 382.2 seconds; all 2051 file hashes and 2055 durable processing records were verified. The app was subsequently no longer running, but the timed-out harness did not capture its later native exit code. This is retained as a failed acceptance attempt, not counted as a full pass. It shows that per-record durable flushing can dominate on a slower or contended data drive. The separate ADS acceptance run uses isolated C-drive application data. Final large-copy acceptance retained D-drive journal storage, used explicit longer workload deadlines, and verified grouped durability barriers. The initial result is not attributed to one cause without controlled comparison.

Evidence: `artifacts/large-copy-20261109/run-kZoJUC/report.json` and `baseline-follow-up.json`.

## Controlled journal-batching comparison

Results and RootDone records share the next intent's durability barrier. The next mutation cannot begin before that sync succeeds. Counts and active result pages expose only the committed byte prefix. Final completion always syncs. Any append/sync failure poisons the writer, stops mutation and continuation writes, preserves the last durable prefix, and reports an unsaved/unknown result. Outcome-only jobs checkpoint by count/bytes or elapsed time when the next record arrives; the elapsed threshold is not a timer guarantee during blocked provider IO.

A six-run actual CopyJobs benchmark used the same C-drive source with fresh D-drive targets and D-drive journals, 128 files per run, alternating legacy/batched/batched/legacy/legacy/batched. All 768 copied files and original source bytes were checked; every job was reopened and its persisted terminal state and outcome pages verified.

| Policy | Three run times | Mean | Journal barriers per run |
| --- | --- | ---: | ---: |
| Sync every record | 14.977 s, 13.496 s, 13.428 s | 13.967 s | 262 |
| Group results with next intent | 11.986 s, 11.991 s, 11.966 s | 11.981 s | 131 |

Observed mean elapsed reduction: 14.2%. Journal barriers: 50% fewer. Timing covers job begin through durable terminal state, excludes capture setup and later byte/recovery verification, and uses reused source/OS caches. This is not a cold-cache or universal copy-throughput claim. No compiler or app workload overlapped the measurement. Evidence: `test-results/copy-batching-benchmark.log`; reproduce the ignored `journal_phase_fusion_benchmark` with explicit synthetic source/target roots.
## Additional actual-app ADS acceptance

The uninstalled ADS-enabled release (SHA256 `bf093b8c9a61736483b6685e88324768f6fa790c72fb22a6fb43f93af1c05d59`) passed real native IPC checks for primary bytes plus Zone.Identifier, Unicode-named, empty, and multi-buffer ADS on NTFS and both existing localhost/127.0.0.1 SMB aliases. An ADS-unsupported WSL destination failed before creating the primary file, with the source unchanged. Ordinary Unicode files passed Windows-to-WSL, WSL-to-Windows and copy-as checks. Normal close produced a saved-project acknowledgment, exit code 0, zero remaining owned processes, and no runtime errors. This run took about 19 seconds and did not touch the interactive clipboard.

Evidence: `artifacts/large-copy-20261109/ads-run-14B4Up/report.json` and `native-ads.png`. This binary predates the long-path and journal-batching revisions; the final acceptance run remains separate.

## Feature-build executable acceptance before the 1.4.0 version bump

The final release was built with `npm run prepare:worker -- --release` followed by `npm run tauri -- build --no-bundle --no-sign --ci`. No installer was built or run. Node 24.19.0, Rust 1.98.1, and WebView2 157.0.4341.42 were used. The frozen 392 source/build-input hashes were verified again after acceptance with no differences.

- Application SHA256: `895daed6878386f620b894bdfec8415b9fab69c40261df7e3c02b29b09253ba7`.
- Read-worker SHA256: `f2fbef50ed63771c59c3522f9ea7c081babf70956f105e57af987459581de031`.
- Combined frozen-source SHA256: `1409c887230b49033b430c4c3532095cbbf6f2b519ef095082bc8caae6330f82`.

The final native workload ran 2026-11-09 06:08:03–06:13:49 UTC with both targets and durable journals on D. It verified every hash for 2051 files plus an empty directory, 2055 paged processing records, and 1025 separately selected roots. The 1025-root test obtained real entry IDs from the native listing and used batched native activation, rather than bypassing selection with raw paths. Each native outcome page was at most 128 records and below 256 KiB. The 2051-file check, including result paging and hash verification, took 196.043 seconds. This remains a substantial cost for many small files on this drive; its difference from the earlier 382-second run is not attributed solely to batching. Only the controlled A/B above supports the 14.2% measured improvement.

The same final binary passed primary-data and ADS checks on local NTFS and both loopback SMB aliases, ordinary Unicode file copies in both WSL directions and copy-as, skip-existing preservation, ADS-unsupported WSL refusal before target creation, source preservation, >500-character source/destination paths with ADS, and cancellation. Across 3249 native status polls, the maximum observed poll time was 11.446 ms. This measures status IPC, not frame timing or general UI latency.

The workload's original acceptance harness then used an incorrect results-button label. That report is preserved as failed; no product change was made to accommodate it. A focused follow-up initially assumed 128 UI rows, whereas the actual UI intentionally requests 16. That failed harness report is also retained. After correcting those two harness assumptions, the same application and worker hashes reopened the original D-drive history, displayed its 16 jobs, and successfully switched between real 16-row pages from the 2055-record job. The focused run then requested normal native close during a fresh 128-file copy. Close waited for completion, all 128 file hashes matched, the terminal journal was durable, the process exited with code 0, a saved-project acknowledgment was received, and zero owned processes or runtime errors remained. The normal-close interval was 14.105 seconds. The focused run completed 06:18:32 UTC.

Evidence, all under `artifacts/large-copy-20261109/`:
- `run-ADQvil/report.json`: completed native filesystem assertions and preserved failed UI-harness outcome.
- `ui-close-run-SNxHFs/report.json`: preserved incorrect UI-page-size assertion.
- `ui-close-run-xf7RTJ/report.json`: passed native restart/history/page-navigation/active-copy-close checks.
- `ui-close-run-xf7RTJ/native-results.png`: reviewed screenshot of actual native result pagination.
- `final-source-manifest.json`: frozen source/build-input provenance.

These combined results complete the intended native checks on this machine; they do not convert either failed harness report into a pass. The interactive OS clipboard was not touched, and no physical Explorer drag was simulated. Browser regressions cover the modeled Ctrl+C/V and drag/drop integration, while the native runs cover the real copy and selection IPC.

## Explicit boundaries

- Copying an ADS-bearing source to a provider that cannot preserve named streams fails before creating that file; streams are not silently stripped. Ordinary files remain eligible on such providers.
- Reparse/symlink, encrypted, sparse, compressed, and other unsupported attributes remain fail-closed. This is not a promise to clone ACLs, EAs, hard-link topology, or an atomic snapshot of a changing tree.
- Safety budgets remain: native clipboard/drag payload bytes, per-packet/result bytes, traversal depth, and at most 128 named-stream handles / 64 KiB stream names per individual file. These are not a 1000-file operation cap.
- Existing 100k-entry/32 MiB listing snapshot budgets and unrelated move/archive constraints are unchanged.
- Real remote NAS/server behavior, physical Explorer drag/drop and the interactive clipboard, installer lifecycle, abrupt process/power-loss behavior, and broad memory benchmarks are not certified by these tests. Loopback SMB is labelled as such.
## 1.4.0 release follow-up

After the initial validation, release preparation was authorized. All package versions were raised to 1.4.0 and license inventories regenerated. The safe copy policy remains unchanged: ADS-incompatible destinations and reparse entries are rejected explicitly rather than silently losing information or following links. A dedicated destination-stream diagnostic and clearer reparse guidance were added.

Native clipboard acquisition and final rendering now retry only clipboard-open contention, with at most four delays totaling 150 ms. A changed or unavailable clipboard sequence, lost ownership, or any other error stops retry. Setting the clipboard remains single-attempt, and a failed rendering step never repeats that write. This is not a bound on a blocking native OLE call.

Physical Explorer transfer tests were prepared with uniquely owned 129/1025-file fixtures, but no transfer input could be sent. Native window capture and input remained unavailable after a visible relaunch; clipboard reading also failed without exposing or changing its contents. Therefore physical Explorer drag/drop and live Ctrl+C/V in either direction remain unverified, including their SMB/WSL combinations. Test preparation is not a passing interaction result. The isolated idle test app was stopped after verifying that it had no active or persisted copy jobs.

A non-disruptive native Shell IDataObject round-trip test passed for 129/1025 Unicode file selections, using the real Windows Shell encoder and both CF_HDROP decoders without setting the system clipboard. Its result is distinct from physical-input acceptance. The final ADS-to-WSL fixture also passed with the precise failed child, unchanged primary/Zone.Identifier bytes, and no created target file. A symlink-creation fixture was blocked by the existing Windows privilege error 1314; no security setting was changed. Final 1.4.0 regression passed:441 Rust tests (362 core, 28 desktop, 51 integration), 20 ignored and 7 name-filtered in the aggregate; two filtered pure WSL tests and the ADS-to-WSL diagnostic fixture passed separately. Only the five old hardcoded native WSL fixtures remain unexecuted from that name filter. Custom slow-progress/worker-lifetime harnesses, formatting and strict Clippy passed. Frontend validation passed 424 Vitest tests, 22 Node/security tests, 267 browser tests, typecheck and production build. Synthetic installer checks passed 3 guard and 8 maintenance cases without installing/uninstalling the app or touching its actual settings.

The real Shell data-object round-trip also passed 129/1025 Unicode files on WSL and loopback SMB using isolated per-process temporary roots. The SMB fixture used canonical lowercase d$ after the Shell normalized uppercase D$; that initial strict string-comparison failure is retained. These tests did not access the global clipboard. Final signed package and publication evidence follows separately.

## Final signed package and publication

The final 1.4.0 package at product commit `f7c0deb5ea7a039ad7b8505348dd809db792d141` passed 17 native acceptance groups using its extracted app and worker. The signed installer was [published](https://github.com/swallow-kim/projectpane-public/releases/tag/v1.4.0) on 2026-11-09 at 07:04:39 UTC. All four anonymous asset downloads matched the staged bytes, the latest feed returned 1.4.0, and the actual updater public-feed test passed 1/1. Exact hashes, final package tests and remaining limits are recorded in [1.4.0 release validation](release-1.4.0-validation.md). Earlier attempts and limitations above remain part of the record.
