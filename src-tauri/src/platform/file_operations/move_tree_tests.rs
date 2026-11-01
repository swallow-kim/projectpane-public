use super::*;
use crate::domain::{ActivationKind, ActivationTarget, FaultCode, Identity, operations::*};
use std::os::windows::ffi::OsStrExt;
use std::{
    fs,
    os::windows::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};

struct Fixture(Option<tempfile::TempDir>, Option<tempfile::TempDir>);
impl Fixture {
    fn new() -> Self {
        let parent = std::env::var_os("PROJECTPANE_CROSS_VOLUME_FIXTURE")
            .expect("explicit synthetic parent");
        Self(
            Some(tempfile::tempdir().unwrap()),
            Some(
                tempfile::Builder::new()
                    .prefix("projectpane-native-move-")
                    .tempdir_in(parent)
                    .unwrap(),
            ),
        )
    }
    fn paths(&self) -> (PathBuf, PathBuf) {
        (
            self.0.as_ref().unwrap().path().into(),
            self.1.as_ref().unwrap().path().into(),
        )
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            for dir in [&mut self.0, &mut self.1] {
                if let Some(dir) = dir.take() {
                    eprintln!("preserved failed fixture: {}", dir.keep().display());
                }
            }
        }
    }
}
fn plan(paths: &[&Path], target: &Path) -> Result<OperationPlan, crate::domain::Fault> {
    prepare::prepare(
        PrepareOperation {
            project_id: None,
            action: OperationAction::Move {
                selection: OperationSelection {
                    identity: Identity {
                        pane: 0,
                        tab_id: "move-test".into(),
                        request_id: "move-request".into(),
                        generation: 1,
                    },
                    entry_ids: (0..paths.len()).map(|i| format!("entry-{i}")).collect(),
                },
                destination: target.to_str().unwrap().into(),
            },
        },
        paths
            .iter()
            .map(|path| ActivationTarget {
                path: path.to_str().unwrap().into(),
                kind: if path.is_dir() {
                    ActivationKind::Folder
                } else {
                    ActivationKind::File
                },
            })
            .collect(),
    )
}
async fn run(
    plan: OperationPlan,
    cancelled: bool,
) -> Result<OperationReport, crate::domain::Fault> {
    let lane = NativeFileOperationLane::start().unwrap();
    let result = lane
        .execute(plan, 0, Arc::new(AtomicBool::new(cancelled)))
        .await;
    lane.shutdown().await.unwrap();
    result
}
fn dacl(path: &Path) -> Vec<u8> {
    use windows_sys::Win32::Security::*;
    let name: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut needed = 0;
    // SAFETY: the first call obtains the required size without an output buffer.
    unsafe {
        GetFileSecurityW(
            name.as_ptr(),
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
    }
    assert!(needed > 0 && needed < 65536);
    let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
    let mut acl = std::ptr::null_mut();
    let mut present = 0;
    let mut defaulted = 0;
    // SAFETY: aligned, sufficient live storage; ACL points within this descriptor.
    unsafe {
        assert_ne!(
            GetFileSecurityW(
                name.as_ptr(),
                DACL_SECURITY_INFORMATION,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed
            ),
            0
        );
        assert_ne!(
            GetSecurityDescriptorDacl(
                buffer.as_mut_ptr().cast(),
                &mut present,
                &mut acl,
                &mut defaulted
            ),
            0
        );
        assert_ne!(present, 0);
        assert!(!acl.is_null());
        assert!((*acl).AclSize as usize <= buffer.len() * 8);
        std::slice::from_raw_parts(acl.cast(), (*acl).AclSize as usize).to_vec()
    }
}

#[tokio::test]
#[ignore = "explicit C/D synthetic fixture required"]
async fn native_tree_move_both_directions_preserves_file_streams_dates_empty_folders() {
    let fixture = Fixture::new();
    let (c, d) = fixture.paths();
    let folder = c.join("원본 폴더");
    fs::create_dir_all(folder.join("nested/empty")).unwrap();
    let file = folder.join("nested/한글.bin");
    let payload = vec![0x6d; 32769];
    fs::write(&file, &payload).unwrap();
    fs::write(format!("{}:extra", file.display()), b"named stream").unwrap();
    fs::write(
        format!("{}:Zone.Identifier", file.display()),
        b"[ZoneTransfer]\r\nZoneId=3\r\n",
    )
    .unwrap();
    let time = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(time)
        .unwrap();
    for (from, to) in [(&c, &d), (&d, &c)] {
        let folder = from.join("원본 폴더");
        let file = folder.join("nested/한글.bin");
        let before = fs::metadata(&file).unwrap();
        let file_acl = dacl(&file);
        let folder_acl = dacl(&folder);
        let prepared = plan(&[&folder], to).unwrap();
        assert!(matches!(prepared.action, PreparedAction::MoveTree { .. }));
        let report = run(prepared, false).await.unwrap();
        eprintln!("move report: {report:?}");
        assert_eq!(report.status(), OperationStatus::Succeeded);
        assert_eq!(report.counts.changed, 4);
        assert!(report.evidence.outcomes_complete);
        assert!(!folder.exists());
        let target = to.join("원본 폴더/nested/한글.bin");
        assert_eq!(fs::read(&target).unwrap(), payload);
        assert_eq!(
            fs::read(format!("{}:extra", target.display())).unwrap(),
            b"named stream"
        );
        assert_eq!(
            fs::read(format!("{}:Zone.Identifier", target.display())).unwrap(),
            b"[ZoneTransfer]\r\nZoneId=3\r\n"
        );
        let after = fs::metadata(&target).unwrap();
        assert_eq!(before.last_write_time(), after.last_write_time());
        assert_eq!(dacl(&target), file_acl);
        assert_eq!(dacl(&to.join("원본 폴더")), folder_acl);
        assert!(to.join("원본 폴더/nested/empty").is_dir());
    }
}

#[tokio::test]
#[ignore = "explicit C/D synthetic fixture required"]
async fn native_tree_move_locked_suffix_reports_partial_without_lost_payloads() {
    let fixture = Fixture::new();
    let (c, d) = fixture.paths();
    let folder = c.join("source");
    fs::create_dir(&folder).unwrap();
    let names = ["01-first.txt", "02-locked.txt", "03-last.txt"];
    for (i, name) in names.iter().enumerate() {
        fs::write(folder.join(name), vec![i as u8 + 1; 4096]).unwrap();
    }
    let prepared = plan(&[&folder], &d).unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(folder.join(names[1]))
        .unwrap();
    let report = run(prepared, false).await.unwrap();
    drop(lock);
    eprintln!("partial report: {report:?}");
    assert_ne!(report.status(), OperationStatus::Succeeded);
    assert!(report.counts.changed >= 1);
    assert!(report.counts.failed >= 1);
    assert!(folder.exists());
    assert!(report.counts.unknown >= 1);
    assert!(!report.evidence.perform_succeeded);
    for (i, name) in names.iter().enumerate() {
        let original = folder.join(name);
        let target = d.join("source").join(name);
        let bytes = vec![i as u8 + 1; 4096];
        assert!(original.exists() || target.exists());
        if original.exists() {
            assert_eq!(fs::read(original).unwrap(), bytes);
        }
        if target.exists() {
            assert_eq!(fs::read(target).unwrap(), bytes);
        }
    }
    // Retrying the original plan is rejected rather than replaying moved items.
    assert!(plan(&[&folder], &d).is_err());
}

#[tokio::test]
#[ignore = "explicit C/D synthetic fixture required"]
async fn native_tree_move_pre_cancel_collision_changed_tree_and_directory_stream_are_non_destructive()
 {
    let fixture = Fixture::new();
    let (c, d) = fixture.paths();
    let folder = c.join("source");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("sentinel"), b"original").unwrap();
    let prepared = plan(&[&folder], &d).unwrap();
    let report = run(prepared.clone(), true).await.unwrap();
    assert_ne!(report.status(), OperationStatus::Succeeded);
    assert_eq!(fs::read_dir(&d).unwrap().count(), 0);
    fs::write(folder.join("new-child"), b"new").unwrap();
    assert_eq!(
        run(prepared, false).await.unwrap_err().code,
        FaultCode::StaleRequest
    );
    fs::write(format!("{}:extra", folder.display()), b"directory ADS").unwrap();
    assert!(plan(&[&folder], &d).is_err());
    assert_eq!(
        fs::read(format!("{}:extra", folder.display())).unwrap(),
        b"directory ADS"
    );
    assert_eq!(fs::read(folder.join("sentinel")).unwrap(), b"original");
    assert_eq!(fs::read_dir(&d).unwrap().count(), 0);
    // Existing output remains untouched, including across prepare/execute.
    let other = c.join("other");
    fs::create_dir(&other).unwrap();
    fs::write(other.join("value"), b"source").unwrap();
    let prepared = plan(&[&other], &d).unwrap();
    fs::create_dir(d.join("other")).unwrap();
    fs::write(d.join("other/value"), b"destination").unwrap();
    assert_eq!(
        run(prepared, false).await.unwrap_err().code,
        FaultCode::DestinationExists
    );
    assert_eq!(fs::read(other.join("value")).unwrap(), b"source");
    assert_eq!(fs::read(d.join("other/value")).unwrap(), b"destination");
}

