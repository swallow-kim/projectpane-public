use super::*;
use crate::persistence::recovery::{read_journal, tests::record};
use std::fs;
use windows_sys::Win32::Storage::FileSystem::{DELETE, FILE_FLAG_BACKUP_SEMANTICS};

fn setup() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    File::create(root.path().join("journal.lock")).unwrap();
    root
}

fn phase(name: &str) -> RecoveryRecord {
    RecoveryRecord::parse(&serde_json::to_vec(&record("lease-proof", name)).unwrap()).unwrap()
}

#[test]
fn lease_blocks_competitors_between_acknowledged_phases_until_drop() {
    let root = setup();
    let lease = JournalLease::open(root.path()).unwrap();
    let (lease, _ack) = lease.append(&phase("ready")).unwrap();

    assert!(matches!(
        append_phase(root.path(), &phase("preserve_requested")),
        Err(AppendFailure::NotCreated(io::ErrorKind::WouldBlock))
    ));
    assert!(read_journal(root.path()).is_err());
    let (lease, _ack) = lease.append(&phase("preserve_requested")).unwrap();
    assert!(read_journal(root.path()).is_err());
    drop(lease);

    assert_eq!(
        read_journal(root.path()).unwrap()[0].phase(),
        RecoveryPhase::PreserveRequested
    );
}

#[test]
fn lease_holds_directory_and_lock_delete_guards_between_appends() {
    let root = setup();
    let lease = JournalLease::open(root.path()).unwrap();
    let delete = |path: &Path, flags| {
        OpenOptions::new()
            .access_mode(DELETE)
            .share_mode(7)
            .custom_flags(flags)
            .open(path)
    };

    assert!(delete(root.path(), FILE_FLAG_BACKUP_SEMANTICS).is_err());
    assert!(delete(&root.path().join("journal.lock"), 0).is_err());
    drop(lease);

    assert!(delete(root.path(), FILE_FLAG_BACKUP_SEMANTICS).is_ok());
    assert!(delete(&root.path().join("journal.lock"), 0).is_ok());
}

#[test]
fn failed_next_phase_releases_lease_without_removing_prior_acknowledged_bytes() {
    let root = setup();
    let (lease, _ack) = JournalLease::open(root.path())
        .unwrap()
        .append(&phase("ready"))
        .unwrap();
    let prior = fs::read(root.path().join("lease-proof.ready.json")).unwrap();

    assert!(matches!(
        lease.append(&phase("install_observed")),
        Err(AppendFailure::NotCreated(_))
    ));

    assert_eq!(read_journal(root.path()).unwrap().len(), 1);
    assert_eq!(
        fs::read(root.path().join("lease-proof.ready.json")).unwrap(),
        prior
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
}

#[test]
fn failed_write_releases_lease_and_keeps_partial_evidence() {
    let root = setup();
    let lease = JournalLease::open(root.path()).unwrap();

    let result = lease.append_with(&phase("ready"), |file, bytes| {
        file.write_all(&bytes[..7])?;
        Err(io::Error::other("injected_partial"))
    });

    assert!(matches!(result, Err(AppendFailure::Unacknowledged(_))));
    let path = root.path().join("lease-proof.ready.json");
    let bytes = fs::read(&path).unwrap();
    assert_eq!(bytes.len(), 7);
    let next = JournalLease::open(root.path()).unwrap();
    assert!(matches!(
        next.append(&phase("ready")),
        Err(AppendFailure::NotCreated(_))
    ));
    assert_eq!(fs::read(path).unwrap(), bytes);
}

#[test]
fn later_append_revalidates_claims_changed_by_a_noncooperating_writer() {
    let root = setup();
    let (lease, _ack) = JournalLease::open(root.path())
        .unwrap()
        .append(&phase("ready"))
        .unwrap();
    let prior = root.path().join("lease-proof.ready.json");
    fs::write(&prior, b"corrupt").unwrap();

    let result = lease.append(&phase("preserve_requested"));

    assert!(matches!(result, Err(AppendFailure::NotCreated(_))));
    assert_eq!(fs::read(prior).unwrap(), b"corrupt");
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    assert!(JournalLease::open(root.path()).is_ok());
}
