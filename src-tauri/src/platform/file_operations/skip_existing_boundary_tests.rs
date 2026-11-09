use super::{skip_existing_tests::Fixture, *};
use crate::domain::{FaultCode, operations::*};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};

#[tokio::test]
async fn skip_existing_dispatch_rejects_swapped_sources_destination_and_skipped_targets() {
    let lane = NativeFileOperationLane::start().unwrap();
    for replace in [
        "source",
        "destination",
        "skipped",
        "removed",
        "new-collision",
    ] {
        // Given a retained plan with one approved existing target.
        let fixture = Fixture::new(1);
        let plan = fixture.plan().unwrap();
        let target = fixture
            .destination
            .join(fixture.sources[0].file_name().unwrap());
        // When an identity or absence condition changes after preparation.
        match replace {
            "source" => {
                fs::rename(&fixture.sources[0], fixture.root.path().join("held-source")).unwrap();
                fs::write(&fixture.sources[0], b"replacement").unwrap();
            }
            "destination" => {
                fs::rename(
                    &fixture.destination,
                    fixture.root.path().join("held-destination"),
                )
                .unwrap();
                fs::create_dir(&fixture.destination).unwrap();
            }
            "skipped" | "removed" => {
                fs::rename(&target, fixture.root.path().join("held-target")).unwrap();
                if replace == "skipped" {
                    fs::write(&target, b"replacement").unwrap();
                }
            }
            "new-collision" => {
                fs::write(
                    fixture
                        .destination
                        .join(fixture.sources[1].file_name().unwrap()),
                    b"late",
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        let error = lane
            .execute(plan, 0, Arc::new(AtomicBool::new(false)))
            .await
            .unwrap_err();
        // Then dispatch fails before applying any user decisions or native copies.
        assert_eq!(
            error.code,
            if replace == "new-collision" {
                FaultCode::DestinationExists
            } else {
                FaultCode::StaleRequest
            }
        );
        if replace != "new-collision" {
            assert!(
                !fixture
                    .destination
                    .join(fixture.sources[1].file_name().unwrap())
                    .exists()
            );
        }
    }
    lane.shutdown().await.unwrap();
}

#[test]
fn skip_existing_refuses_folders_duplicate_identities_same_target_and_source_target_aliases() {
    for case in [
        "folder-source",
        "folder-target",
        "same-source",
        "duplicate-id",
        "target-alias",
        "duplicate-target-id",
        "duplicate-name",
    ] {
        let mut fixture = Fixture::new(0);
        match case {
            "folder-source" => {
                let folder = fixture.root.path().join("folder");
                fs::create_dir(&folder).unwrap();
                fixture.sources[0] = folder;
            }
            "folder-target" => {
                fs::create_dir(
                    fixture
                        .destination
                        .join(fixture.sources[0].file_name().unwrap()),
                )
                .unwrap();
            }
            "same-source" => {
                fixture.destination = fixture.sources[0].parent().unwrap().into();
            }
            "duplicate-id" => {
                let link = fixture.root.path().join("linked");
                fs::hard_link(&fixture.sources[0], &link).unwrap();
                fixture.sources[1] = link;
            }
            "target-alias" => {
                fs::hard_link(
                    &fixture.sources[0],
                    fixture
                        .destination
                        .join(fixture.sources[0].file_name().unwrap()),
                )
                .unwrap();
            }
            "duplicate-target-id" => {
                let first = fixture
                    .destination
                    .join(fixture.sources[0].file_name().unwrap());
                fs::write(&first, b"sentinel").unwrap();
                fs::hard_link(
                    first,
                    fixture
                        .destination
                        .join(fixture.sources[1].file_name().unwrap()),
                )
                .unwrap();
            }
            "duplicate-name" => {
                let other = fixture
                    .root
                    .path()
                    .join(fixture.sources[0].file_name().unwrap());
                fs::write(&other, b"other").unwrap();
                fixture.sources[1] = other;
            }
            _ => unreachable!(),
        }
        assert_eq!(
            fixture.plan().unwrap_err().code,
            FaultCode::InvalidRequest,
            "{case}"
        );
    }
}

#[test]
fn skip_existing_plan_rejects_forged_decision_membership_identity_and_result_budget() {
    let fixture = Fixture::new(1);
    let plan = fixture.plan().unwrap();
    for case in ["source", "target", "identity", "duplicate", "folder"] {
        let mut bad = plan.clone();
        let PreparedAction::CopySkipExisting {
            sources, skipped, ..
        } = &mut bad.action
        else {
            panic!()
        };
        match case {
            "source" => skipped[0].source = r"C:\unselected".into(),
            "target" => skipped[0].target.path = r"C:\elsewhere".into(),
            "identity" => {
                skipped[0].target.volume = sources[0].volume;
                skipped[0].target.file_index = sources[0].file_index;
            }
            "duplicate" => skipped.push(skipped[0].clone()),
            "folder" => skipped[0].target.kind = crate::domain::EntryKind::Folder,
            _ => unreachable!(),
        }
        assert!(bad.validate().is_err(), "{case}");
    }
    let mut large = plan;
    let PreparedAction::CopySkipExisting {
        sources,
        destination,
        skipped,
    } = &mut large.action
    else {
        panic!()
    };
    skipped.clear();
    destination.path = format!("C:\\{}", "long\\".repeat(100));
    let template = sources[0].clone();
    *sources = (1..=600)
        .map(|index| OperationSource {
            path: format!("C:\\s\\{index}"),
            file_index: index,
            ..template.clone()
        })
        .collect();
    assert_eq!(large.validate().unwrap_err().code, FaultCode::PayloadLimit);
}

#[test]
fn skip_existing_refuses_reparse_destination_without_following_it() {
    let fixture = Fixture::new(0);
    let link = fixture.root.path().join("junction");
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&link)
        .arg(&fixture.destination)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut fixture = fixture;
    fixture.destination = link;
    assert_eq!(
        fixture.plan().unwrap_err().code,
        FaultCode::UnsafeReparsePoint
    );
}
