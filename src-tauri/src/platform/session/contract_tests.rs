use super::{tests::fixture, *};
use serde_json::{Value, json};

fn shared_contract() -> Value {
    serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../tests/fixtures/ipc-contract.json"
    )))
    .unwrap()
}

fn batch() -> Value {
    shared_contract()["validReply"]["batch"].clone()
}

#[test]
fn shared_reply_contract_deserializes_and_validates_for_display_window() {
    let validate = |value: Value| -> Result<Reply, Fault> {
        let reply: Reply =
            serde_json::from_value(value).map_err(|_| Fault::new(FaultCode::ProtocolError))?;
        reply.identity.validate()?;
        reply.batch.validate(&Request::Window {
            offset: 0,
            limit: 64,
        })?;
        Ok(reply)
    };
    let contract = shared_contract();
    let valid = validate(contract["validReply"].clone()).unwrap();
    assert_eq!(serde_json::to_value(valid).unwrap(), contract["validReply"]);
    for mutation in contract["invalidReplyMutations"].as_array().unwrap() {
        let mut reply = contract["validReply"].clone();
        let field = mutation["field"].as_str().unwrap();
        reply["batch"][field] = mutation["value"].clone();
        assert_eq!(
            validate(reply).unwrap_err().code,
            FaultCode::ProtocolError,
            "{field}"
        );
    }
}

async fn reject(batch: Value, request: Request) {
    let (session, _reply_file) = fixture(&json!({"Ok": batch}).to_string()).await;
    let result = session.exchange(request, Duration::from_secs(8)).await;
    if result.is_ok() {
        session.stop().await;
    }
    assert_eq!(result.unwrap_err().code, FaultCode::ProtocolError);
    let mut pipes = session.pipes.lock().await;
    assert_eq!(pipes.child.id(), None);
    assert!(pipes.child.try_wait().unwrap().is_some());
}

#[tokio::test]
async fn duplicate_entry_ids_are_rejected_and_reaped() {
    let mut reply = batch();
    reply["entries"] = json!([reply["entries"][0], reply["entries"][0]]);
    reply["total"] = json!(2);
    reject(reply, Request::Advance).await;
}

#[tokio::test]
async fn shared_invalid_batch_mutations_are_rejected_and_reaped() {
    for mutation in shared_contract()["invalidReplyMutations"]
        .as_array()
        .unwrap()
    {
        let mut reply = batch();
        reply[mutation["field"].as_str().unwrap()] = mutation["value"].clone();
        reject(reply, Request::Advance).await;
    }
}

#[tokio::test]
async fn every_batch_and_entry_key_is_required_on_worker_wire() {
    let valid = batch();
    for key in valid.as_object().unwrap().keys() {
        let mut reply = valid.clone();
        reply.as_object_mut().unwrap().remove(key);
        reject(reply, Request::Advance).await;
    }
    for key in valid["entries"][0].as_object().unwrap().keys() {
        let mut reply = valid.clone();
        reply["entries"][0].as_object_mut().unwrap().remove(key);
        reject(reply, Request::Advance).await;
    }
}

#[tokio::test]
async fn invalid_entry_values_are_rejected_and_reaped() {
    for (key, value) in [
        ("kind", json!("new-kind")),
        ("id", json!("")),
        ("id", json!("a".repeat(MAX_ENTRY_ID + 1))),
        ("sizeBytes", json!(-1)),
        ("sizeBytes", json!(9_007_199_254_740_992_u64)),
        ("sizeBytes", json!(0.5)),
        ("name", json!("a".repeat(32_769))),
        ("unexpected", json!(true)),
        ("modifiedAtMs", json!(8_640_000_000_000_001_i64)),
        ("modifiedAtMs", json!(-8_640_000_000_000_001_i64)),
        ("modifiedAtMs", json!(0.5)),
        ("modifiedAtMs", json!("0")),
    ] {
        let mut reply = batch();
        reply["entries"][0][key] = value;
        reject(reply, Request::Advance).await;
    }
}

#[tokio::test]
async fn replies_must_match_progress_open_and_window_requests() {
    reject(batch(), Request::Progress).await;
    reject(
        batch(),
        Request::Window {
            offset: 1,
            limit: 1,
        },
    )
    .await;
    let mut offset_reply = batch();
    offset_reply["offset"] = json!(1);
    offset_reply["total"] = json!(2);
    reject(
        offset_reply,
        Request::Open {
            view: Default::default(),
            path: "unused".into(),
        },
    )
    .await;
    let mut reply = batch();
    let mut second = reply["entries"][0].clone();
    second["id"] = json!("entry-1");
    reply["entries"].as_array_mut().unwrap().push(second);
    reply["total"] = json!(2);
    reject(
        reply,
        Request::Window {
            offset: 0,
            limit: 1,
        },
    )
    .await;
}

#[tokio::test]
async fn worker_allows_256_unique_entries_but_rejects_257() {
    let mut reply = batch();
    let entry = reply["entries"][0].clone();
    reply["entries"] = (0..BATCH_SIZE)
        .map(|index| {
            let mut entry = entry.clone();
            entry["id"] = json!(format!("entry-{index}"));
            entry
        })
        .collect();
    reply["total"] = json!(BATCH_SIZE);
    let (session, _reply_file) = fixture(&json!({"Ok": reply}).to_string()).await;
    let result = session
        .exchange(
            Request::Window {
                offset: 0,
                limit: BATCH_SIZE,
            },
            Duration::from_secs(8),
        )
        .await;
    session.stop().await;
    assert_eq!(result.unwrap().batch.entries.len(), BATCH_SIZE);
    let mut last = entry;
    last["id"] = json!("entry-256");
    reply["entries"].as_array_mut().unwrap().push(last);
    reply["total"] = json!(BATCH_SIZE + 1);
    reject(reply, Request::Advance).await;
}
