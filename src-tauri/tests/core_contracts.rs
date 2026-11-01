use projectpane_core::domain::{AbsolutePath, Batch, EntryKind, Fault, FaultCode, Identity};

fn root() -> &'static str {
    if cfg!(windows) { "C:\\" } else { "/" }
}

#[test]
fn absolute_path_checks_bytes_and_preserves_unicode_without_io() {
    let path = format!("{}missing/../한글 📁", root());
    assert_eq!(AbsolutePath::new(path.clone()).unwrap().into_string(), path);
    let limit = format!("{}{}", root(), "a".repeat(32_768 - root().len()));
    assert!(AbsolutePath::new(limit.clone()).is_ok());
    for invalid in [
        String::new(),
        "relative".into(),
        format!("{}nul\0", root()),
        limit + "a",
        format!("{}{}", root(), "한".repeat(11_000)),
    ] {
        assert_eq!(
            AbsolutePath::new(invalid).unwrap_err().code,
            FaultCode::InvalidPath
        );
    }
}

#[test]
fn identity_rejects_invalid_numbers_and_identifiers() {
    let valid = Identity {
        pane: 3,
        tab_id: "a".repeat(80),
        request_id: "request-9".into(),
        generation: 9_007_199_254_740_991,
    };
    assert!(valid.validate().is_ok());
    let mut invalid = vec![
        Identity {
            pane: 4,
            ..valid.clone()
        },
        Identity {
            generation: 0,
            ..valid.clone()
        },
        Identity {
            generation: 9_007_199_254_740_992,
            ..valid.clone()
        },
    ];
    for value in ["", "a_b", "with space", "한글", &"a".repeat(81)] {
        invalid.push(Identity {
            tab_id: value.into(),
            ..valid.clone()
        });
        invalid.push(Identity {
            request_id: value.into(),
            ..valid.clone()
        });
    }
    for identity in invalid {
        assert_eq!(
            identity.validate().unwrap_err().code,
            FaultCode::InvalidRequest
        );
    }
}

#[test]
fn fault_codes_have_exact_closed_wire_and_safe_log_labels() {
    let codes = [
        FaultCode::InvalidRequest,
        FaultCode::InvalidPath,
        FaultCode::InvalidState,
        FaultCode::NotFound,
        FaultCode::AccessDenied,
        FaultCode::IoError,
        FaultCode::ResourceLimit,
        FaultCode::PayloadLimit,
        FaultCode::ProtocolError,
        FaultCode::WorkerUnavailable,
        FaultCode::Busy,
        FaultCode::Cancelled,
        FaultCode::Timeout,
        FaultCode::StaleRequest,
    ];
    let labels = [
        "invalid_request",
        "invalid_path",
        "invalid_state",
        "not_found",
        "access_denied",
        "io_error",
        "resource_limit",
        "payload_limit",
        "protocol_error",
        "worker_unavailable",
        "busy",
        "cancelled",
        "timeout",
        "stale_request",
    ];
    for (code, label) in codes.into_iter().zip(labels) {
        assert_eq!(code.as_str(), label);
        let fault = Fault::new(code);
        let wire = serde_json::json!({"code": label});
        assert_eq!(serde_json::to_value(&fault).unwrap(), wire);
        assert_eq!(serde_json::from_value::<Fault>(wire).unwrap(), fault);
    }
    assert!(serde_json::from_str::<Fault>(r#"{"code":"private-path-or-new-code"}"#).is_err());
    assert!(serde_json::from_str::<Fault>(r#"{"code":"timeout","detail":"private"}"#).is_err());
}

#[test]
fn entry_kinds_preserve_wire_without_string_storage() {
    for (kind, label) in [
        (EntryKind::File, "file"),
        (EntryKind::Folder, "folder"),
        (EntryKind::Link, "link"),
        (EntryKind::Unknown, "unknown"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), label);
        assert_eq!(
            serde_json::from_value::<EntryKind>(serde_json::json!(label)).unwrap(),
            kind
        );
    }
    assert!(serde_json::from_value::<EntryKind>(serde_json::json!("other")).is_err());
    assert_eq!(std::mem::size_of::<EntryKind>(), 1);
}

#[test]
fn batch_requires_every_wire_field_including_nullable_cap_reason() {
    let valid = serde_json::json!({
        "revision": 1, "offset": 0, "entries": [], "total": 0, "complete": true,
        "capped": false, "snapshotBytes": 0, "capReason": null
    });
    assert!(serde_json::from_value::<Batch>(valid.clone()).is_ok());
    for field in valid.as_object().unwrap().keys() {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<Batch>(missing).is_err(), "{field}");
    }
}

#[tokio::test]
async fn invalid_inputs_are_rejected_before_worker_spawn_or_filesystem_access() {
    use projectpane_core::{domain::Request, platform::worker::Listing, services::Broker};
    let directory = tempfile::tempdir().unwrap();
    let broker = Broker::new(
        directory.path().join("missing-worker"),
        std::time::Duration::from_secs(8),
    );
    let mut id = Identity {
        pane: 0,
        tab_id: "tab".into(),
        request_id: "request".into(),
        generation: 1,
    };
    for path in [
        String::new(),
        "relative".into(),
        format!("{}nul\0", root()),
        format!("{}{}", root(), "a".repeat(32_768)),
    ] {
        assert_eq!(
            broker
                .start(id.clone(), path.clone())
                .await
                .unwrap_err()
                .code,
            FaultCode::InvalidPath
        );
        assert_eq!(
            Listing::default()
                .handle(Request::Open {
                    path,
                    view: Default::default()
                })
                .unwrap_err()
                .code,
            FaultCode::InvalidPath
        );
    }
    id.generation = 0;
    assert_eq!(
        broker
            .start(id.clone(), root().into())
            .await
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    assert_eq!(
        broker.read(id.clone(), None).await.unwrap_err().code,
        FaultCode::InvalidRequest
    );
    assert_eq!(
        broker.cancel(&id).await.unwrap_err().code,
        FaultCode::InvalidRequest
    );
}

#[tokio::test]
async fn mismatched_identity_cannot_read_or_cancel_current_worker() {
    use projectpane_core::services::Broker;
    let directory = tempfile::tempdir().unwrap();
    let broker = Broker::new(
        env!("CARGO_BIN_EXE_projectpane-read-worker").into(),
        std::time::Duration::from_secs(8),
    );
    let id = Identity {
        pane: 0,
        tab_id: "tab".into(),
        request_id: "request".into(),
        generation: 1,
    };
    broker
        .start(id.clone(), directory.path().to_str().unwrap().into())
        .await
        .unwrap();
    for mismatch in [
        Identity {
            pane: 1,
            ..id.clone()
        },
        Identity {
            tab_id: "other".into(),
            ..id.clone()
        },
        Identity {
            request_id: "other".into(),
            ..id.clone()
        },
        Identity {
            generation: 2,
            ..id.clone()
        },
    ] {
        assert_eq!(
            broker.read(mismatch.clone(), None).await.unwrap_err().code,
            FaultCode::StaleRequest
        );
        broker.cancel(&mismatch).await.unwrap();
        assert_eq!(broker.read(id.clone(), Some(0)).await.unwrap().identity, id);
    }
    broker.cancel(&id).await.unwrap();
}
