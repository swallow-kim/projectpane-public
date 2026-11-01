use projectpane_core::{
    domain,
    persistence::{BoundedLogger, EventKind, LogEvent},
};
use std::{fs, time::Duration};

fn event() -> LogEvent {
    LogEvent {
        event: EventKind::AppStarted,
        pane: None,
        code: None,
    }
}

#[test]
fn shutdown_from_clone_drains_and_closes_every_producer() {
    let directory = tempfile::tempdir().unwrap();
    let logger = BoundedLogger::start(directory.path().into()).unwrap();
    logger.record(event());
    logger.record(LogEvent {
        event: EventKind::AppStopped,
        pane: Some(3),
        code: Some(domain::FaultCode::IoError),
    });
    logger.clone().shutdown(Duration::from_secs(2)).unwrap();
    let bytes = fs::read(directory.path().join("logs/current.jsonl")).unwrap();
    let records: Vec<serde_json::Value> = bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).unwrap())
        .collect();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["event"], "app_started");
    assert_eq!(records[1]["event"], "app_stopped");
    assert_eq!(records[1]["code"], "io_error");
    for record in records {
        let object = record.as_object().unwrap();
        assert_eq!(object.len(), 4);
        assert!(
            object
                .keys()
                .all(|key| ["timestampMs", "event", "pane", "code"].contains(&key.as_str()))
        );
    }
    assert_eq!(logger.dropped_count(), 0);
    logger.record(event());
    assert_eq!(logger.dropped_count(), 1);
}

#[test]
fn partial_tail_and_oversized_logs_recover_with_bounded_rotation() {
    let directory = tempfile::tempdir().unwrap();
    let logs = directory.path().join("logs");
    fs::create_dir(&logs).unwrap();
    let line = b"{\"event\":\"app_started\",\"pane\":null,\"code\":null,\"timestampMs\":1}\n";
    let mut existing = Vec::new();
    while existing.len() + line.len() <= 65536 {
        existing.extend_from_slice(line);
    }
    existing.extend_from_slice(b"{\"interrupted");
    fs::write(logs.join("current.jsonl"), &existing).unwrap();
    fs::write(logs.join("1.jsonl"), line.repeat(2000)).unwrap();
    fs::write(logs.join("2.jsonl"), line).unwrap();
    let logger = BoundedLogger::start(directory.path().into()).unwrap();
    for _ in 0..100 {
        logger.record(event());
    }
    logger.shutdown(Duration::from_secs(2)).unwrap();
    assert_eq!(fs::read_dir(&logs).unwrap().count(), 3);
    for name in ["current.jsonl", "1.jsonl", "2.jsonl"] {
        let bytes = fs::read(logs.join(name)).unwrap();
        assert!(bytes.len() <= 65536);
        assert!(bytes.ends_with(b"\n"));
        for line in bytes
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
        {
            assert!(serde_json::from_slice::<serde_json::Value>(line).is_ok());
        }
    }
    let current = fs::read_to_string(logs.join("current.jsonl")).unwrap();
    assert_eq!(current.lines().count(), 100);
}

#[test]
fn second_logger_cannot_open_active_writers_files() {
    let directory = tempfile::tempdir().unwrap();
    let logger = BoundedLogger::start(directory.path().into()).unwrap();
    assert!(matches!(
        BoundedLogger::start(directory.path().into()),
        Err(projectpane_core::persistence::LogError::Busy)
    ));
    logger.shutdown(Duration::from_secs(2)).unwrap();
    BoundedLogger::start(directory.path().into())
        .unwrap()
        .shutdown(Duration::from_secs(2))
        .unwrap();
}

#[test]
fn writer_io_failure_disables_logging_and_surfaces_at_shutdown() {
    let directory = tempfile::tempdir().unwrap();
    let logger = BoundedLogger::start(directory.path().into()).unwrap();
    fs::create_dir(directory.path().join("logs/current.jsonl")).unwrap();
    logger.record(event());
    assert_eq!(
        logger.shutdown(Duration::from_secs(2)),
        Err(projectpane_core::persistence::LogError::Io)
    );
}
