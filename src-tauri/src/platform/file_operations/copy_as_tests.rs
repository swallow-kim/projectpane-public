use super::*;
use crate::domain::{ActivationKind, ActivationTarget, Fault, FaultCode, Identity, operations::*};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

pub(super) fn named_plan(
    source: &Path,
    destination: &Path,
    name: &str,
) -> Result<OperationPlan, Fault> {
    prepare::prepare(
        PrepareOperation {
            project_id: None,
            action: OperationAction::CopyAs {
                selection: OperationSelection {
                    identity: Identity {
                        pane: 0,
                        tab_id: "copy-as".into(),
                        request_id: "request-1".into(),
                        generation: 1,
                    },
                    entry_ids: vec!["opaque-1".into()],
                },
                destination: destination.to_str().unwrap().into(),
                name: name.into(),
            },
        },
        vec![ActivationTarget {
            path: source.to_str().unwrap().into(),
            kind: if source.is_dir() {
                ActivationKind::Folder
            } else {
                ActivationKind::File
            },
        }],
    )
}

#[tokio::test]
async fn copy_as_native_unicode_same_folder_preserves_source_and_exact_bytes() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original.bin");
    let payload: Vec<u8> = (0_u8..=250).cycle().take(262144).collect();
    fs::write(&original, &payload).unwrap();
    let plan = named_plan(&original, fixture.path(), "사본 😀.bin").unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let report = lane
        .execute(plan, 0, Arc::new(AtomicBool::new(false)))
        .await
        .unwrap();
    lane.shutdown().await.unwrap();
    assert_eq!(report.status(), OperationStatus::Succeeded, "{report:?}");
    assert_eq!(report.counts.changed, 1);
    assert_eq!(report.items.len(), 1);
    assert_eq!(
        report.items[0].destination.as_deref(),
        fixture.path().join("사본 😀.bin").to_str()
    );
    assert_eq!(fs::read(&original).unwrap(), payload);
    assert_eq!(
        fs::read(fixture.path().join("사본 😀.bin")).unwrap(),
        payload
    );
    assert_eq!(fs::read_dir(fixture.path()).unwrap().count(), 2);
    report.validate().unwrap();
}

#[test]
fn copy_as_prepare_refuses_existing_target_and_folder_source() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    fs::write(&original, b"source").unwrap();
    assert_eq!(
        named_plan(&original, fixture.path(), "original")
            .unwrap_err()
            .code,
        FaultCode::DestinationExists
    );
    assert_eq!(
        named_plan(fixture.path(), fixture.path(), "new")
            .unwrap_err()
            .code,
        FaultCode::InvalidRequest
    );
    assert_eq!(fs::read(original).unwrap(), b"source");
}

#[tokio::test]
async fn copy_as_dispatch_refuses_target_created_after_prepare() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    fs::write(&original, b"source").unwrap();
    let plan = named_plan(&original, fixture.path(), "target").unwrap();
    fs::write(fixture.path().join("target"), b"sentinel").unwrap();
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane
        .execute(plan, 0, Arc::new(AtomicBool::new(false)))
        .await;
    lane.shutdown().await.unwrap();
    assert_eq!(result.unwrap_err().code, FaultCode::DestinationExists);
    assert_eq!(fs::read(original).unwrap(), b"source");
    assert_eq!(
        fs::read(fixture.path().join("target")).unwrap(),
        b"sentinel"
    );
}

#[tokio::test]
async fn copy_as_dispatch_refuses_replaced_source_or_destination_identity() {
    for replace_source in [true, false] {
        let fixture = tempfile::tempdir().unwrap();
        let original = fixture.path().join("original");
        let destination = fixture.path().join("destination");
        fs::write(&original, b"source").unwrap();
        fs::create_dir(&destination).unwrap();
        let plan = named_plan(&original, &destination, "target").unwrap();
        if replace_source {
            fs::rename(&original, fixture.path().join("held-source")).unwrap();
            fs::write(&original, b"replacement").unwrap();
        } else {
            fs::rename(&destination, fixture.path().join("held-destination")).unwrap();
            fs::create_dir(&destination).unwrap();
        }
        let lane = NativeFileOperationLane::start().unwrap();
        let result = lane
            .execute(plan, 0, Arc::new(AtomicBool::new(false)))
            .await;
        lane.shutdown().await.unwrap();
        assert_eq!(result.unwrap_err().code, FaultCode::StaleRequest);
        assert!(!destination.join("target").exists());
    }
}

#[test]
fn copy_as_callback_preflight_binds_source_parent_name_and_absence() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    let other = fixture.path().join("other");
    let wrong_parent = fixture.path().join("wrong-parent");
    fs::write(&original, b"source").unwrap();
    fs::write(&other, b"other").unwrap();
    fs::create_dir(&wrong_parent).unwrap();
    let plan = named_plan(&original, fixture.path(), "target").unwrap();
    let original = original.to_str().unwrap();
    let parent = fixture.path().to_str().unwrap();
    for (source, parent, name) in [
        (other.to_str().unwrap(), parent, Some("target")),
        (original, wrong_parent.to_str().unwrap(), Some("target")),
        (original, parent, Some("renamed")),
        (original, parent, None),
        (original, parent, Some("")),
    ] {
        assert!(copy_as::check_before(&plan, source, (parent, name)).is_err());
    }
    copy_as::check_before(&plan, original, (parent, Some("target"))).unwrap();
    fs::write(fixture.path().join("target"), b"sentinel").unwrap();
    assert_eq!(
        copy_as::check_before(&plan, original, (parent, Some("target")))
            .unwrap_err()
            .code,
        FaultCode::DestinationExists
    );
}

#[test]
fn copy_as_callback_confirmation_refuses_unapproved_actual_destination() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    fs::write(&original, b"source").unwrap();
    let plan = named_plan(&original, fixture.path(), "target").unwrap();
    let other = fixture.path().join("autorename");
    fs::copy(&original, &other).unwrap();
    assert!(
        copy_as::check_after(&plan, original.to_str().unwrap(), other.to_str().unwrap()).is_err()
    );
    fs::rename(other, fixture.path().join("target")).unwrap();
    copy_as::check_after(
        &plan,
        original.to_str().unwrap(),
        fixture.path().join("target").to_str().unwrap(),
    )
    .unwrap();
}
