//! Disabled external optimized-move protocol. Compiled only in the test target.
//! This module never moves/deletes files or obtains the user's clipboard.
//! The caller must execute a separately authorized, journaled operation after arm.
pub(crate) mod tests;

use super::{OleApartment, format, native_error, read_file_paths};
use crate::{
    domain::{EntryKind, Fault, FaultCode, operations::OperationSource},
    platform::file_operations::capture_source,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
use windows::{
    Win32::System::{
        Com::{IDataObject, STGMEDIUM, STGMEDIUM_0, TYMED_HGLOBAL},
        DataExchange::RegisterClipboardFormatW,
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        Ole::ReleaseStgMedium,
    },
    core::w,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
    PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_QUIT,
};

const MAX_RECORD: usize = 256 * 1024;
const MAX_ITEMS: usize = 128;
fn invalid() -> Fault {
    Fault::new(FaultCode::InvalidState)
}

fn new_token() -> Result<String, Fault> {
    let guid = unsafe { windows::Win32::System::Com::CoCreateGuid() }.map_err(native_error)?;
    Ok(format!("{guid:?}").to_lowercase())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ItemState {
    Unchanged,
    Moved,
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Item {
    source: OperationSource,
    source_parent: OperationSource,
    destination: Option<String>,
    destination_parent: Option<OperationSource>,
    state: ItemState,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Phase {
    Captured,
    Armed,
    ReconcileRequired,
    BeforeNotice,
    PerformedConfirmed,
    Notified,
    NoticeUncertain,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    version: u32,
    session: String,
    operation: String,
    sequence: u32,
    phase: Phase,
    items: Vec<Item>,
}
impl Record {
    pub(crate) fn notified(&self) -> bool {
        self.phase == Phase::Notified
    }
    // Recovery is observation only. Even Notified is not a replay capability.
    fn recover(bytes: &[u8]) -> Result<Self, Fault> {
        if bytes.len() > MAX_RECORD {
            return Err(invalid());
        }
        let record: Self = serde_json::from_slice(bytes).map_err(|_| invalid())?;
        if record.version != 1
            || !(1..=6).contains(&record.sequence)
            || record.items.is_empty()
            || record.items.len() > MAX_ITEMS
            || !crate::domain::operations::valid_token(&record.session)
            || !crate::domain::operations::valid_token(&record.operation)
        {
            return Err(invalid());
        }
        for item in &record.items {
            crate::domain::AbsolutePath::new(item.source.path.clone())?;
            if let Some(path) = &item.destination {
                crate::domain::AbsolutePath::new(path.clone())?;
            }
        }
        Ok(record)
    }
    fn permits_replay(&self) -> bool {
        false
    }
}
struct Journal {
    root: PathBuf,
    fail_at: Option<u32>,
}
impl Journal {
    fn persist(&self, record: &mut Record, phase: Phase) -> Result<(), Fault> {
        record.phase = phase;
        record.sequence += 1;
        if self.fail_at == Some(record.sequence) {
            return Err(Fault::new(FaultCode::IoError));
        }
        let bytes = serde_json::to_vec(record).map_err(|_| invalid())?;
        Record::recover(&bytes)?;
        let name = format!("{}-{}.json", record.session, record.sequence);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join(name))
            .map_err(Fault::from)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(Fault::from)
    }
}

// HGLOBAL remains owned by the caller. SetData(FALSE) must copy it if retained.
fn medium(value: u32) -> Result<STGMEDIUM, Fault> {
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, 4) }.map_err(native_error)?;
    let mut result = STGMEDIUM {
        tymed: TYMED_HGLOBAL.0 as u32,
        u: STGMEDIUM_0 { hGlobal: handle },
        ..Default::default()
    };
    let pointer = unsafe { GlobalLock(handle) };
    if pointer.is_null() {
        unsafe { ReleaseStgMedium(&mut result) };
        return Err(invalid());
    }
    unsafe { pointer.cast::<u32>().write_unaligned(value) };
    let _ = unsafe { GlobalUnlock(handle) };
    Ok(result)
}
fn read_effect(object: &IDataObject) -> Result<u32, Fault> {
    let id = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    if id == 0 {
        return Err(invalid());
    }
    let mut data = unsafe { object.GetData(&format(id as u16)) }.map_err(native_error)?;
    let result = (|| {
        if data.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(invalid());
        }
        let handle = unsafe { data.u.hGlobal };
        if unsafe { GlobalSize(handle) } < 4 {
            return Err(invalid());
        }
        let pointer = unsafe { GlobalLock(handle) };
        if pointer.is_null() {
            return Err(invalid());
        }
        let value = unsafe { pointer.cast::<u32>().read_unaligned() };
        let _ = unsafe { GlobalUnlock(handle) };
        Ok(value)
    })();
    unsafe { ReleaseStgMedium(&mut data) };
    result
}
fn notify(object: &IDataObject, paste: bool) -> Result<(), Fault> {
    let name = if paste {
        w!("Paste Succeeded")
    } else {
        w!("Performed DropEffect")
    };
    let id = unsafe { RegisterClipboardFormatW(name) };
    if id == 0 {
        return Err(invalid());
    }
    // Optimized move only: NONE first, then MOVE. Never request source deletion
    // by sending Performed DropEffect=MOVE, including on partial failures.
    let mut data = medium(if paste { 2 } else { 0 })?;
    let result = unsafe { object.SetData(&format(id as u16), &data, false) }.map_err(native_error);
    unsafe { ReleaseStgMedium(&mut data) };
    result
}
fn parent(path: &str) -> Result<OperationSource, Fault> {
    capture_source(
        Path::new(path)
            .parent()
            .and_then(Path::to_str)
            .ok_or_else(invalid)?,
    )
}
fn absent(path: &str) -> bool {
    matches!(fs::symlink_metadata(path), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}
fn observe(item: &Item) -> ItemState {
    let Some(destination) = &item.destination else {
        return ItemState::Unchanged;
    };
    if parent(&item.source.path).ok().as_ref() != Some(&item.source_parent)
        || parent(destination).ok().as_ref() != item.destination_parent.as_ref()
    {
        return ItemState::Unknown;
    }
    let source = capture_source(&item.source.path).ok();
    let target = capture_source(destination).ok();
    if source.as_ref() == Some(&item.source) && absent(destination) {
        return ItemState::Unchanged;
    }
    if absent(&item.source.path)
        && target.is_some_and(|value| {
            value.kind == item.source.kind
                && value.volume == item.source.volume
                && value.file_index == item.source.file_index
        })
    {
        return ItemState::Moved;
    }
    ItemState::Unknown
}

enum Command {
    Arm(String, Vec<String>, SyncSender<Result<Record, Fault>>),
    Finish(SyncSender<Result<Record, Fault>>),
}
#[derive(Default)]
struct Coordinator {
    active: Arc<AtomicBool>,
}
struct Lease(Arc<AtomicBool>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}
pub(crate) struct Session {
    commands: SyncSender<Command>,
    cancelled: Arc<AtomicBool>,
    finished: Receiver<()>,
    snapshot: Record,
    journal_root: PathBuf,
    bound: Mutex<Option<Record>>,
    finish_started: AtomicBool,
    notices: Arc<AtomicU8>,
}
#[derive(Clone)]
pub(crate) struct Revocation {
    cancelled: Arc<AtomicBool>,
    notices: Arc<AtomicU8>,
}
impl Revocation {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        self.notices.store(5, Ordering::Release);
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        self.close();
    }
}
impl Session {
    pub(crate) fn revocation(&self) -> Revocation {
        Revocation {
            cancelled: self.cancelled.clone(),
            notices: self.notices.clone(),
        }
    }
    fn arm(&self, destinations: Vec<String>) -> Result<Record, Fault> {
        self.bind(self.snapshot.operation.clone(), destinations)
    }
    pub(crate) fn sources(&self) -> Vec<OperationSource> {
        self.snapshot
            .items
            .iter()
            .map(|item| item.source.clone())
            .collect()
    }
    pub(crate) fn journal_root(&self) -> &Path {
        &self.journal_root
    }
    pub(crate) fn cancellation(&self) -> Arc<AtomicBool> {
        self.cancelled.clone()
    }
    pub(crate) fn bind(
        &self,
        operation: String,
        destinations: Vec<String>,
    ) -> Result<Record, Fault> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Arm(operation, destinations, tx))
            .map_err(|_| invalid())?;
        let record = rx.recv_timeout(Duration::from_secs(5)).map_err(|_| {
            self.close();
            invalid()
        })??;
        *self.bound.lock().map_err(|_| invalid())? = Some(record.clone());
        Ok(record)
    }
    pub(crate) fn finish(&self) -> Result<Record, Fault> {
        self.finish_with_timeout(Duration::from_secs(5))
    }
    pub(crate) fn finish_with_timeout(&self, timeout: Duration) -> Result<Record, Fault> {
        self.finish_started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| invalid())?;
        let (tx, rx) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Finish(tx))
            .map_err(|_| invalid())?;
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.close(); // permanently revoke subsequent notices; never retry
                let mut record = self
                    .bound
                    .lock()
                    .map_err(|_| invalid())?
                    .clone()
                    .ok_or_else(invalid)?;
                record.phase = Phase::NoticeUncertain;
                record.sequence = 6;
                for item in &mut record.items {
                    item.state = ItemState::Unknown;
                }
                let bytes = serde_json::to_vec(&record).map_err(|_| invalid())?;
                Record::recover(&bytes)?;
                let path = self
                    .journal_root
                    .join(format!("{}-timeout.json", record.session));
                let mut file = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(path)
                    .map_err(Fault::from)?;
                file.write_all(&bytes)
                    .and_then(|()| file.sync_all())
                    .map_err(Fault::from)?;
                Err(Fault::new(FaultCode::WorkerUnavailable))
            }
            Err(_) => Err(invalid()),
        }
    }
    pub(crate) fn close(&self) {
        self.revocation().cancel();
    }
    pub(crate) fn wait(&self) {
        self.finished
            .recv_timeout(Duration::from_secs(5))
            .expect("STA releases object and apartment");
    }
    pub(crate) fn wait_until_released(&self) -> Result<(), Fault> {
        // A COM timeout is not revocation. The integration keeps file/journal
        // guards and admission until the actual STA release, including a hung
        // provider. Product activation needs an isolation/UX policy for that case.
        self.finished
            .recv()
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))
    }
}
impl Coordinator {
    fn start(
        &self,
        factory: impl FnOnce() -> Result<IDataObject, Fault> + Send + 'static,
        journal: Journal,
        lifetime: Duration,
    ) -> Result<Session, Fault> {
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Fault::new(FaultCode::Busy))?;
        let lease = Lease(self.active.clone());
        let (commands, input) = mpsc::sync_channel(4);
        let (ready_tx, ready) = mpsc::sync_channel(1);
        let (finished_tx, finished) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancel = cancelled.clone();
        let notices = Arc::new(AtomicU8::new(0));
        let notice_state = notices.clone();
        let journal_root = journal.root.clone();
        std::thread::Builder::new()
            .name("projectpane-move-prototype".into())
            .spawn(move || {
                {
                    let _lease = lease; // dropped after object and OLE, even on error
                    let result = (|| {
                        let _ole = OleApartment::new()?;
                        let object = factory()?; // created/acquired and released on this STA only
                        if read_effect(&object)? != 2 {
                            return Err(invalid());
                        }
                        let paths = read_file_paths(&object)?;
                        if paths.len() > MAX_ITEMS {
                            return Err(invalid());
                        }
                        let mut items = Vec::new();
                        for path in paths {
                            let source = capture_source(&path)?;
                            if source.kind != EntryKind::File
                                || items.iter().any(|item: &Item| {
                                    item.source.volume == source.volume
                                        && item.source.file_index == source.file_index
                                })
                            {
                                return Err(invalid());
                            }
                            items.push(Item {
                                source_parent: parent(&path)?,
                                source,
                                destination: None,
                                destination_parent: None,
                                state: ItemState::Unchanged,
                            });
                        }
                        let mut record = Record {
                            version: 1,
                            session: new_token()?,
                            operation: new_token()?,
                            sequence: 0,
                            phase: Phase::Captured,
                            items,
                        };
                        journal.persist(&mut record, Phase::Captured)?;
                        if ready_tx.send(Ok(record.clone())).is_err() {
                            return Ok(());
                        }
                        run(
                            &object,
                            &input,
                            &cancel,
                            &journal,
                            &mut record,
                            lifetime,
                            &notice_state,
                        );
                        Ok(())
                    })();
                    if let Err(error) = result {
                        let _ = ready_tx.try_send(Err(error));
                    }
                }
                let _ = finished_tx.send(());
            })
            .map_err(|_| Fault::new(FaultCode::WorkerUnavailable))?;
        let snapshot = ready.recv_timeout(Duration::from_secs(5)).map_err(|_| {
            cancelled.store(true, Ordering::Release);
            invalid()
        })??;
        Ok(Session {
            commands,
            cancelled,
            finished,
            snapshot,
            journal_root,
            bound: Mutex::new(None),
            finish_started: AtomicBool::new(false),
            notices,
        })
    }
}
fn run(
    object: &IDataObject,
    input: &Receiver<Command>,
    cancelled: &AtomicBool,
    journal: &Journal,
    record: &mut Record,
    lifetime: Duration,
    notices: &AtomicU8,
) {
    let deadline = Instant::now() + lifetime;
    loop {
        if cancelled.load(Ordering::Acquire) || Instant::now() >= deadline {
            cancelled.store(true, Ordering::Release);
            for item in &mut record.items {
                item.state = observe(item);
            }
            let _ = journal.persist(record, Phase::ReconcileRequired);
            return;
        }
        match input.try_recv() {
            Ok(Command::Arm(operation, destinations, reply)) => {
                let result = arm(record, operation, destinations, journal);
                // Invalid or duplicate requests do not consume a live operation.
                let _ = reply.send(result);
            }
            Ok(Command::Finish(reply)) => {
                if record.phase != Phase::Armed {
                    let _ = reply.send(Err(invalid()));
                    continue;
                }
                let result = finish(object, record, journal, cancelled, notices);
                let _ = reply.send(result);
                return;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                cancelled.store(true, Ordering::Release);
                continue;
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
        // Pump COM calls while idle. No blocking recv, no native input injection.
        unsafe {
            let waited = MsgWaitForMultipleObjectsEx(
                0,
                std::ptr::null(),
                20,
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            );
            if waited == windows_sys::Win32::Foundation::WAIT_FAILED {
                cancelled.store(true, Ordering::Release);
                continue;
            }
            let mut message = MSG::default();
            for _ in 0..64 {
                if PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) == 0 {
                    break;
                }
                if message.message == WM_QUIT {
                    cancelled.store(true, Ordering::Release);
                    break;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}
fn arm(
    record: &mut Record,
    operation: String,
    paths: Vec<String>,
    journal: &Journal,
) -> Result<Record, Fault> {
    if record.phase != Phase::Captured
        || paths.len() != record.items.len()
        || !crate::domain::operations::valid_token(&operation)
    {
        return Err(invalid());
    }
    let mut next = record.clone();
    next.operation = operation;
    let mut destinations = std::collections::HashSet::new();
    for (item, path) in next.items.iter_mut().zip(paths) {
        let destination_parent = parent(&path)?;
        if capture_source(&item.source.path)? != item.source
            || parent(&item.source.path)? != item.source_parent
            || destination_parent.volume != item.source.volume
            || !absent(&path)
            || !destinations.insert(path.to_lowercase())
        {
            return Err(invalid());
        }
        crate::domain::AbsolutePath::new(path.clone())?;
        item.destination = Some(path);
        item.destination_parent = Some(destination_parent);
    }
    // An arm acknowledgement is issued only after the intent record is flushed.
    journal.persist(&mut next, Phase::Armed)?;
    *record = next;
    Ok(record.clone())
}
fn finish(
    object: &IDataObject,
    record: &mut Record,
    journal: &Journal,
    cancelled: &AtomicBool,
    notices: &AtomicU8,
) -> Result<Record, Fault> {
    for item in &mut record.items {
        item.state = observe(item);
    }
    if !record
        .items
        .iter()
        .all(|item| item.state == ItemState::Moved)
    {
        journal.persist(record, Phase::ReconcileRequired)?;
        return Ok(record.clone());
    }
    journal.persist(record, Phase::BeforeNotice)?;
    // CAS is the dispatch linearization point. Timeout revokes every future
    // dispatch, but cannot retract a COM call already admitted/in flight.
    if cancelled.load(Ordering::Acquire)
        || notices
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        journal.persist(record, Phase::ReconcileRequired)?;
        return Ok(record.clone());
    }
    notify(object, false)?; // after any error: terminal, no automatic retry
    journal.persist(record, Phase::PerformedConfirmed)?;
    if cancelled.load(Ordering::Acquire)
        || notices
            .compare_exchange(1, 2, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        journal.persist(record, Phase::ReconcileRequired)?;
        return Ok(record.clone());
    }
    if notices
        .compare_exchange(2, 3, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        journal.persist(record, Phase::ReconcileRequired)?;
        return Ok(record.clone());
    }
    notify(object, true)?;
    let confirmed = notices
        .compare_exchange(3, 4, Ordering::AcqRel, Ordering::Acquire)
        .is_ok();
    journal.persist(
        record,
        if !confirmed || cancelled.load(Ordering::Acquire) {
            Phase::NoticeUncertain
        } else {
            Phase::Notified
        },
    )?;
    Ok(record.clone())
}
