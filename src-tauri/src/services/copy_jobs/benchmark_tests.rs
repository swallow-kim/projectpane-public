//! Explicit synthetic benchmark, never runs in the ordinary test suite.
use super::*;
use std::time::{Duration, Instant};

#[derive(Serialize)]
struct Measurement {
    run: usize,
    mode: &'static str,
    elapsed_ms: f64,
    journal_syncs: u64,
    files_verified: usize,
    outcome_records: u64,
    persisted_status: CopyStatus,
}

fn failure(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}

fn payload(index: usize) -> Vec<u8> {
    format!("projectpane synthetic benchmark payload {index:06}\n")
        .repeat(4)
        .into_bytes()
}

fn completed(jobs: &CopyJobs, id: &str) -> Result<Snapshot, String> {
    let started = Instant::now();
    loop {
        let snapshot = jobs.snapshot(id).map_err(failure)?;
        if !snapshot.status.active() {
            return Ok(snapshot);
        }
        if started.elapsed() > Duration::from_secs(600) {
            let _ = jobs.cancel(id);
            return Err(format!(
                "benchmark timed out; cancellation requested for {id}"
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn run_mode(
    fixture: &Path,
    source: &Path,
    count: usize,
    legacy: bool,
    run: usize,
) -> Result<Measurement, String> {
    let mode = if legacy { "legacy_sync" } else { "batched" };
    let target = fixture.join(format!("target-{run:02}-{mode}"));
    let data = fixture.join(format!("journal-{run:02}-{mode}"));
    fs::create_dir(&target).map_err(failure)?;
    let jobs = CopyJobs::open(&data, None).map_err(failure)?;
    jobs.legacy_sync.store(legacy, Ordering::Release);
    let capture = jobs.begin_capture().map_err(failure)?;
    jobs.append(
        &capture.id,
        vec![source.to_str().ok_or("source path is not UTF-8")?.into()],
    )
    .map_err(failure)?;
    jobs.seal(&capture.id).map_err(failure)?;
    let before_syncs = jobs.journal_syncs.load(Ordering::Acquire);
    let timer = Instant::now();
    let started = jobs
        .begin(BeginJob {
            capture_id: capture.id,
            destination: target.to_str().ok_or("target path is not UTF-8")?.into(),
            project_id: None,
            conflict_policy: ConflictPolicy::Fail,
            name: None,
        })
        .map_err(failure)?;
    let terminal = completed(&jobs, &started.id)?;
    let elapsed_ms = timer.elapsed().as_secs_f64() * 1000.0;
    let journal_syncs = jobs
        .journal_syncs
        .load(Ordering::Acquire)
        .checked_sub(before_syncs)
        .ok_or("journal sync counter regressed")?;
    if terminal.status != CopyStatus::Succeeded
        || !terminal.journal_saved
        || terminal.failure.is_some()
        || terminal.roots_total != 1
        || terminal.roots_completed != 1
        || terminal.counts.changed != count as u64 + 2
        || terminal.counts.failed != 0
        || terminal.counts.unknown != 0
        || terminal.counts.skipped != 0
    {
        return Err(format!("{mode}: invalid terminal snapshot {terminal:?}"));
    }

    // Verify outside the timer. Both runs use the exact same source directory.
    let copied = target.join("source");
    for index in 0..count {
        let name = format!("item-{index:06}.txt");
        let expected = payload(index);
        if fs::read(source.join(&name)).map_err(failure)? != expected {
            return Err(format!("{mode}: source changed: {name}"));
        }
        if fs::read(copied.join(&name)).map_err(failure)? != expected {
            return Err(format!("{mode}: copied bytes differ: {name}"));
        }
    }
    for directory in [source, copied.as_path()] {
        let entries = fs::read_dir(directory)
            .map_err(failure)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(failure)?;
        if entries.len() != count {
            return Err(format!("{mode}: unexpected entry count at {directory:?}"));
        }
    }

    // The worker publishes its durable terminal state just before dropping its
    // Arc. Wait for that release, then prove a new service sees the same result.
    let release = Instant::now();
    while Arc::strong_count(&jobs) != 1 {
        if release.elapsed() > Duration::from_secs(5) {
            return Err(format!("{mode}: terminal worker did not release its lease"));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(jobs);
    let reopened = CopyJobs::open(&data, None).map_err(failure)?;
    let persisted = reopened.snapshot(&started.id).map_err(failure)?;
    if persisted.status != terminal.status
        || persisted.counts != terminal.counts
        || persisted.outcome_count != terminal.outcome_count
        || !persisted.journal_saved
    {
        return Err(format!("{mode}: reopened snapshot differs: {persisted:?}"));
    }
    let mut offset = 0;
    let mut records = 0u64;
    loop {
        let page = reopened
            .outcomes(&started.id, offset, 128)
            .map_err(failure)?;
        if page.failure.is_some() || (!page.done && page.next_offset <= offset) {
            return Err(format!("{mode}: incomplete journal outcome page"));
        }
        records += page.items.len() as u64;
        offset = page.next_offset;
        if page.done {
            break;
        }
    }
    if records != terminal.outcome_count {
        return Err(format!("{mode}: durable outcome count differs"));
    }
    Ok(Measurement {
        run,
        mode,
        elapsed_ms,
        journal_syncs,
        files_verified: count,
        outcome_records: records,
        persisted_status: persisted.status,
    })
}

#[test]
#[ignore = "requires explicit PROJECTPANE_COPY_BENCH_ROOT synthetic storage base"]
fn journal_phase_fusion_benchmark() {
    let base = PathBuf::from(std::env::var_os("PROJECTPANE_COPY_BENCH_ROOT").expect(
        "set PROJECTPANE_COPY_BENCH_ROOT to an existing absolute target/journal storage base",
    ));
    let source_base = std::env::var_os("PROJECTPANE_COPY_BENCH_SOURCE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| base.clone());
    for path in [&base, &source_base] {
        assert!(
            path.is_absolute() && path.is_dir(),
            "benchmark bases must already exist and be absolute"
        );
    }
    let count = std::env::var("PROJECTPANE_COPY_BENCH_COUNT")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("benchmark count must be an integer")
        })
        .unwrap_or(128)
        .clamp(1, 4096);
    let run_count = std::env::var("PROJECTPANE_COPY_BENCH_RUNS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("benchmark runs must be an integer")
        })
        .unwrap_or(6)
        .clamp(2, 6);
    let fixture = tempfile::Builder::new()
        .prefix("projectpane-journal-benchmark-target-")
        .tempdir_in(&base)
        .expect("create unique synthetic target/journal fixture");
    let source_fixture = tempfile::Builder::new()
        .prefix("projectpane-journal-benchmark-source-")
        .tempdir_in(&source_base)
        .expect("create unique synthetic source fixture");
    let result = (|| -> Result<serde_json::Value, String> {
        let source = source_fixture.path().join("source");
        fs::create_dir(&source).map_err(failure)?;
        for index in 0..count {
            fs::write(source.join(format!("item-{index:06}.txt")), payload(index))
                .map_err(failure)?;
        }
        let sequence = [true, false, false, true, true, false];
        let mut measurements = Vec::with_capacity(run_count);
        for (run, legacy) in sequence.into_iter().take(run_count).enumerate() {
            measurements.push(run_mode(fixture.path(), &source, count, legacy, run + 1)?);
        }
        let legacy: Vec<_> = measurements
            .iter()
            .filter(|m| m.mode == "legacy_sync")
            .collect();
        let batched: Vec<_> = measurements
            .iter()
            .filter(|m| m.mode == "batched")
            .collect();
        if batched
            .iter()
            .any(|b| legacy.iter().any(|l| b.journal_syncs >= l.journal_syncs))
        {
            return Err("expected fewer writer syncs for every batched run".into());
        }
        let legacy_ms: f64 = legacy.iter().map(|m| m.elapsed_ms).sum();
        let batched_ms: f64 = batched.iter().map(|m| m.elapsed_ms).sum();
        let legacy_syncs: u64 = legacy.iter().map(|m| m.journal_syncs).sum();
        let batched_syncs: u64 = batched.iter().map(|m| m.journal_syncs).sum();
        let legacy_mean = legacy_ms / legacy.len() as f64;
        let batched_mean = batched_ms / batched.len() as f64;
        let legacy_mean_syncs = legacy_syncs as f64 / legacy.len() as f64;
        let batched_mean_syncs = batched_syncs as f64 / batched.len() as f64;
        Ok(serde_json::json!({
            "schema": 2,
            "benchmark": "copy_job_journal_phase_fusion",
            "source_storage_base": source_base,
            "target_and_journal_storage_base": base,
            "files_per_run": count,
            "order": measurements.iter().map(|m| m.mode).collect::<Vec<_>>(),
            "same_source": true,
            "fresh_target_and_journal_per_run": true,
            "cold_cache_controlled": false,
            "cache_caveat": "same source reused; OS/storage caches are not flushed; alternating modes reduces but does not eliminate order effects",
            "timing_scope": "begin through durable terminal; capture setup and byte/recovery verification excluded",
            "sync_scope": "job log writer only",
            "measurements": measurements,
            "totals": {
                "legacy_sync": {"runs": legacy.len(), "elapsed_ms": legacy_ms, "mean_elapsed_ms": legacy_mean, "journal_syncs": legacy_syncs},
                "batched": {"runs": batched.len(), "elapsed_ms": batched_ms, "mean_elapsed_ms": batched_mean, "journal_syncs": batched_syncs}
            },
            "observed_mean_speedup": legacy_mean / batched_mean.max(0.000_001),
            "mean_sync_reduction_percent": 100.0 * (1.0 - batched_mean_syncs / legacy_mean_syncs),
        }))
    })();
    match result {
        Ok(report) => println!("{report}"),
        Err(error) => {
            let preserved_target = fixture.keep();
            let preserved_source = source_fixture.keep();
            panic!(
                "benchmark failed: {error}; preserved source fixture: {preserved_source:?}; preserved target/journal fixture: {preserved_target:?}"
            );
        }
    }
}
