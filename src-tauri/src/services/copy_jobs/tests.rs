use super::*;
use std::{
    io::Write,
    time::{Duration, Instant},
};
fn fixture() -> (tempfile::TempDir, Arc<CopyJobs>, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    fs::create_dir(&source).unwrap();
    fs::create_dir(&target).unwrap();
    let jobs = CopyJobs::open(&temp.path().join("data"), None).unwrap();
    (temp, jobs, source, target)
}
fn wait(jobs: &CopyJobs, id: &str) -> Snapshot {
    let start = Instant::now();
    loop {
        let snapshot = jobs.snapshot(id).unwrap();
        if !snapshot.status.active() {
            return snapshot;
        }
        assert!(start.elapsed() < Duration::from_secs(180));
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[test]
fn copy_1025_roots_with_bounded_capture_pages_and_reopen() {
    let (temp, jobs, source, target) = fixture();
    let mut paths = Vec::new();
    for n in 0..1025 {
        let path = source.join(format!("item-{n}.txt"));
        fs::write(&path, b"owned synthetic bytes").unwrap();
        paths.push(path.to_str().unwrap().to_owned());
    }
    let capture = jobs.begin_capture().unwrap();
    for chunk in paths.chunks(128) {
        jobs.append(&capture.id, chunk.to_vec()).unwrap();
    }
    assert_eq!(jobs.seal(&capture.id).unwrap().count, 1025);
    let started = jobs
        .begin(BeginJob {
            capture_id: capture.id.clone(),
            destination: target.to_str().unwrap().into(),
            project_id: None,
            conflict_policy: ConflictPolicy::Fail,
            name: None,
        })
        .unwrap();
    jobs.discard_capture(&capture.id).unwrap();
    let complete = wait(&jobs, &started.id);
    assert_eq!(
        complete.status,
        CopyStatus::Succeeded,
        "{:?}",
        complete.failure
    );
    assert_eq!(complete.counts.changed, 1025);
    let mut offset = 0;
    let mut count = 0;
    loop {
        let page = jobs.outcomes(&started.id, offset, 128).unwrap();
        assert!(page.items.len() <= 128);
        assert!(page.next_offset > offset || page.done);
        offset = page.next_offset;
        count += page.items.len();
        if page.done {
            break;
        }
    }
    assert_eq!(count, 1025);
    assert_eq!(
        fs::read(target.join("item-1024.txt")).unwrap(),
        b"owned synthetic bytes"
    );
    drop(jobs);
    let reopened = CopyJobs::open(&temp.path().join("data"), None).unwrap();
    assert_eq!(reopened.snapshot(&started.id).unwrap().counts.changed, 1025);
}
#[test]
fn capture_lifecycle_releases_more_than_32_tokens() {
    let (_temp, jobs, source, _) = fixture();
    let path = source.join("one");
    fs::write(&path, b"test").unwrap();
    for _ in 0..40 {
        let capture = jobs.begin_capture().unwrap();
        jobs.append(&capture.id, vec![path.to_str().unwrap().into()])
            .unwrap();
        jobs.seal(&capture.id).unwrap();
        jobs.discard_capture(&capture.id).unwrap();
    }
}
#[test]
fn recovery_reports_unmatched_intent_without_replaying() {
    let (temp, jobs, source, target) = fixture();
    let path = source.join("one");
    fs::write(&path, b"test").unwrap();
    let id = "synthetic-interrupted".to_owned();
    let header = Header {
        schema: 1,
        id: id.clone(),
        capture: Capture {
            id: "synthetic-capture".into(),
            count: 1,
            sealed: true,
        },
        destination: capture_source(target.to_str().unwrap()).unwrap(),
        project_id: None,
        conflict_policy: ConflictPolicy::Fail,
        name: None,
    };
    let mut head = open_file(&jobs.directory.join(format!("{id}.header")), true).unwrap();
    write_record(&mut head, &header).unwrap();
    drop(head);
    let mut journal = open_file(&jobs.directory.join(format!("{id}.journal")), true).unwrap();
    write_record(
        &mut journal,
        &Event::Intent {
            source: path.to_str().unwrap().into(),
            destination: target.join("one").to_str().unwrap().into(),
        },
    )
    .unwrap();
    drop(journal);
    drop(jobs);
    let reopened = CopyJobs::open(&temp.path().join("data"), None).unwrap();
    let snapshot = reopened.snapshot(&id).unwrap();
    assert_eq!(snapshot.status, CopyStatus::InterruptedUnknown);
    assert_eq!(snapshot.counts.unknown, 1);
    let page = reopened.outcomes(&id, 0, 128).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].status, ItemStatus::Unknown);
    assert!(page.done);
    assert!(!target.join("one").exists());
}
#[test]
fn corrupt_terminal_page_fails_instead_of_looping() {
    let (_temp, jobs, _source, target) = fixture();
    let header = Header {
        schema: 1,
        id: "torn".into(),
        capture: Capture {
            id: "capture".into(),
            count: 1,
            sealed: true,
        },
        destination: capture_source(target.to_str().unwrap()).unwrap(),
        project_id: None,
        conflict_policy: ConflictPolicy::Fail,
        name: None,
    };
    let mut journal = open_file(&jobs.directory.join("torn.journal"), true).unwrap();
    journal.write_all(b"{\"event\":").unwrap();
    drop(journal);
    assert!(!recover(&jobs.directory, &header).unwrap().journal_saved);
    let page = read_page(&jobs.directory, "torn", 0, 128, true).unwrap();
    assert!(page.done && page.failure.is_some());
}
#[test]
fn no_mutation_after_closed_and_invalid_capture_chunks() {
    let (_temp, jobs, source, _) = fixture();
    let capture = jobs.begin_capture().unwrap();
    assert!(
        jobs.append(&capture.id, vec![source.to_str().unwrap().into(); 129])
            .is_err()
    );
    jobs.close(true).unwrap();
    assert!(
        jobs.append(&capture.id, vec![source.to_str().unwrap().into()])
            .is_err()
    );
    assert!(jobs.begin_capture().is_err());
    jobs.keep_open().unwrap();
    assert!(jobs.begin_capture().is_ok());
}
#[test]
fn result_pages_are_bounded_by_bytes_as_well_as_items() {
    let (_temp, jobs, _source, _target) = fixture();
    let mut file = open_file(&jobs.directory.join("large-results.journal"), true).unwrap();
    for n in 0..8 {
        write_record(
            &mut file,
            &Event::Outcome {
                item: ItemOutcome {
                    source: format!("C:\\{}-{n}", "s".repeat(30000)),
                    destination: Some(format!("D:\\{}-{n}", "d".repeat(30000))),
                    status: ItemStatus::Changed,
                    hresult: 0,
                },
            },
        )
        .unwrap();
    }
    drop(file);
    let mut count = 0;
    let mut offset = 0;
    loop {
        let page = read_page(&jobs.directory, "large-results", offset, 128, true).unwrap();
        assert!(serde_json::to_vec(&page).unwrap().len() < 256 * 1024);
        assert!(page.next_offset > offset || page.done);
        count += page.items.len();
        offset = page.next_offset;
        if page.done {
            break;
        }
    }
    assert_eq!(count, 8);
}
#[test]
fn persisted_header_rejects_trailing_data() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("header");
    fs::write(
        &path,
        b"{\"id\":\"test\",\"count\":1,\"sealed\":true}\n{}\n",
    )
    .unwrap();
    assert!(read_one::<Capture>(&path).is_err());
}