#[test]
#[ignore = "explicit C/D synthetic fixture required"]
fn native_tree_move_cancellation_and_new_directory_stream_after_first_item_preserve_remainder() {
    use std::sync::atomic::Ordering;
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            move_tree_sink::AFTER_MOVE.with(|h| *h.borrow_mut() = None);
        }
    }
    for (cancel_operation, after_name) in
        [(true, "01-first"), (false, "01-first"), (false, "02-last")]
    {
        let fixture = Fixture::new();
        let (c, d) = fixture.paths();
        let folder = c.join("source");
        fs::create_dir(&folder).unwrap();
        for name in ["01-first", "02-last"] {
            fs::write(folder.join(name), name.as_bytes()).unwrap();
        }
        let prepared = plan(&[&folder], &d).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let callback_cancel = cancel.clone();
        let stream = format!("{}:new-stream", folder.display());
        let callback_stream = stream.clone();
        move_tree_sink::AFTER_MOVE.with(|h| {
            *h.borrow_mut() = Some(Box::new(move |path| {
                if path.ends_with(after_name) {
                    if cancel_operation {
                        callback_cancel.store(true, Ordering::Release);
                    } else {
                        fs::write(&callback_stream, b"arrived during move").unwrap();
                    }
                }
            }))
        });
        let reset = Reset;
        let sta = execute::Apartment::initialize().unwrap();
        let report = sta.execute(prepared, 0, cancel, Arc::default()).unwrap();
        drop(sta);
        drop(reset);
        eprintln!("boundary cancellation={cancel_operation}: {report:?}");
        assert_ne!(report.status(), OperationStatus::Succeeded);
        assert_eq!(
            report.counts.changed,
            if after_name == "01-first" { 1 } else { 2 }
        );
        assert_eq!(report.evidence.cancellation_confirmed, cancel_operation);
        assert_eq!(fs::read(d.join("source/01-first")).unwrap(), b"01-first");
        assert_eq!(
            fs::read(if after_name == "01-first" {
                folder.join("02-last")
            } else {
                d.join("source/02-last")
            })
            .unwrap(),
            b"02-last"
        );
        if !cancel_operation {
            assert_eq!(fs::read(stream).unwrap(), b"arrived during move");
        }
        assert!(folder.exists());
    }
}
