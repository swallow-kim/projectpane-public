use super::*;
use tokio::time::timeout;

#[tokio::test]
async fn cancellation_keeps_slot_occupied_until_reaping_finishes() {
    let identity = Identity {
        pane: 0,
        tab_id: "tab".into(),
        request_id: "request".into(),
        generation: 1,
    };
    let session = Arc::new(
        Session::spawn(std::path::Path::new("/bin/cat"), identity.clone())
            .await
            .unwrap(),
    );
    let broker = Arc::new(Broker::new("/bin/sleep".into(), Duration::from_secs(1)));
    broker.slots.lock().unwrap()[0] = Slot {
        generation: 1,
        session: Some(session.clone()),
    };
    // Hold the in-flight RPC lock to make the formerly racy interval deterministic.
    let held = session.pipes.lock().await;
    let pending_broker = broker.clone();
    let pending = tokio::spawn(async move { pending_broker.cancel(&identity).await });
    let mut signal = session.cancelled.subscribe();
    timeout(Duration::from_secs(1), signal.wait_for(|value| *value))
        .await
        .unwrap()
        .unwrap();
    assert!(broker.slots.lock().unwrap()[0].session.is_some());
    drop(held);
    pending.await.unwrap().unwrap();
    assert!(broker.slots.lock().unwrap()[0].session.is_none());
}