#[test]
fn torn_header_is_diagnostic_not_a_global_copy_blocker() {
    let (temp, jobs, _, _) = fixture();
    fs::write(jobs.directory.join("torn-header.header"), b"{").unwrap();
    drop(jobs);
    let reopened = CopyJobs::open(&temp.path().join("data"), None).unwrap();
    let diagnostic = reopened.snapshot("torn-header").unwrap();
    assert_eq!(diagnostic.status, CopyStatus::InterruptedUnknown);
    assert!(!diagnostic.journal_saved);
    assert!(diagnostic.failure.is_some());
    assert!(reopened.begin_capture().is_ok());
}
#[test]
fn torn_outcome_preserves_valid_prefix_and_pending_unknown() {
    let (_temp, jobs, _, _) = fixture();
    let mut file = open_file(&jobs.directory.join("partial.journal"), true).unwrap();
    write_record(
        &mut file,
        &Event::Outcome {
            item: ItemOutcome {
                source: "C:\\a".into(),
                destination: Some("D:\\a".into()),
                status: ItemStatus::Changed,
                hresult: 0,
            },
        },
    )
    .unwrap();
    write_record(
        &mut file,
        &Event::Intent {
            source: "C:\\b".into(),
            destination: "D:\\b".into(),
        },
    )
    .unwrap();
    file.write_all(b"{bad").unwrap();
    drop(file);
    let page = read_page(&jobs.directory, "partial", 0, 128, true).unwrap();
    assert!(page.done && page.failure.is_some());
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].status, ItemStatus::Changed);
    assert_eq!(page.items[1].status, ItemStatus::Unknown);
}
#[test]
fn resolved_long_paths_are_spooled_in_byte_bounded_chunks() {
    let (_temp, jobs, mut source, _) = fixture();
    for n in 0..14 {
        source = source.join(format!("{n}-{}", "long-folder-".repeat(5)));
    }
    fs::create_dir_all(&source).unwrap();
    let mut paths = Vec::new();
    for n in 0..128 {
        let path = source.join(format!("file-{n}.txt"));
        fs::write(&path, b"synthetic").unwrap();
        paths.push(path.to_str().unwrap().to_owned());
    }
    assert!(paths.iter().map(String::len).sum::<usize>() > 65536);
    let capture = jobs.begin_capture().unwrap();
    assert_eq!(jobs.append_bounded(&capture.id, paths).unwrap().count, 128);
    jobs.discard_capture(&capture.id).unwrap();
}
#[test]
fn active_pages_exclude_staged_outcomes_and_failure_prefix_keeps_unknown() {
    let (_temp, jobs, _source, target) = fixture();
    let id = "watermark";
    let header = Header {
        schema: 1,
        id: id.into(),
        capture: Capture {
            id: "capture".into(),
            count: 2,
            sealed: true,
        },
        destination: capture_source(target.to_str().unwrap()).unwrap(),
        project_id: None,
        conflict_policy: ConflictPolicy::Fail,
        name: None,
    };
    let mut file = open_file(&jobs.directory.join("watermark.journal"), true).unwrap();
    let mut log = writer::JobLog::new(&mut file, false).unwrap();
    log.intent("source-a", "target-a").unwrap();
    let first = log.durable_end();
    log.result(&Event::Outcome {
        item: ItemOutcome {
            source: "source-a".into(),
            destination: Some("target-a".into()),
            status: ItemStatus::Changed,
            hresult: 0,
        },
    })
    .unwrap();
    let page = read_page_until(&jobs.directory, id, 0, 128, false, Some(first)).unwrap();
    assert!(page.items.is_empty() && !page.done);
    assert_eq!(page.next_offset, 0);
    log.intent("source-b", "target-b").unwrap();
    let committed = log.durable_end();
    log.result(&Event::Outcome {
        item: ItemOutcome {
            source: "source-b".into(),
            destination: Some("target-b".into()),
            status: ItemStatus::Changed,
            hresult: 0,
        },
    })
    .unwrap();
    let active = read_page_until(&jobs.directory, id, 0, 128, false, Some(committed)).unwrap();
    assert_eq!(active.items.len(), 1);
    assert_eq!(active.items[0].source, "source-a");
    let terminal = read_page_until(&jobs.directory, id, 0, 128, true, Some(committed)).unwrap();
    assert!(terminal.done);
    assert_eq!(terminal.items.len(), 2);
    assert_eq!(terminal.items[1].status, ItemStatus::Unknown);
    let recovered = recover_until(&jobs.directory, &header, Some(committed)).unwrap();
    assert_eq!(recovered.counts.changed, 1);
    assert_eq!(recovered.counts.unknown, 1);
    assert_eq!(recovered.roots_completed, 0);
}
#[test]
fn finished_cannot_claim_success_for_failed_or_incomplete_evidence() {
    let (_temp, jobs, _source, target) = fixture();
    let header = Header {
        schema: 1,
        id: "contradiction".into(),
        capture: Capture {
            id: "capture".into(),
            count: 1,
            sealed: true,
        },
        destination: capture_source(target.to_str().unwrap()).unwrap(),
        project_id: None,
        conflict_policy: ConflictPolicy::Fail,
        name: None,
    };
    let mut file = open_file(&jobs.directory.join("contradiction.journal"), true).unwrap();
    write_record(
        &mut file,
        &Event::Outcome {
            item: ItemOutcome {
                source: "source".into(),
                destination: None,
                status: ItemStatus::Failed,
                hresult: i32::MIN,
            },
        },
    )
    .unwrap();
    write_record(&mut file, &Event::RootDone).unwrap();
    let mut claimed = initial(&header);
    claimed.status = CopyStatus::Succeeded;
    claimed.counts.failed = 1;
    claimed.outcome_count = 1;
    claimed.roots_completed = 1;
    write_record(&mut file, &Event::Finished { snapshot: claimed }).unwrap();
    drop(file);
    let recovered = recover(&jobs.directory, &header).unwrap();
    assert_eq!(recovered.status, CopyStatus::InterruptedUnknown);
    assert!(!recovered.journal_saved);
    assert_eq!(recovered.counts.failed, 1);
}

#[test]
fn unsafe_numeric_header_is_isolated_before_listing() {
    let (temp, jobs, _, target) = fixture();
    let header = Header {
        schema: 1,
        id: "bad-count".into(),
        capture: Capture {
            id: "capture".into(),
            count: 9_007_199_254_740_992,
            sealed: true,
        },
        destination: capture_source(target.to_str().unwrap()).unwrap(),
        project_id: None,
        conflict_policy: ConflictPolicy::Fail,
        name: None,
    };
    publish_header(&jobs.directory, &header).unwrap();
    drop(jobs);
    let reopened = CopyJobs::open(&temp.path().join("data"), None).unwrap();
    let value = reopened.snapshot("bad-count").unwrap();
    assert_eq!(value.roots_total, 0);
    assert_eq!(value.status, CopyStatus::InterruptedUnknown);
    assert!(!value.journal_saved);
    assert!(reopened.begin_capture().is_ok());
}
