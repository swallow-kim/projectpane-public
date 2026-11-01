use super::*;
use std::sync::Mutex;
use windows::{
    Win32::{
        Foundation::{DV_E_FORMATETC, E_FAIL, E_NOTIMPL},
        System::{
            Com::{FORMATETC, IAdviseSink, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA},
            Threading::GetCurrentThreadId,
        },
    },
    core::{BOOL, HRESULT, Ref, implement},
};

#[derive(Default, Debug)]
pub(crate) struct Trace {
    created: u32,
    pub(crate) dropped: u32,
    pub(crate) calls: Vec<(u32, u16, u32)>,
    pub(crate) fail_at: Option<usize>,
    pub(crate) notice_gate: Option<Arc<(Mutex<bool>, std::sync::Condvar)>>,
    pub(crate) block_at: Option<usize>,
}
#[implement(IDataObject)]
struct PrivateObject {
    paths: Vec<String>,
    trace: Arc<Mutex<Trace>>,
}
impl Drop for PrivateObject {
    fn drop(&mut self) {
        self.trace.lock().unwrap().dropped = unsafe { GetCurrentThreadId() };
    }
}
impl IDataObject_Impl for PrivateObject_Impl {
    fn GetData(&self, format: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
        let id = unsafe { (*format).cfFormat };
        if id == unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) } as u16 {
            return medium(2).map_err(|_| E_FAIL.into());
        }
        if id != 15 {
            return Err(DV_E_FORMATETC.into());
        }
        let mut bytes = vec![0u8; 20];
        bytes[..4].copy_from_slice(&20u32.to_le_bytes());
        bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
        for path in &self.paths {
            for unit in path.encode_utf16().chain([0]) {
                bytes.extend(unit.to_le_bytes());
            }
        }
        bytes.extend([0, 0]);
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) }?;
        let pointer = unsafe { GlobalLock(handle) };
        assert!(!pointer.is_null());
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.cast::<u8>(), bytes.len()) };
        let _ = unsafe { GlobalUnlock(handle) };
        Ok(STGMEDIUM {
            tymed: TYMED_HGLOBAL.0 as u32,
            u: STGMEDIUM_0 { hGlobal: handle },
            ..Default::default()
        })
    }
    fn GetDataHere(&self, _: *const FORMATETC, _: *mut STGMEDIUM) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn QueryGetData(&self, _: *const FORMATETC) -> HRESULT {
        DV_E_FORMATETC
    }
    fn GetCanonicalFormatEtc(&self, _: *const FORMATETC, _: *mut FORMATETC) -> HRESULT {
        E_NOTIMPL
    }
    fn SetData(
        &self,
        format: *const FORMATETC,
        medium: *const STGMEDIUM,
        release: BOOL,
    ) -> windows::core::Result<()> {
        assert!(!release.as_bool());
        let data = unsafe { &*medium };
        assert_eq!(data.tymed, TYMED_HGLOBAL.0 as u32);
        let handle = unsafe { data.u.hGlobal };
        assert!(unsafe { GlobalSize(handle) } >= 4);
        let pointer = unsafe { GlobalLock(handle) };
        assert!(!pointer.is_null());
        let value = unsafe { pointer.cast::<u32>().read_unaligned() };
        let _ = unsafe { GlobalUnlock(handle) };
        let mut trace = self.trace.lock().unwrap();
        trace.calls.push((
            unsafe { GetCurrentThreadId() },
            unsafe { (*format).cfFormat },
            value,
        ));
        // Simulate a provider that accepts the side effect but returns an error.
        if trace.fail_at == Some(trace.calls.len()) {
            return Err(E_FAIL.into());
        }
        let gate = if trace.block_at.is_none() || trace.block_at == Some(trace.calls.len()) {
            trace.notice_gate.clone()
        } else {
            None
        };
        drop(trace);
        if let Some(gate) = gate {
            let (lock, changed) = &*gate;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = changed.wait(released).unwrap();
            }
        }
        Ok(())
    }
    fn EnumFormatEtc(&self, _: u32) -> windows::core::Result<IEnumFORMATETC> {
        Err(E_NOTIMPL.into())
    }
    fn DAdvise(
        &self,
        _: *const FORMATETC,
        _: u32,
        _: Ref<IAdviseSink>,
    ) -> windows::core::Result<u32> {
        Err(E_NOTIMPL.into())
    }
    fn DUnadvise(&self, _: u32) -> windows::core::Result<()> {
        Err(E_NOTIMPL.into())
    }
    fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
        Err(E_NOTIMPL.into())
    }
}
pub(crate) struct Fixture {
    pub(crate) root: tempfile::TempDir,
    pub(crate) paths: Vec<String>,
    pub(crate) targets: Vec<String>,
    pub(crate) trace: Arc<Mutex<Trace>>,
    coordinator: Coordinator,
}
impl Fixture {
    pub(crate) fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        Self::in_root(root)
    }
    pub(crate) fn in_root(root: tempfile::TempDir) -> Self {
        fs::create_dir(root.path().join("source")).unwrap();
        fs::create_dir(root.path().join("target")).unwrap();
        fs::create_dir(root.path().join("journal")).unwrap();
        let mut paths = vec![];
        let mut targets = vec![];
        for name in ["합성 이동 첫째.txt", "합성 이동 둘째.txt"] {
            let path = root.path().join("source").join(name);
            fs::write(&path, b"owned synthetic bytes").unwrap();
            paths.push(path.to_str().unwrap().to_owned());
            targets.push(
                root.path()
                    .join("target")
                    .join(name)
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );
        }
        Self {
            root,
            paths,
            targets,
            trace: Arc::new(Mutex::new(Trace::default())),
            coordinator: Coordinator::default(),
        }
    }
    pub(crate) fn start(&self, fail_at: Option<u32>, lifetime: Duration) -> Result<Session, Fault> {
        let paths = self.paths.clone();
        let trace = self.trace.clone();
        self.coordinator.start(
            move || {
                trace.lock().unwrap().created = unsafe { GetCurrentThreadId() };
                Ok(PrivateObject { paths, trace }.into())
            },
            Journal {
                root: self.root.path().join("journal"),
                fail_at,
            },
            lifetime,
        )
    }
    fn move_item(&self, index: usize) {
        // Only known files below this owned TempDir are ever moved by tests.
        assert!(Path::new(&self.paths[index]).starts_with(self.root.path()));
        assert!(Path::new(&self.targets[index]).starts_with(self.root.path()));
        fs::rename(&self.paths[index], &self.targets[index]).unwrap();
    }
    fn recover(&self) -> Vec<Record> {
        let mut records: Vec<_> = fs::read_dir(self.root.path().join("journal"))
            .unwrap()
            .map(|e| Record::recover(&fs::read(e.unwrap().path()).unwrap()).unwrap())
            .collect();
        records.sort_by_key(|r| r.sequence);
        records
    }
    pub(crate) fn released(&self) {
        let trace = self.trace.lock().unwrap();
        assert_ne!(trace.created, 0);
        assert_eq!(trace.created, trace.dropped);
        assert!(trace.calls.iter().all(|call| call.0 == trace.created));
        assert!(!self.coordinator.active.load(Ordering::Acquire));
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if std::thread::panicking() {
            self.root.disable_cleanup(true);
            eprintln!(
                "preserved failed synthetic fixture: {}",
                self.root.path().display()
            );
        }
    }
}

