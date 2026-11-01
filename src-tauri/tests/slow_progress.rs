use projectpane_core::domain::FaultCode;
#[path = "slow_progress/fixture.rs"]
mod fixture;
#[path = "slow_progress/malformed.rs"]
mod malformed;

use projectpane_core::{
    domain::{BATCH_SIZE, Identity},
    services::Broker,
};
use std::time::{Duration, Instant};

fn identity() -> Identity {
    Identity {
        pane: 0,
        tab_id: "slow-progress".into(),
        request_id: "slow-progress-1".into(),
        generation: 1,
    }
}

async fn scenario(broker: &Broker, path: String) {
    // Given: a worker with 25 ms metadata exchanges and more than 64 chunks.
    let id = identity();
    let first = broker.start(id.clone(), path).await.unwrap();
    assert_eq!(first.batch.entries.len(), BATCH_SIZE);
    let mut total = first.batch.total;
    let mut calls = 0;
    // When: repeatedly polling progress through the real subprocess transport.
    loop {
        let started = Instant::now();
        let reply = broker.progress(id.clone()).await.unwrap();
        calls += 1;
        let chunks = (reply.batch.total - total).div_ceil(BATCH_SIZE);
        eprintln!(
            "call={calls} chunks={chunks} offset={} total={} complete={} elapsed_ms={}",
            reply.batch.offset,
            reply.batch.total,
            reply.batch.complete,
            started.elapsed().as_millis()
        );
        // Then: each slice yields after a handful of chunks, with no lost progress.
        assert_eq!(reply.identity, id);
        assert!(reply.batch.entries.is_empty());
        assert_eq!(reply.batch.offset, total);
        assert!(reply.batch.total > total);
        assert!(
            (1..=3).contains(&chunks),
            "slow exchange collected {chunks} chunks"
        );
        assert!(!reply.batch.capped);
        total = reply.batch.total;
        assert!(total <= fixture::TOTAL);
        if reply.batch.complete {
            break;
        }
        assert!(calls <= fixture::TOTAL.div_ceil(BATCH_SIZE));
    }
    assert_eq!(total, fixture::TOTAL);
    for offset in (0..total).step_by(BATCH_SIZE) {
        let reply = broker.read(id.clone(), Some(offset)).await.unwrap();
        assert_eq!(reply.batch.offset, offset);
        assert_eq!(reply.batch.total, total);
        assert!(reply.batch.complete);
        assert_eq!(reply.batch.entries.len(), (total - offset).min(BATCH_SIZE));
        for (index, actual) in reply.batch.entries.iter().enumerate() {
            let expected = fixture::entry(offset + index);
            assert_eq!(actual.id, expected.id);
            assert_eq!(actual.name, expected.name);
            assert_eq!(actual.kind, expected.kind);
        }
    }
    eprintln!("completed: calls={calls} total={total}; all window data verified");
}

fn main() {
    let executable = std::env::current_exe().unwrap();
    if executable.file_stem().unwrap() == "malformed-worker-fixture" {
        malformed::fixture();
        return;
    }
    let fixture_name = format!("slow-progress-fixture{}", std::env::consts::EXE_SUFFIX);
    if executable.file_name().unwrap() == fixture_name.as_str() {
        fixture::run();
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let fixture_path = directory.path().join(fixture_name);
    std::fs::copy(executable, &fixture_path).unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(malformed::scenario(directory.path()));
    let broker = Broker::new(fixture_path, Duration::from_secs(8));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        runtime.block_on(scenario(
            &broker,
            directory.path().to_string_lossy().into_owned(),
        ));
    }));
    // Reap our owned subprocess even when the expected RED assertion panics.
    runtime.block_on(broker.cancel(&identity())).unwrap();
    eprintln!("fixture cancelled and reaped");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
    assert_eq!(
        runtime
            .block_on(broker.progress(identity()))
            .unwrap_err()
            .code,
        FaultCode::StaleRequest
    );
}
