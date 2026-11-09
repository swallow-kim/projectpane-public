use super::{copy_as_tests::named_plan, *};
use crate::domain::{EntryKind, FaultCode, operations::*};
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
};

#[tokio::test]
async fn copy_as_native_queue_rejects_invalid_intent_even_when_cancelled() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    fs::write(&original, b"source").unwrap();
    let mut plan = named_plan(&original, fixture.path(), "target").unwrap();
    let PreparedAction::CopyAs { source, .. } = &mut plan.action else {
        panic!("named-copy plan required");
    };
    source.kind = EntryKind::Folder;
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane.execute(plan, 0, Arc::new(AtomicBool::new(true))).await;
    lane.shutdown().await.unwrap();
    assert_eq!(result.unwrap_err().code, FaultCode::InvalidRequest);
    assert!(!fixture.path().join("target").exists());
}

#[test]
fn copy_as_native_preflight_refuses_unsupported_volume_paths() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    fs::write(&original, b"source").unwrap();
    let plan = named_plan(&original, fixture.path(), "target").unwrap();
    for replace_source in [true, false] {
        let mut invalid = plan.clone();
        let PreparedAction::CopyAs {
            source,
            destination,
            ..
        } = &mut invalid.action
        else {
            panic!("named-copy plan required");
        };
        let entry = if replace_source { source } else { destination };
        entry.path = r"\\server\share\file".into();
        assert_eq!(
            check_plan(&invalid).unwrap_err().code,
            FaultCode::InvalidPath
        );
    }
}

#[test]
fn copy_as_native_capture_refuses_junction_sources_and_destinations() {
    use std::os::windows::process::CommandExt;
    let fixture = tempfile::tempdir().unwrap();
    let outside = fixture.path().join("outside-owned");
    let junction = fixture.path().join("junction");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("source"), b"source").unwrap();
    let created = std::process::Command::new("pwsh")
        .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:PROJECTPANE_TEST_JUNCTION -Value $env:PROJECTPANE_TEST_TARGET | Out-Null"])
        .env("PROJECTPANE_TEST_JUNCTION", &junction)
        .env("PROJECTPANE_TEST_TARGET", &outside)
        .creation_flags(0x08000000).output().unwrap();
    assert!(created.status.success(), "owned junction creation failed");
    assert_eq!(
        named_plan(&junction.join("source"), fixture.path(), "target")
            .unwrap_err()
            .code,
        FaultCode::UnsafeReparsePoint
    );
    assert_eq!(
        named_plan(&outside.join("source"), &junction, "target")
            .unwrap_err()
            .code,
        FaultCode::UnsafeReparsePoint
    );
    assert_eq!(fs::read(outside.join("source")).unwrap(), b"source");
    assert!(!outside.join("target").exists());
}

#[test]
fn copy_as_callback_confirmation_requires_exact_returned_path_case() {
    let fixture = tempfile::tempdir().unwrap();
    let original = fixture.path().join("original");
    fs::write(&original, b"source").unwrap();
    let plan = named_plan(&original, fixture.path(), "target").unwrap();
    fs::copy(&original, fixture.path().join("target")).unwrap();
    assert!(
        copy_as::check_after(
            &plan,
            original.to_str().unwrap(),
            fixture.path().join("TARGET").to_str().unwrap()
        )
        .is_err()
    );
}