#[test]
fn complete_optimized_move_notifies_none_then_paste_move_once_on_owner_sta() {
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    assert_eq!(session.snapshot.phase, Phase::Captured);
    assert!(
        f.start(None, Duration::from_secs(5)).is_err(),
        "one active data object"
    );
    let armed = session.arm(f.targets.clone()).unwrap();
    assert_eq!(armed.phase, Phase::Armed);
    assert!(
        session.arm(f.targets.clone()).is_err(),
        "duplicate execution rejected"
    );
    f.move_item(0);
    f.move_item(1);
    let result = session.finish().unwrap();
    assert_eq!(result.phase, Phase::Notified);
    session.wait();
    assert!(session.finish().is_err());
    f.released();
    let trace = f.trace.lock().unwrap();
    assert_eq!(
        trace.calls.iter().map(|c| (c.1, c.2)).collect::<Vec<_>>(),
        vec![
            (
                unsafe { RegisterClipboardFormatW(w!("Performed DropEffect")) } as u16,
                0
            ),
            (
                unsafe { RegisterClipboardFormatW(w!("Paste Succeeded")) } as u16,
                2
            )
        ]
    );
    for (from, to) in f.paths.iter().zip(&f.targets) {
        assert!(absent(from));
        assert_eq!(fs::read(to).unwrap(), b"owned synthetic bytes");
    }
    let records = f.recover();
    assert_eq!(records.len(), 5);
    assert!(records.iter().all(|r| !r.permits_replay()));
}
#[test]
fn partial_move_preserves_unmoved_sources_and_never_sends_batch_success() {
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    session.arm(f.targets.clone()).unwrap();
    f.move_item(0);
    let result = session.finish().unwrap();
    session.wait();
    f.released();
    assert_eq!(result.phase, Phase::ReconcileRequired);
    assert_eq!(
        result.items.iter().map(|i| &i.state).collect::<Vec<_>>(),
        vec![&ItemState::Moved, &ItemState::Unchanged]
    );
    assert!(f.trace.lock().unwrap().calls.is_empty());
    assert_eq!(fs::read(&f.paths[1]).unwrap(), b"owned synthetic bytes");
    assert!(!f.recover().last().unwrap().permits_replay());
}
#[test]
fn replaced_or_recreated_paths_remain_unknown_without_deletion_acknowledgement() {
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    session.arm(f.targets.clone()).unwrap();
    f.move_item(0);
    f.move_item(1);
    fs::write(&f.paths[0], b"new unrelated synthetic file").unwrap();
    let result = session.finish().unwrap();
    session.wait();
    assert_eq!(result.items[0].state, ItemState::Unknown);
    assert_eq!(result.phase, Phase::ReconcileRequired);
    assert!(f.trace.lock().unwrap().calls.is_empty());
    assert_eq!(
        fs::read(&f.paths[0]).unwrap(),
        b"new unrelated synthetic file"
    );
}
#[test]
fn journal_failures_never_issue_an_unrecorded_notice_or_retry_effects() {
    for fail in [2, 3, 4, 5] {
        let f = Fixture::new();
        let session = f.start(Some(fail), Duration::from_secs(5)).unwrap();
        if fail == 2 {
            assert!(session.arm(f.targets.clone()).is_err());
            assert!(session.finish().is_err());
            session.close();
        } else {
            session.arm(f.targets.clone()).unwrap();
            f.move_item(0);
            f.move_item(1);
            assert!(session.finish().is_err());
        }
        session.wait();
        f.released();
        assert_eq!(
            f.trace.lock().unwrap().calls.len(),
            match fail {
                2 | 3 => 0,
                4 => 1,
                5 => 2,
                _ => unreachable!(),
            }
        );
        assert!(f.recover().iter().all(|r| !r.permits_replay()));
    }
}
#[test]
fn provider_error_after_acceptance_is_terminal_and_does_not_replay() {
    for fail in [1, 2] {
        let f = Fixture::new();
        f.trace.lock().unwrap().fail_at = Some(fail);
        let session = f.start(None, Duration::from_secs(5)).unwrap();
        session.arm(f.targets.clone()).unwrap();
        f.move_item(0);
        f.move_item(1);
        assert!(session.finish().is_err());
        session.wait();
        assert!(session.finish().is_err());
        f.released();
        assert_eq!(f.trace.lock().unwrap().calls.len(), fail);
        assert!(!f.recover().last().unwrap().permits_replay());
    }
}
#[test]
fn close_expiry_and_dropped_handle_release_without_clipboard_or_success() {
    for expiry in [false, true] {
        let f = Fixture::new();
        let session = f
            .start(
                None,
                if expiry {
                    Duration::from_millis(50)
                } else {
                    Duration::from_secs(5)
                },
            )
            .unwrap();
        if !expiry {
            session.arm(f.targets.clone()).unwrap();
            f.move_item(0);
            session.close();
        }
        session.wait();
        f.released();
        assert!(f.trace.lock().unwrap().calls.is_empty());
        assert_eq!(f.recover().last().unwrap().phase, Phase::ReconcileRequired);
    }
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    drop(session);
    let deadline = Instant::now() + Duration::from_secs(3);
    while f.coordinator.active.load(Ordering::Acquire) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    f.released();
    assert!(f.trace.lock().unwrap().calls.is_empty());
}
#[test]
fn occupied_destination_and_stale_source_reject_arm_before_any_move() {
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    fs::write(&f.targets[0], b"existing target").unwrap();
    assert!(session.arm(f.targets.clone()).is_err());
    assert!(session.finish().is_err());
    session.close();
    session.wait();
    for path in &f.paths {
        assert_eq!(fs::read(path).unwrap(), b"owned synthetic bytes");
    }
    assert_eq!(fs::read(&f.targets[0]).unwrap(), b"existing target");
    assert!(f.trace.lock().unwrap().calls.is_empty());
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    fs::rename(&f.paths[0], f.root.path().join("retained-original.txt")).unwrap();
    fs::write(&f.paths[0], b"replacement").unwrap();
    assert!(session.arm(f.targets.clone()).is_err());
    session.close();
    session.wait();
    assert!(f.trace.lock().unwrap().calls.is_empty());
}
#[test]
fn recovery_rejects_malformed_oversized_unknown_version_and_never_authorizes_replay() {
    assert!(Record::recover(b"{").is_err());
    assert!(Record::recover(&vec![0; MAX_RECORD + 1]).is_err());
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    session.close();
    session.wait();
    let record = f.recover().pop().unwrap();
    let mut changed = serde_json::to_value(&record).unwrap();
    changed["version"] = 2.into();
    assert!(Record::recover(&serde_json::to_vec(&changed).unwrap()).is_err());
    changed["version"] = 1.into();
    changed["extra"] = true.into();
    assert!(Record::recover(&serde_json::to_vec(&changed).unwrap()).is_err());
    assert!(!record.permits_replay());
}

