use projectpane_core::{
    domain::{FaultCode, Identity, Request},
    platform::worker::Listing,
    services::Broker,
};
use std::{
    io::{BufRead, Write},
    path::Path,
    time::Duration,
};

pub fn fixture() {
    let mut listing = Listing::default();
    let mut output = std::io::stdout().lock();
    for line in std::io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line.unwrap()).unwrap();
        if let Request::Open { path, .. } = &request {
            let mode = Path::new(path).file_name().unwrap().to_str().unwrap();
            let malformed = match mode {
                "unknown-fault" => Some(r#"{"Err":{"code":"private-or-unknown"}}"#),
                "invalid-json" => Some("not-json"),
                "invalid-batch" => Some(
                    r#"{"Ok":{"revision":1,"offset":0,"entries":[],"total":100001,"complete":true,"capped":false,"snapshotBytes":0,"capReason":null}}"#,
                ),
                _ => None,
            };
            if let Some(malformed) = malformed {
                writeln!(output, "{malformed}").unwrap();
                output.flush().unwrap();
                loop {
                    std::thread::park();
                }
            }
        }
        serde_json::to_writer(&mut output, &listing.handle(request)).unwrap();
        output.write_all(b"\n").unwrap();
        output.flush().unwrap();
    }
}

fn identity(pane: usize, generation: u64) -> Identity {
    Identity {
        pane,
        tab_id: format!("pane-{pane}"),
        request_id: format!("request-{generation}"),
        generation,
    }
}

pub async fn scenario(root: &Path) {
    let executable = root.join(format!(
        "malformed-worker-fixture{}",
        std::env::consts::EXE_SUFFIX
    ));
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let broker = Broker::new(executable, Duration::from_secs(8));
    let good = root.join("good");
    std::fs::create_dir(&good).unwrap();
    std::fs::write(good.join("untouched.txt"), b"unchanged").unwrap();
    for pane in 0..4 {
        broker
            .start(identity(pane, 1), good.to_str().unwrap().into())
            .await
            .unwrap();
    }
    let mut generation = 1;
    for mode in ["unknown-fault", "invalid-json", "invalid-batch"] {
        generation += 1;
        let failed = identity(0, generation);
        assert_eq!(
            broker
                .start(failed.clone(), root.join(mode).to_str().unwrap().into())
                .await
                .unwrap_err()
                .code,
            FaultCode::ProtocolError
        );
        assert_eq!(
            broker.read(failed.clone(), None).await.unwrap_err().code,
            FaultCode::StaleRequest
        );
        generation += 1;
        let current = identity(0, generation);
        let reply = broker
            .start(current.clone(), good.to_str().unwrap().into())
            .await
            .unwrap();
        assert_eq!(reply.identity, current);
        assert_eq!(reply.batch.total, 1);
        broker.cancel(&failed).await.unwrap();
        for pane in 1..4 {
            assert_eq!(
                broker
                    .read(identity(pane, 1), Some(0))
                    .await
                    .unwrap()
                    .batch
                    .total,
                1
            );
        }
        assert_eq!(broker.read(current, Some(0)).await.unwrap().batch.total, 1);
        eprintln!(
            "{mode}: protocol_error, stale generation rejected, slot reused, other panes healthy"
        );
    }
    broker.cancel(&identity(0, generation)).await.unwrap();
    for pane in 1..4 {
        broker.cancel(&identity(pane, 1)).await.unwrap();
    }
    assert_eq!(
        std::fs::read(good.join("untouched.txt")).unwrap(),
        b"unchanged"
    );
}