#[test]
fn locked_synthetic_source_keeps_its_bytes_and_prevents_whole_batch_ack() {
    use std::os::windows::fs::OpenOptionsExt;
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    session.arm(f.targets.clone()).unwrap();
    let locked = OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&f.paths[1])
        .unwrap();
    f.move_item(0);
    assert!(fs::rename(&f.paths[1], &f.targets[1]).is_err());
    let result = session.finish().unwrap();
    session.wait();
    assert_eq!(result.phase, Phase::ReconcileRequired);
    assert!(f.trace.lock().unwrap().calls.is_empty());
    drop(locked);
    assert_eq!(fs::read(&f.paths[1]).unwrap(), b"owned synthetic bytes");
    assert!(absent(&f.targets[1]));
}

#[test]
fn fresh_process_recovery_reads_evidence_without_replaying_files_or_notifications() {
    const KEY: &str = "PROJECTPANE_PRIVATE_MOVE_RECOVERY_FIXTURE";
    if let Some(root) = std::env::var_os(KEY) {
        let root = PathBuf::from(root);
        let records: Vec<_> = fs::read_dir(root.join("journal"))
            .unwrap()
            .map(|file| Record::recover(&fs::read(file.unwrap().path()).unwrap()).unwrap())
            .collect();
        assert!(
            records
                .iter()
                .any(|record| record.phase == Phase::ReconcileRequired)
        );
        assert!(records.iter().all(|record| !record.permits_replay()));
        return;
    }
    let f = Fixture::new();
    let session = f.start(None, Duration::from_secs(5)).unwrap();
    session.arm(f.targets.clone()).unwrap();
    f.move_item(0);
    session.close();
    session.wait();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "platform::explorer::external_move::tests::fresh_process_recovery_reads_evidence_without_replaying_files_or_notifications", "--nocapture"])
        .env(KEY, f.root.path()).output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(absent(&f.paths[0]));
    assert_eq!(fs::read(&f.targets[0]).unwrap(), b"owned synthetic bytes");
    assert_eq!(fs::read(&f.paths[1]).unwrap(), b"owned synthetic bytes");
    assert!(absent(&f.targets[1]));
    assert!(f.trace.lock().unwrap().calls.is_empty());
}
